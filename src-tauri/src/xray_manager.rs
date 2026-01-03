use crate::xray_config_generator::generate_xray_config;
use crate::xray_protocol_parser::{parse_proxy_url, XrayProxyConfig};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Information about a running Xray instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XrayInstance {
  pub id: String,
  pub config: XrayProxyConfig,
  pub local_socks_port: u16,
  pub local_http_port: u16,
  pub config_path: PathBuf,
  #[serde(skip)]
  pub pid: Option<u32>,
}

/// Manager for Xray-core instances
pub struct XrayManager {
  instances: Mutex<HashMap<String, XrayInstance>>,
  processes: Mutex<HashMap<String, Child>>,
  base_dirs: BaseDirs,
}

impl XrayManager {
  pub fn new() -> Self {
    let base_dirs = BaseDirs::new().expect("Failed to get base directories");
    Self {
      instances: Mutex::new(HashMap::new()),
      processes: Mutex::new(HashMap::new()),
      base_dirs,
    }
  }

  /// Get the Xray data directory
  fn get_xray_dir(&self) -> PathBuf {
    let mut path = self.base_dirs.data_local_dir().to_path_buf();
    path.push(if cfg!(debug_assertions) {
      "DonutBrowserDev"
    } else {
      "DonutBrowser"
    });
    path.push("xray");
    path
  }

  /// Get the path to store Xray configs
  fn get_configs_dir(&self) -> PathBuf {
    self.get_xray_dir().join("configs")
  }

  /// Get the Xray binary path based on platform
  pub fn get_xray_binary_path(&self) -> PathBuf {
    let xray_dir = self.get_xray_dir();

    #[cfg(target_os = "windows")]
    let binary_name = "xray.exe";
    #[cfg(not(target_os = "windows"))]
    let binary_name = "xray";

    xray_dir.join("bin").join(binary_name)
  }

  /// Check if Xray binary is available
  pub fn is_xray_available(&self) -> bool {
    self.get_xray_binary_path().exists()
  }

  /// Find an available port for Xray local proxy
  async fn find_available_port(&self, start_port: u16) -> Result<u16, String> {
    use tokio::net::TcpListener;

    for port in start_port..start_port + 1000 {
      if TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .is_ok()
      {
        return Ok(port);
      }
    }
    Err("No available port found".to_string())
  }

  /// Start an Xray instance from a proxy URL
  pub async fn start_from_url(&self, url: &str) -> Result<XrayInstance, String> {
    let config = parse_proxy_url(url)?;
    self.start_instance(config).await
  }

  /// Start an Xray instance with the given configuration
  pub async fn start_instance(&self, config: XrayProxyConfig) -> Result<XrayInstance, String> {
    if !self.is_xray_available() {
      return Err("Xray binary not found. Please ensure Xray-core is installed.".to_string());
    }

    // Generate unique ID for this instance
    let instance_id = format!(
      "xray_{}_{}",
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs(),
      uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );

    // Find available ports
    let socks_port = self.find_available_port(10808).await?;
    let http_port = socks_port + 1;

    // Generate Xray config
    let xray_config = generate_xray_config(&config, socks_port);

    // Write config to file
    let configs_dir = self.get_configs_dir();
    fs::create_dir_all(&configs_dir)
      .map_err(|e| format!("Failed to create configs directory: {}", e))?;

    let config_path = configs_dir.join(format!("{}.json", instance_id));
    let config_json = serde_json::to_string_pretty(&xray_config)
      .map_err(|e| format!("Failed to serialize Xray config: {}", e))?;

    fs::write(&config_path, &config_json)
      .map_err(|e| format!("Failed to write Xray config: {}", e))?;

    // Start Xray process
    let xray_binary = self.get_xray_binary_path();
    let mut child = Command::new(&xray_binary)
      .arg("run")
      .arg("-config")
      .arg(&config_path)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true)
      .spawn()
      .map_err(|e| format!("Failed to start Xray process: {}", e))?;

    let pid = child.id();

    // Wait for Xray to start and check for errors
    let stderr = child.stderr.take();
    if let Some(stderr) = stderr {
      let mut reader = BufReader::new(stderr).lines();

      // Read first few lines to check for startup errors
      let timeout = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Ok(Some(line)) = reader.next_line().await {
          log::debug!("Xray[{}]: {}", instance_id, line);
          if line.contains("started") || line.contains("listening") {
            return Ok(());
          }
          if line.contains("error") || line.contains("failed") {
            return Err(format!("Xray startup error: {}", line));
          }
        }
        Ok(())
      })
      .await;

      match timeout {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
          let _ = child.kill().await;
          let _ = fs::remove_file(&config_path);
          return Err(e);
        }
        Err(_) => {
          // Timeout is okay, Xray might not output anything on success
        }
      }
    }

    // Wait a bit for Xray to bind to ports
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify the port is listening
    use tokio::net::TcpStream;
    match TcpStream::connect(format!("127.0.0.1:{}", socks_port)).await {
      Ok(_) => {}
      Err(_) => {
        let _ = child.kill().await;
        let _ = fs::remove_file(&config_path);
        return Err(format!(
          "Xray failed to bind to port {}. Check if another process is using it.",
          socks_port
        ));
      }
    }

    let instance = XrayInstance {
      id: instance_id.clone(),
      config,
      local_socks_port: socks_port,
      local_http_port: http_port,
      config_path: config_path.clone(),
      pid,
    };

    // Store instance and process
    {
      let mut instances = self.instances.lock().unwrap();
      instances.insert(instance_id.clone(), instance.clone());
    }
    {
      let mut processes = self.processes.lock().unwrap();
      processes.insert(instance_id, child);
    }

    log::info!(
      "Started Xray instance on SOCKS5://127.0.0.1:{} and HTTP://127.0.0.1:{}",
      socks_port,
      http_port
    );

    Ok(instance)
  }

  /// Stop an Xray instance by ID
  pub async fn stop_instance(&self, instance_id: &str) -> Result<bool, String> {
    // Remove and kill the process
    let mut child = {
      let mut processes = self.processes.lock().unwrap();
      processes.remove(instance_id)
    };

    if let Some(ref mut child) = child {
      let _ = child.kill().await;
    }

    // Remove instance info and config file
    let instance = {
      let mut instances = self.instances.lock().unwrap();
      instances.remove(instance_id)
    };

    if let Some(instance) = instance {
      let _ = fs::remove_file(&instance.config_path);
      log::info!("Stopped Xray instance: {}", instance_id);
      Ok(true)
    } else {
      Ok(false)
    }
  }

  /// Stop all Xray instances
  pub async fn stop_all(&self) -> Result<(), String> {
    let instance_ids: Vec<String> = {
      let instances = self.instances.lock().unwrap();
      instances.keys().cloned().collect()
    };

    for id in instance_ids {
      let _ = self.stop_instance(&id).await;
    }

    Ok(())
  }

  /// Get information about a running instance
  pub fn get_instance(&self, instance_id: &str) -> Option<XrayInstance> {
    let instances = self.instances.lock().unwrap();
    instances.get(instance_id).cloned()
  }

  /// Get all running instances
  pub fn get_all_instances(&self) -> Vec<XrayInstance> {
    let instances = self.instances.lock().unwrap();
    instances.values().cloned().collect()
  }

  /// Check if an instance is still running
  pub fn is_instance_running(&self, instance_id: &str) -> bool {
    let processes = self.processes.lock().unwrap();
    if let Some(child) = processes.get(instance_id) {
      // Check if process is still running by trying to get its ID
      child.id().is_some()
    } else {
      false
    }
  }

  /// Clean up dead instances
  pub async fn cleanup_dead_instances(&self) -> Vec<String> {
    let mut dead_ids = Vec::new();

    // Find dead instances
    {
      let processes = self.processes.lock().unwrap();
      let instances = self.instances.lock().unwrap();

      for (id, _instance) in instances.iter() {
        if let Some(child) = processes.get(id) {
          if child.id().is_none() {
            dead_ids.push(id.clone());
          }
        } else {
          dead_ids.push(id.clone());
        }
      }
    }

    // Clean up dead instances
    for id in &dead_ids {
      let _ = self.stop_instance(id).await;
    }

    dead_ids
  }
}

impl Default for XrayManager {
  fn default() -> Self {
    Self::new()
  }
}

// Global Xray manager instance
lazy_static::lazy_static! {
    pub static ref XRAY_MANAGER: XrayManager = XrayManager::new();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_xray_manager_creation() {
    let manager = XrayManager::new();
    assert!(!manager.is_xray_available()); // Binary won't be present in tests
  }

  #[test]
  fn test_xray_dir_paths() {
    let manager = XrayManager::new();
    let xray_dir = manager.get_xray_dir();
    let binary_path = manager.get_xray_binary_path();
    let configs_dir = manager.get_configs_dir();

    assert!(xray_dir.ends_with("xray") || xray_dir.to_string_lossy().contains("xray"));
    assert!(
      binary_path.to_string_lossy().contains("xray")
        || binary_path.to_string_lossy().contains("xray.exe")
    );
    assert!(configs_dir.ends_with("configs"));
  }
}
