use base64::{engine::general_purpose, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

/// Supported Xray protocol types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum XrayProtocol {
  VMess,
  VLESS,
  Trojan,
  Shadowsocks,
}

impl std::fmt::Display for XrayProtocol {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      XrayProtocol::VMess => write!(f, "vmess"),
      XrayProtocol::VLESS => write!(f, "vless"),
      XrayProtocol::Trojan => write!(f, "trojan"),
      XrayProtocol::Shadowsocks => write!(f, "shadowsocks"),
    }
  }
}

/// TLS settings for Xray protocols
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TlsSettings {
  pub enabled: bool,
  pub server_name: Option<String>,
  pub allow_insecure: bool,
  pub fingerprint: Option<String>,
  pub alpn: Option<Vec<String>>,
}

/// Transport settings for Xray protocols
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TransportSettings {
  Tcp,
  Ws {
    path: String,
    host: Option<String>,
  },
  Grpc {
    service_name: String,
  },
  Http {
    path: String,
    host: Option<Vec<String>>,
  },
  Quic {
    security: Option<String>,
    key: Option<String>,
    header_type: Option<String>,
  },
}

impl Default for TransportSettings {
  fn default() -> Self {
    TransportSettings::Tcp
  }
}

/// Parsed Xray proxy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XrayProxyConfig {
  pub protocol: XrayProtocol,
  pub address: String,
  pub port: u16,
  pub uuid: Option<String>,
  pub alter_id: Option<u16>,
  pub security: Option<String>,
  pub encryption: Option<String>,
  pub password: Option<String>,
  pub flow: Option<String>,
  pub tls: TlsSettings,
  pub transport: TransportSettings,
  pub remark: Option<String>,
}

impl XrayProxyConfig {
  pub fn new(protocol: XrayProtocol, address: String, port: u16) -> Self {
    Self {
      protocol,
      address,
      port,
      uuid: None,
      alter_id: None,
      security: None,
      encryption: None,
      password: None,
      flow: None,
      tls: TlsSettings::default(),
      transport: TransportSettings::default(),
      remark: None,
    }
  }
}

/// Parse a proxy URL and return an XrayProxyConfig
pub fn parse_proxy_url(url: &str) -> Result<XrayProxyConfig, String> {
  let url = url.trim();

  if url.starts_with("vmess://") {
    parse_vmess(url)
  } else if url.starts_with("vless://") {
    parse_vless(url)
  } else if url.starts_with("trojan://") {
    parse_trojan(url)
  } else if url.starts_with("ss://") {
    parse_shadowsocks(url)
  } else {
    Err(format!("Unsupported protocol URL: {}", url))
  }
}

/// Parse VMess URL (base64 encoded JSON)
fn parse_vmess(url: &str) -> Result<XrayProxyConfig, String> {
  let encoded = url
    .strip_prefix("vmess://")
    .ok_or("Invalid VMess URL format")?;

  let decoded = general_purpose::STANDARD
    .decode(encoded.trim())
    .or_else(|_| general_purpose::URL_SAFE.decode(encoded.trim()))
    .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(encoded.trim()))
    .map_err(|e| format!("Failed to decode VMess base64: {}", e))?;

  let json_str =
    String::from_utf8(decoded).map_err(|e| format!("Invalid UTF-8 in VMess config: {}", e))?;

  let json: serde_json::Value =
    serde_json::from_str(&json_str).map_err(|e| format!("Failed to parse VMess JSON: {}", e))?;

  let address = json["add"]
    .as_str()
    .or_else(|| json["addr"].as_str())
    .ok_or("Missing address in VMess config")?
    .to_string();

  let port = json["port"]
    .as_u64()
    .or_else(|| json["port"].as_str().and_then(|s| s.parse().ok()))
    .ok_or("Missing port in VMess config")? as u16;

  let mut config = XrayProxyConfig::new(XrayProtocol::VMess, address, port);

  config.uuid = json["id"].as_str().map(|s| s.to_string());
  config.alter_id = json["aid"]
    .as_u64()
    .or_else(|| json["aid"].as_str().and_then(|s| s.parse().ok()))
    .map(|v| v as u16);
  config.security = json["scy"]
    .as_str()
    .or_else(|| json["security"].as_str())
    .map(|s| s.to_string());
  config.remark = json["ps"]
    .as_str()
    .or_else(|| json["remark"].as_str())
    .map(|s| s.to_string());

  // TLS settings
  let tls_type = json["tls"].as_str().unwrap_or("");
  config.tls.enabled = tls_type == "tls" || tls_type == "xtls";
  config.tls.server_name = json["sni"]
    .as_str()
    .or_else(|| json["host"].as_str())
    .map(|s| s.to_string());
  config.tls.fingerprint = json["fp"].as_str().map(|s| s.to_string());

  // Transport settings
  let net = json["net"].as_str().unwrap_or("tcp");
  config.transport = parse_transport_settings(net, &json);

  Ok(config)
}

/// Parse VLESS URL
fn parse_vless(url: &str) -> Result<XrayProxyConfig, String> {
  let url_without_scheme = url
    .strip_prefix("vless://")
    .ok_or("Invalid VLESS URL format")?;

  let parsed = Url::parse(&format!("http://{}", url_without_scheme))
    .map_err(|e| format!("Failed to parse VLESS URL: {}", e))?;

  let uuid = parsed.username().to_string();
  if uuid.is_empty() {
    return Err("Missing UUID in VLESS URL".to_string());
  }

  let address = parsed
    .host_str()
    .ok_or("Missing host in VLESS URL")?
    .to_string();
  let port = parsed.port().unwrap_or(443);

  let mut config = XrayProxyConfig::new(XrayProtocol::VLESS, address, port);
  config.uuid = Some(uuid);

  let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

  config.encryption = params.get("encryption").cloned();
  config.flow = params.get("flow").cloned();

  // TLS settings
  let security = params.get("security").map(|s| s.as_str()).unwrap_or("");
  config.tls.enabled = security == "tls" || security == "xtls" || security == "reality";
  config.tls.server_name = params.get("sni").cloned();
  config.tls.fingerprint = params.get("fp").cloned();
  config.tls.allow_insecure = params.get("allowInsecure").is_some_and(|v| v == "1");

  if let Some(alpn) = params.get("alpn") {
    config.tls.alpn = Some(alpn.split(',').map(|s| s.to_string()).collect());
  }

  // Transport settings
  let transport_type = params.get("type").map(|s| s.as_str()).unwrap_or("tcp");
  config.transport = parse_transport_from_params(transport_type, &params);

  // Remark from fragment
  config.remark = parsed.fragment().map(|s| urlencoding::decode(s).unwrap_or_default().to_string());

  Ok(config)
}

/// Parse Trojan URL
fn parse_trojan(url: &str) -> Result<XrayProxyConfig, String> {
  let url_without_scheme = url
    .strip_prefix("trojan://")
    .ok_or("Invalid Trojan URL format")?;

  let parsed = Url::parse(&format!("http://{}", url_without_scheme))
    .map_err(|e| format!("Failed to parse Trojan URL: {}", e))?;

  let password = parsed.username().to_string();
  if password.is_empty() {
    return Err("Missing password in Trojan URL".to_string());
  }

  let address = parsed
    .host_str()
    .ok_or("Missing host in Trojan URL")?
    .to_string();
  let port = parsed.port().unwrap_or(443);

  let mut config = XrayProxyConfig::new(XrayProtocol::Trojan, address, port);
  config.password = Some(urlencoding::decode(&password).unwrap_or_default().to_string());

  let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

  // TLS is enabled by default for Trojan
  let security = params.get("security").map(|s| s.as_str()).unwrap_or("tls");
  config.tls.enabled = security != "none";
  config.tls.server_name = params.get("sni").cloned();
  config.tls.fingerprint = params.get("fp").cloned();
  config.tls.allow_insecure = params.get("allowInsecure").is_some_and(|v| v == "1");

  if let Some(alpn) = params.get("alpn") {
    config.tls.alpn = Some(alpn.split(',').map(|s| s.to_string()).collect());
  }

  // Transport settings
  let transport_type = params.get("type").map(|s| s.as_str()).unwrap_or("tcp");
  config.transport = parse_transport_from_params(transport_type, &params);

  // Remark from fragment
  config.remark = parsed.fragment().map(|s| urlencoding::decode(s).unwrap_or_default().to_string());

  Ok(config)
}

/// Parse Shadowsocks URL (SIP002 format: ss://BASE64(method:password)@host:port#remark)
fn parse_shadowsocks(url: &str) -> Result<XrayProxyConfig, String> {
  let url_without_scheme = url
    .strip_prefix("ss://")
    .ok_or("Invalid Shadowsocks URL format")?;

  // Handle SIP002 format with userinfo
  if let Some(at_pos) = url_without_scheme.rfind('@') {
    let userinfo_encoded = &url_without_scheme[..at_pos];
    let host_part = &url_without_scheme[at_pos + 1..];

    // Decode userinfo (method:password in base64)
    let userinfo = general_purpose::STANDARD
      .decode(userinfo_encoded)
      .or_else(|_| general_purpose::URL_SAFE.decode(userinfo_encoded))
      .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(userinfo_encoded))
      .map_err(|e| format!("Failed to decode Shadowsocks userinfo: {}", e))?;

    let userinfo_str = String::from_utf8(userinfo)
      .map_err(|e| format!("Invalid UTF-8 in Shadowsocks userinfo: {}", e))?;

    let (method, password) = userinfo_str
      .split_once(':')
      .ok_or("Invalid Shadowsocks userinfo format")?;

    // Parse host:port#remark
    let (host_port, remark) = if let Some(hash_pos) = host_part.find('#') {
      (
        &host_part[..hash_pos],
        Some(urlencoding::decode(&host_part[hash_pos + 1..]).unwrap_or_default().to_string()),
      )
    } else {
      (host_part, None)
    };

    // Handle query parameters
    let (host_port_clean, _params) = if let Some(q_pos) = host_port.find('?') {
      (&host_port[..q_pos], Some(&host_port[q_pos + 1..]))
    } else {
      (host_port, None)
    };

    let (address, port) = parse_host_port(host_port_clean)?;

    let mut config = XrayProxyConfig::new(XrayProtocol::Shadowsocks, address, port);
    config.encryption = Some(method.to_string());
    config.password = Some(password.to_string());
    config.remark = remark;

    return Ok(config);
  }

  // Handle legacy format (entire URL is base64 encoded)
  let base64_part = url_without_scheme.split('#').next().unwrap_or(url_without_scheme);
  let decoded = general_purpose::STANDARD
    .decode(base64_part)
    .or_else(|_| general_purpose::URL_SAFE.decode(base64_part))
    .map_err(|e| format!("Failed to decode Shadowsocks URL: {}", e))?;

  let decoded_str = String::from_utf8(decoded)
    .map_err(|e| format!("Invalid UTF-8 in Shadowsocks URL: {}", e))?;

  // Parse method:password@host:port
  let (method_pass, host_port) = decoded_str
    .split_once('@')
    .ok_or("Invalid Shadowsocks URL format")?;

  let (method, password) = method_pass
    .split_once(':')
    .ok_or("Invalid Shadowsocks method:password format")?;

  let (address, port) = parse_host_port(host_port)?;

  let mut config = XrayProxyConfig::new(XrayProtocol::Shadowsocks, address, port);
  config.encryption = Some(method.to_string());
  config.password = Some(password.to_string());

  // Extract remark from fragment
  if let Some(hash_pos) = url_without_scheme.find('#') {
    config.remark = Some(
      urlencoding::decode(&url_without_scheme[hash_pos + 1..])
        .unwrap_or_default()
        .to_string(),
    );
  }

  Ok(config)
}

/// Parse host:port string
fn parse_host_port(s: &str) -> Result<(String, u16), String> {
  // Handle IPv6 addresses in brackets
  if s.starts_with('[') {
    if let Some(bracket_end) = s.find(']') {
      let host = s[1..bracket_end].to_string();
      let port_str = &s[bracket_end + 1..];
      let port = if port_str.starts_with(':') {
        port_str[1..]
          .parse()
          .map_err(|_| "Invalid port number".to_string())?
      } else {
        443
      };
      return Ok((host, port));
    }
  }

  // Handle regular host:port
  if let Some(colon_pos) = s.rfind(':') {
    let host = s[..colon_pos].to_string();
    let port = s[colon_pos + 1..]
      .parse()
      .map_err(|_| "Invalid port number".to_string())?;
    Ok((host, port))
  } else {
    Ok((s.to_string(), 443))
  }
}

/// Parse transport settings from VMess JSON
fn parse_transport_settings(net: &str, json: &serde_json::Value) -> TransportSettings {
  match net {
    "ws" => TransportSettings::Ws {
      path: json["path"].as_str().unwrap_or("/").to_string(),
      host: json["host"].as_str().map(|s| s.to_string()),
    },
    "grpc" => TransportSettings::Grpc {
      service_name: json["serviceName"]
        .as_str()
        .or_else(|| json["path"].as_str())
        .unwrap_or("")
        .to_string(),
    },
    "h2" | "http" => TransportSettings::Http {
      path: json["path"].as_str().unwrap_or("/").to_string(),
      host: json["host"].as_str().map(|s| vec![s.to_string()]),
    },
    "quic" => TransportSettings::Quic {
      security: json["quicSecurity"].as_str().map(|s| s.to_string()),
      key: json["key"].as_str().map(|s| s.to_string()),
      header_type: json["headerType"].as_str().map(|s| s.to_string()),
    },
    _ => TransportSettings::Tcp,
  }
}

/// Parse transport settings from URL query parameters
fn parse_transport_from_params(
  transport_type: &str,
  params: &HashMap<String, String>,
) -> TransportSettings {
  match transport_type {
    "ws" => TransportSettings::Ws {
      path: params.get("path").cloned().unwrap_or_else(|| "/".to_string()),
      host: params.get("host").cloned(),
    },
    "grpc" => TransportSettings::Grpc {
      service_name: params
        .get("serviceName")
        .or_else(|| params.get("path"))
        .cloned()
        .unwrap_or_default(),
    },
    "h2" | "http" => TransportSettings::Http {
      path: params.get("path").cloned().unwrap_or_else(|| "/".to_string()),
      host: params.get("host").map(|s| vec![s.clone()]),
    },
    "quic" => TransportSettings::Quic {
      security: params.get("quicSecurity").cloned(),
      key: params.get("key").cloned(),
      header_type: params.get("headerType").cloned(),
    },
    _ => TransportSettings::Tcp,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_vless_url() {
    let url = concat!(
      "vless://uuid-here@example.com:443",
      "?encryption=none&security=tls&sni=example.com&type=ws&path=%2Fpath#Remark"
    );
    let config = parse_proxy_url(url).unwrap();

    assert_eq!(config.protocol, XrayProtocol::VLESS);
    assert_eq!(config.address, "example.com");
    assert_eq!(config.port, 443);
    assert_eq!(config.uuid, Some("uuid-here".to_string()));
    assert!(config.tls.enabled);
    assert_eq!(config.tls.server_name, Some("example.com".to_string()));
    assert_eq!(config.remark, Some("Remark".to_string()));
  }

  #[test]
  fn test_parse_trojan_url() {
    let url = "trojan://password@example.com:443?sni=example.com#Trojan%20Server";
    let config = parse_proxy_url(url).unwrap();

    assert_eq!(config.protocol, XrayProtocol::Trojan);
    assert_eq!(config.address, "example.com");
    assert_eq!(config.port, 443);
    assert_eq!(config.password, Some("password".to_string()));
    assert!(config.tls.enabled);
    assert_eq!(config.remark, Some("Trojan Server".to_string()));
  }

  #[test]
  fn test_parse_shadowsocks_sip002() {
    // SIP002 format: ss://BASE64(method:password)@host:port#remark
    let userinfo = general_purpose::STANDARD.encode("aes-256-gcm:testpassword");
    let url = format!("ss://{}@example.com:8388#Test%20SS", userinfo);
    let config = parse_proxy_url(&url).unwrap();

    assert_eq!(config.protocol, XrayProtocol::Shadowsocks);
    assert_eq!(config.address, "example.com");
    assert_eq!(config.port, 8388);
    assert_eq!(config.encryption, Some("aes-256-gcm".to_string()));
    assert_eq!(config.password, Some("testpassword".to_string()));
    assert_eq!(config.remark, Some("Test SS".to_string()));
  }

  #[test]
  fn test_unsupported_protocol() {
    let url = "http://example.com:8080";
    let result = parse_proxy_url(url);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unsupported protocol"));
  }
}
