use crate::xray_config_generator::generate_xray_config;
use crate::xray_protocol_parser::{parse_proxy_url, XrayProxyConfig};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::ShellExt;

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
  processes: Mutex<HashMap<String, CommandChild>>,
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
  pub async fn start_from_url(
    &self,
    app_handle: &AppHandle,
    url: &str,
  ) -> Result<XrayInstance, String> {
    let config = parse_proxy_url(url)?;
    self.start_instance(app_handle, config).await
  }

  /// Start an Xray instance with the given configuration
  pub async fn start_instance(
    &self,
    app_handle: &AppHandle,
    config: XrayProxyConfig,
  ) -> Result<XrayInstance, String> {
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

    // Start Xray process using Tauri sidecar
    let sidecar_cmd = app_handle
      .shell()
      .sidecar("xray")
      .map_err(|e| format!("Failed to create Xray sidecar: {}", e))?
      .arg("run")
      .arg("-config")
      .arg(config_path.to_string_lossy().to_string());

    let (mut rx, child) = sidecar_cmd
      .spawn()
      .map_err(|e| format!("Failed to spawn Xray process: {}", e))?;

    let pid = child.pid();

    // Wait for Xray to start by checking stderr/stdout for startup messages
    let instance_id_clone = instance_id.clone();
    let startup_check = tokio::time::timeout(std::time::Duration::from_secs(3), async {
      use tauri_plugin_shell::process::CommandEvent;
      while let Some(event) = rx.recv().await {
        match event {
          CommandEvent::Stdout(line) => {
            let line_str = String::from_utf8_lossy(&line);
            log::debug!("Xray[{}] stdout: {}", instance_id_clone, line_str);
            if line_str.contains("started") || line_str.contains("listening") {
              return Ok(());
            }
          }
          CommandEvent::Stderr(line) => {
            let line_str = String::from_utf8_lossy(&line);
            log::debug!("Xray[{}] stderr: {}", instance_id_clone, line_str);
            if line_str.contains("error") || line_str.contains("failed") {
              return Err(format!("Xray startup error: {}", line_str));
            }
            if line_str.contains("started") || line_str.contains("listening") {
              return Ok(());
            }
          }
          CommandEvent::Error(err) => {
            return Err(format!("Xray error: {}", err));
          }
          CommandEvent::Terminated(_) => {
            return Err("Xray process terminated unexpectedly".to_string());
          }
          _ => {}
        }
      }
      Ok(())
    })
    .await;

    match startup_check {
      Ok(Ok(())) => {}
      Ok(Err(e)) => {
        let _ = child.kill();
        let _ = fs::remove_file(&config_path);
        return Err(e);
      }
      Err(_) => {
        // Timeout is okay, Xray might not output anything on success
      }
    }

    // Wait a bit for Xray to bind to ports
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify the port is listening
    use tokio::net::TcpStream;
    match TcpStream::connect(format!("127.0.0.1:{}", socks_port)).await {
      Ok(_) => {}
      Err(_) => {
        let _ = child.kill();
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
      pid: Some(pid),
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
  pub fn stop_instance(&self, instance_id: &str) -> Result<bool, String> {
    // Remove and kill the process
    let child = {
      let mut processes = self.processes.lock().unwrap();
      processes.remove(instance_id)
    };

    if let Some(child) = child {
      let _ = child.kill();
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
  pub fn stop_all(&self) -> Result<(), String> {
    let instance_ids: Vec<String> = {
      let instances = self.instances.lock().unwrap();
      instances.keys().cloned().collect()
    };

    for id in instance_ids {
      let _ = self.stop_instance(&id);
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
    let instances = manager.get_all_instances();
    assert!(instances.is_empty());
  }

  #[test]
  fn test_xray_dir_paths() {
    let manager = XrayManager::new();
    let xray_dir = manager.get_xray_dir();
    let configs_dir = manager.get_configs_dir();

    assert!(xray_dir.ends_with("xray") || xray_dir.to_string_lossy().contains("xray"));
    assert!(configs_dir.ends_with("configs"));
  }
}
