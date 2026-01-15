use crate::xray_config::{generate_xray_config, is_xray_protocol};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::sync::Mutex;

/// Xray release info from GitHub API
#[derive(Debug, Clone, Deserialize)]
struct GitHubRelease {
  tag_name: String,
  #[allow(dead_code)]
  assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct GitHubAsset {
  name: String,
  browser_download_url: String,
}

/// Xray process instance info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XrayInstance {
  pub id: String,
  pub pid: u32,
  pub local_port: u16,
  pub upstream_url: String,
  pub config_path: PathBuf,
}

// Global Xray instances registry
lazy_static::lazy_static! {
  static ref XRAY_INSTANCES: Mutex<std::collections::HashMap<String, XrayInstance>> =
    Mutex::new(std::collections::HashMap::new());
}

/// Get Xray binary directory
pub fn get_xray_bin_dir() -> PathBuf {
  let base_dirs = BaseDirs::new().expect("Failed to get base directories");
  let mut path = base_dirs.data_local_dir().to_path_buf();
  path.push(if cfg!(debug_assertions) {
    "DonutBrowserDev"
  } else {
    "DonutBrowser"
  });
  path.push("xray");
  path.push(get_platform_arch());
  path
}

/// Get platform-architecture string
fn get_platform_arch() -> String {
  let os = if cfg!(target_os = "windows") {
    "windows"
  } else if cfg!(target_os = "macos") {
    "darwin"
  } else {
    "linux"
  };

  let arch = if cfg!(target_arch = "x86_64") {
    "x64"
  } else if cfg!(target_arch = "aarch64") {
    "arm64"
  } else {
    "x86"
  };

  format!("{}-{}", os, arch)
}

/// Get Xray executable name for current platform
fn get_xray_executable_name() -> &'static str {
  if cfg!(target_os = "windows") {
    "xray.exe"
  } else {
    "xray"
  }
}

/// Get Xray asset name for current platform
fn get_xray_asset_name() -> String {
  if cfg!(target_os = "windows") {
    if cfg!(target_arch = "x86_64") {
      "Xray-windows-64.zip".to_string()
    } else {
      "Xray-windows-32.zip".to_string()
    }
  } else if cfg!(target_os = "macos") {
    if cfg!(target_arch = "aarch64") {
      "Xray-macos-arm64-v8a.zip".to_string()
    } else {
      "Xray-macos-64.zip".to_string()
    }
  } else {
    // Linux
    if cfg!(target_arch = "x86_64") {
      "Xray-linux-64.zip".to_string()
    } else if cfg!(target_arch = "aarch64") {
      "Xray-linux-arm64-v8a.zip".to_string()
    } else {
      "Xray-linux-32.zip".to_string()
    }
  }
}

/// Get Xray executable path
pub fn get_xray_executable_path() -> PathBuf {
  get_xray_bin_dir().join(get_xray_executable_name())
}

/// Check if Xray is installed
pub fn is_xray_installed() -> bool {
  get_xray_executable_path().exists()
}

/// Get Xray version
pub async fn get_xray_version() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
  let exe_path = get_xray_executable_path();
  if !exe_path.exists() {
    return Err("Xray not installed".into());
  }

  let output = tokio::process::Command::new(&exe_path)
    .arg("version")
    .output()
    .await?;

  let stdout = String::from_utf8_lossy(&output.stdout);
  // Extract version from output like "Xray 24.12.31 (Xray, Penetrates Everything.)"
  if let Some(line) = stdout.lines().next() {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
      return Ok(parts[1].to_string());
    }
  }

  Ok("unknown".to_string())
}

/// Get latest Xray version from GitHub
async fn get_latest_xray_version() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
  let client = reqwest::Client::builder()
    .user_agent("DonutBrowser")
    .build()?;

  let response = client
    .get("https://api.github.com/repos/XTLS/Xray-core/releases/latest")
    .send()
    .await?;

  if !response.status().is_success() {
    return Err(format!("GitHub API error: {}", response.status()).into());
  }

  let release: GitHubRelease = response.json().await?;
  Ok(release.tag_name)
}

/// Download and install Xray
pub async fn download_xray(
  progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
  log::info!("Starting Xray download...");

  // Get latest version
  let version = get_latest_xray_version().await.unwrap_or_else(|e| {
    log::warn!("Failed to get latest version: {}, using fallback", e);
    "v25.1.1".to_string()
  });

  log::info!("Downloading Xray version: {}", version);

  let asset_name = get_xray_asset_name();
  let download_url = format!(
    "https://github.com/XTLS/Xray-core/releases/download/{}/{}",
    version, asset_name
  );

  // Try mirror if direct download fails (for users in restricted regions)
  let mirror_url = format!("https://gh-proxy.com/{}", download_url);

  let client = reqwest::Client::builder()
    .user_agent("DonutBrowser")
    .timeout(std::time::Duration::from_secs(300))
    .build()?;

  // Try direct URL first, then mirror
  let response = match client.get(&download_url).send().await {
    Ok(r) if r.status().is_success() => r,
    _ => {
      log::info!("Direct download failed, trying mirror...");
      client.get(&mirror_url).send().await?
    }
  };

  if !response.status().is_success() {
    return Err(format!("Download failed: {}", response.status()).into());
  }

  let total_size = response.content_length().unwrap_or(0);
  let mut downloaded: u64 = 0;

  // Download to temp file
  let bin_dir = get_xray_bin_dir();
  fs::create_dir_all(&bin_dir)?;

  let zip_path = bin_dir.join("xray.zip");
  let mut file = fs::File::create(&zip_path)?;

  let mut stream = response.bytes_stream();
  use futures_util::StreamExt;

  while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    file.write_all(&chunk)?;
    downloaded += chunk.len() as u64;

    if let Some(ref callback) = progress_callback {
      callback(downloaded, total_size);
    }
  }

  drop(file);

  log::info!("Download complete, extracting...");

  // Extract zip
  let zip_file = fs::File::open(&zip_path)?;
  let mut archive = zip::ZipArchive::new(zip_file)?;

  for i in 0..archive.len() {
    let mut file = archive.by_index(i)?;
    let outpath = bin_dir.join(file.name());

    if file.name().ends_with('/') {
      fs::create_dir_all(&outpath)?;
    } else {
      if let Some(parent) = outpath.parent() {
        fs::create_dir_all(parent)?;
      }
      let mut outfile = fs::File::create(&outpath)?;
      std::io::copy(&mut file, &mut outfile)?;
    }
  }

  // Clean up zip
  let _ = fs::remove_file(&zip_path);

  // Make executable on Unix
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let exe_path = get_xray_executable_path();
    if exe_path.exists() {
      let mut perms = fs::metadata(&exe_path)?.permissions();
      perms.set_mode(0o755);
      fs::set_permissions(&exe_path, perms)?;
    }
  }

  // Move geo files to parent directory for sharing
  let geo_files = ["geoip.dat", "geosite.dat"];
  let parent_dir = bin_dir.parent().unwrap();
  for geo_file in geo_files {
    let src = bin_dir.join(geo_file);
    let dst = parent_dir.join(geo_file);
    if src.exists() && !dst.exists() {
      let _ = fs::rename(&src, &dst);
    } else if src.exists() {
      let _ = fs::remove_file(&src);
    }
  }

  log::info!("Xray {} installed successfully", version);
  Ok(version)
}

/// Start Xray instance for a proxy URL
pub async fn start_xray_instance(
  id: &str,
  upstream_url: &str,
  local_port: u16,
  pre_proxy_url: Option<&str>,
) -> Result<XrayInstance, Box<dyn std::error::Error + Send + Sync>> {
  // Check if Xray is installed
  if !is_xray_installed() {
    return Err("Xray is not installed. Please download it first.".into());
  }

  // Generate config
  let config = generate_xray_config(upstream_url, local_port, pre_proxy_url)
    .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e))?;

  // Write config to temp file
  let config_dir = get_xray_bin_dir().parent().unwrap().join("configs");
  fs::create_dir_all(&config_dir)?;

  let config_path = config_dir.join(format!("{}.json", id));
  let config_content = serde_json::to_string_pretty(&config)?;
  fs::write(&config_path, &config_content)?;

  log::info!(
    "Starting Xray instance {} on port {} for {}",
    id,
    local_port,
    upstream_url
  );

  // Start Xray process
  let exe_path = get_xray_executable_path();

  // Set environment for geo files
  let geo_dir = get_xray_bin_dir().parent().unwrap().to_path_buf();

  #[cfg(unix)]
  let child = {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new(&exe_path);
    cmd.arg("run");
    cmd.arg("-c");
    cmd.arg(&config_path);
    cmd.env("XRAY_LOCATION_ASSET", &geo_dir);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    unsafe {
      cmd.pre_exec(|| {
        libc::setsid();
        Ok(())
      });
    }

    cmd.spawn()?
  };

  #[cfg(windows)]
  let child = {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    std::process::Command::new(&exe_path)
      .arg("run")
      .arg("-c")
      .arg(&config_path)
      .env("XRAY_LOCATION_ASSET", &geo_dir)
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
      .spawn()?
  };

  let pid = child.id();

  let instance = XrayInstance {
    id: id.to_string(),
    pid,
    local_port,
    upstream_url: upstream_url.to_string(),
    config_path: config_path.clone(),
  };

  // Store instance
  {
    let mut instances = XRAY_INSTANCES.lock().await;
    instances.insert(id.to_string(), instance.clone());
  }

  // Wait a moment for Xray to start
  tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

  // Verify it's listening
  let mut attempts = 0;
  let max_attempts = 20;

  while attempts < max_attempts {
    match tokio::net::TcpStream::connect(("127.0.0.1", local_port)).await {
      Ok(_) => {
        log::info!("Xray instance {} started successfully on port {}", id, local_port);
        return Ok(instance);
      }
      Err(_) => {
        attempts += 1;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
      }
    }
  }

  // If we got here, Xray failed to start
  stop_xray_instance(id).await?;
  Err(format!("Xray failed to start listening on port {}", local_port).into())
}

/// Stop Xray instance
pub async fn stop_xray_instance(id: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
  let instance = {
    let mut instances = XRAY_INSTANCES.lock().await;
    instances.remove(id)
  };

  if let Some(instance) = instance {
    log::info!("Stopping Xray instance {} (PID: {})", id, instance.pid);

    // Kill process
    #[cfg(unix)]
    {
      use std::process::Command;
      let _ = Command::new("kill")
        .arg("-TERM")
        .arg(instance.pid.to_string())
        .output();
    }

    #[cfg(windows)]
    {
      use std::process::Command;
      let _ = Command::new("taskkill")
        .args(["/F", "/PID", &instance.pid.to_string()])
        .output();
    }

    // Remove config file
    let _ = fs::remove_file(&instance.config_path);

    Ok(true)
  } else {
    Ok(false)
  }
}

/// Stop all Xray instances
pub async fn stop_all_xray_instances() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let ids: Vec<String> = {
    let instances = XRAY_INSTANCES.lock().await;
    instances.keys().cloned().collect()
  };

  for id in ids {
    let _ = stop_xray_instance(&id).await;
  }

  Ok(())
}

/// Get running Xray instance
pub async fn get_xray_instance(id: &str) -> Option<XrayInstance> {
  let instances = XRAY_INSTANCES.lock().await;
  instances.get(id).cloned()
}

/// List all running Xray instances
pub async fn list_xray_instances() -> Vec<XrayInstance> {
  let instances = XRAY_INSTANCES.lock().await;
  instances.values().cloned().collect()
}

/// Check if an upstream URL requires Xray
pub fn requires_xray(upstream_url: &str) -> bool {
  is_xray_protocol(upstream_url)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_get_platform_arch() {
    let arch = get_platform_arch();
    assert!(!arch.is_empty());
    // Should contain os-arch pattern
    assert!(arch.contains('-'));
  }

  #[test]
  fn test_get_xray_asset_name() {
    let asset = get_xray_asset_name();
    assert!(asset.starts_with("Xray-"));
    assert!(asset.ends_with(".zip"));
  }

  #[test]
  fn test_requires_xray() {
    assert!(requires_xray("vmess://abc123"));
    assert!(requires_xray("vless://abc123"));
    assert!(requires_xray("trojan://abc123"));
    assert!(requires_xray("ss://abc123"));
    assert!(!requires_xray("http://localhost:8080"));
    assert!(!requires_xray("socks5://localhost:1080"));
    assert!(!requires_xray("DIRECT"));
  }
}
