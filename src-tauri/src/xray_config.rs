use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use url::Url;

/// Parsed proxy configuration from URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedProxy {
  pub protocol: String,
  pub tag: String,
  pub remark: Option<String>,
  pub outbound: Value,
}

/// Parse VMess URL (vmess://base64json)
fn parse_vmess(url_str: &str, tag: &str) -> Result<ParsedProxy, String> {
  let base64_str = url_str
    .strip_prefix("vmess://")
    .ok_or("Invalid vmess URL")?;

  // Decode base64
  let decoded = STANDARD
    .decode(base64_str.replace(['-', '_'], "+"))
    .map_err(|e| format!("Failed to decode vmess base64: {}", e))?;

  let config_str =
    String::from_utf8(decoded).map_err(|e| format!("Invalid UTF-8 in vmess config: {}", e))?;

  let vmess: HashMap<String, Value> =
    serde_json::from_str(&config_str).map_err(|e| format!("Invalid vmess JSON: {}", e))?;

  let address = vmess
    .get("add")
    .and_then(|v| v.as_str())
    .unwrap_or_default();
  let port = vmess
    .get("port")
    .and_then(|v| v.as_str().map(|s| s.parse().unwrap_or(443)).or(v.as_u64()))
    .unwrap_or(443) as u16;
  let id = vmess.get("id").and_then(|v| v.as_str()).unwrap_or_default();
  let aid = vmess
    .get("aid")
    .and_then(|v| v.as_str().map(|s| s.parse().unwrap_or(0)).or(v.as_u64()))
    .unwrap_or(0);
  let security = vmess
    .get("scy")
    .and_then(|v| v.as_str())
    .unwrap_or("auto");
  let net = vmess
    .get("net")
    .and_then(|v| v.as_str())
    .unwrap_or("tcp");
  let tls = vmess.get("tls").and_then(|v| v.as_str()).unwrap_or("none");
  let host = vmess
    .get("host")
    .and_then(|v| v.as_str())
    .unwrap_or_default();
  let path = vmess
    .get("path")
    .and_then(|v| v.as_str())
    .unwrap_or_default();
  let sni = vmess.get("sni").and_then(|v| v.as_str()).unwrap_or(host);
  let alpn = vmess.get("alpn").and_then(|v| v.as_str());
  let remark = vmess.get("ps").and_then(|v| v.as_str()).map(String::from);

  let mut stream_settings = json!({
    "network": net,
    "security": tls
  });

  // Network-specific settings
  match net {
    "ws" => {
      stream_settings["wsSettings"] = json!({
        "path": path,
        "headers": { "Host": host }
      });
    }
    "grpc" => {
      stream_settings["grpcSettings"] = json!({
        "serviceName": if path.is_empty() {
          vmess.get("serviceName").and_then(|v| v.as_str()).unwrap_or_default()
        } else {
          path
        }
      });
    }
    "h2" => {
      let hosts: Vec<&str> = if host.is_empty() {
        vec![]
      } else {
        host.split(',').collect()
      };
      stream_settings["httpSettings"] = json!({
        "path": path,
        "host": hosts
      });
    }
    "kcp" => {
      let header_type = vmess
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
      stream_settings["kcpSettings"] = json!({
        "header": { "type": header_type },
        "seed": path
      });
    }
    "quic" => {
      let header_type = vmess
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
      stream_settings["quicSettings"] = json!({
        "security": host,
        "key": path,
        "header": { "type": header_type }
      });
    }
    _ => {}
  }

  // TLS settings
  if tls == "tls" {
    let mut tls_settings = json!({
      "serverName": if sni.is_empty() { host } else { sni },
      "fingerprint": "chrome",
      "allowInsecure": true
    });
    if let Some(alpn_str) = alpn {
      let alpn_list: Vec<&str> = alpn_str.split(',').collect();
      tls_settings["alpn"] = json!(alpn_list);
    }
    stream_settings["tlsSettings"] = tls_settings;
  }

  let outbound = json!({
    "tag": tag,
    "protocol": "vmess",
    "settings": {
      "vnext": [{
        "address": address,
        "port": port,
        "users": [{
          "id": id,
          "alterId": aid,
          "security": security
        }]
      }]
    },
    "streamSettings": stream_settings,
    "sniffing": {
      "enabled": true,
      "destOverride": ["http", "tls", "quic"],
      "routeOnly": true
    }
  });

  Ok(ParsedProxy {
    protocol: "vmess".to_string(),
    tag: tag.to_string(),
    remark,
    outbound,
  })
}

/// Parse VLESS URL (vless://uuid@host:port?params#remark)
fn parse_vless(url_str: &str, tag: &str) -> Result<ParsedProxy, String> {
  let url = Url::parse(url_str).map_err(|e| format!("Invalid vless URL: {}", e))?;

  let uuid = url.username();
  let host = url.host_str().unwrap_or_default();
  let port = url.port().unwrap_or(443);
  let remark = url
    .fragment()
    .map(|f| urlencoding::decode(f).unwrap_or_default().to_string());

  let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

  let security = params.get("security").map(|s| s.as_str()).unwrap_or("none");
  let net_type = params.get("type").map(|s| s.as_str()).unwrap_or("tcp");
  let encryption = params
    .get("encryption")
    .map(|s| s.as_str())
    .unwrap_or("none");
  let flow = params.get("flow").map(|s| s.as_str()).unwrap_or("");

  let mut stream_settings = json!({
    "network": if net_type == "splithttp" { "xhttp" } else { net_type },
    "security": security
  });

  // Network-specific settings
  match net_type {
    "ws" => {
      stream_settings["wsSettings"] = json!({
        "path": params.get("path").map(|s| s.as_str()).unwrap_or("/"),
        "headers": { "Host": params.get("host").map(|s| s.as_str()).unwrap_or(host) }
      });
    }
    "grpc" => {
      stream_settings["grpcSettings"] = json!({
        "serviceName": params.get("serviceName").map(|s| s.as_str()).unwrap_or("")
      });
    }
    "xhttp" | "splithttp" => {
      stream_settings["network"] = json!("xhttp");
      stream_settings["xhttpSettings"] = json!({
        "path": params.get("path").map(|s| s.as_str()).unwrap_or("/"),
        "host": params.get("host").map(|s| s.as_str()).unwrap_or(""),
        "mode": params.get("mode").map(|s| s.as_str()).unwrap_or("stream-up")
      });
    }
    "kcp" => {
      stream_settings["kcpSettings"] = json!({
        "header": { "type": params.get("headerType").map(|s| s.as_str()).unwrap_or("none") },
        "seed": params.get("seed").map(|s| s.as_str()).unwrap_or("")
      });
    }
    "h2" => {
      let hosts: Vec<&str> = params
        .get("host")
        .map(|h| h.split(',').collect())
        .unwrap_or_default();
      stream_settings["httpSettings"] = json!({
        "path": params.get("path").map(|s| s.as_str()).unwrap_or("/"),
        "host": hosts
      });
    }
    _ => {}
  }

  // Security settings
  match security {
    "tls" => {
      let sni = params
        .get("sni")
        .or(params.get("host"))
        .map(|s| s.as_str())
        .unwrap_or(host);
      let fp = params.get("fp").map(|s| s.as_str()).unwrap_or("chrome");
      let mut tls_settings = json!({
        "serverName": sni,
        "fingerprint": fp,
        "allowInsecure": true
      });
      if let Some(alpn) = params.get("alpn") {
        let alpn_list: Vec<&str> = alpn.split(',').collect();
        tls_settings["alpn"] = json!(alpn_list);
      }
      stream_settings["tlsSettings"] = tls_settings;
    }
    "reality" => {
      let sni = params
        .get("sni")
        .or(params.get("host"))
        .map(|s| s.as_str())
        .unwrap_or("");
      stream_settings["realitySettings"] = json!({
        "show": false,
        "fingerprint": params.get("fp").map(|s| s.as_str()).unwrap_or("chrome"),
        "serverName": sni,
        "publicKey": params.get("pbk").map(|s| s.as_str()).unwrap_or(""),
        "shortId": params.get("sid").map(|s| s.as_str()).unwrap_or(""),
        "spiderX": params.get("spx").map(|s| s.as_str()).unwrap_or("")
      });
    }
    _ => {}
  }

  let outbound = json!({
    "tag": tag,
    "protocol": "vless",
    "settings": {
      "vnext": [{
        "address": host,
        "port": port,
        "users": [{
          "id": uuid,
          "encryption": encryption,
          "flow": flow
        }]
      }]
    },
    "streamSettings": stream_settings,
    "sniffing": {
      "enabled": true,
      "destOverride": ["http", "tls", "quic"],
      "routeOnly": true
    }
  });

  Ok(ParsedProxy {
    protocol: "vless".to_string(),
    tag: tag.to_string(),
    remark,
    outbound,
  })
}

/// Parse Trojan URL (trojan://password@host:port?params#remark)
fn parse_trojan(url_str: &str, tag: &str) -> Result<ParsedProxy, String> {
  let url = Url::parse(url_str).map_err(|e| format!("Invalid trojan URL: {}", e))?;

  let password = url.username();
  let host = url.host_str().unwrap_or_default();
  let port = url.port().unwrap_or(443);
  let remark = url
    .fragment()
    .map(|f| urlencoding::decode(f).unwrap_or_default().to_string());

  let params: HashMap<String, String> = url.query_pairs().into_owned().collect();

  let net_type = params.get("type").map(|s| s.as_str()).unwrap_or("tcp");
  let security = params.get("security").map(|s| s.as_str()).unwrap_or("tls");

  let mut stream_settings = json!({
    "network": net_type,
    "security": security,
    "tlsSettings": {
      "serverName": params.get("sni").map(|s| s.as_str()).unwrap_or(host),
      "fingerprint": "chrome",
      "allowInsecure": true
    }
  });

  // Network-specific settings
  match net_type {
    "ws" => {
      stream_settings["wsSettings"] = json!({
        "path": params.get("path").map(|s| s.as_str()).unwrap_or("/"),
        "headers": { "Host": params.get("host").map(|s| s.as_str()).unwrap_or(host) }
      });
    }
    "grpc" => {
      stream_settings["grpcSettings"] = json!({
        "serviceName": params.get("serviceName").map(|s| s.as_str()).unwrap_or("")
      });
    }
    _ => {}
  }

  let outbound = json!({
    "tag": tag,
    "protocol": "trojan",
    "settings": {
      "servers": [{
        "address": host,
        "port": port,
        "password": password
      }]
    },
    "streamSettings": stream_settings,
    "sniffing": {
      "enabled": true,
      "destOverride": ["http", "tls", "quic"],
      "routeOnly": true
    }
  });

  Ok(ParsedProxy {
    protocol: "trojan".to_string(),
    tag: tag.to_string(),
    remark,
    outbound,
  })
}

/// Parse Shadowsocks URL
/// Supports both formats:
/// - ss://base64(method:password)@host:port#remark
/// - ss://base64(method:password@host:port)#remark (legacy)
fn parse_shadowsocks(url_str: &str, tag: &str) -> Result<ParsedProxy, String> {
  let raw = url_str.strip_prefix("ss://").ok_or("Invalid ss URL")?;

  // Extract remark if present
  let (raw, remark) = if let Some(idx) = raw.find('#') {
    let (r, rem) = raw.split_at(idx);
    (
      r,
      Some(
        urlencoding::decode(&rem[1..])
          .unwrap_or_default()
          .to_string(),
      ),
    )
  } else {
    (raw, None)
  };

  let (method, password, host, port) = if raw.contains('@') {
    // Format: base64(method:password)@host:port or method:password@host:port
    let parts: Vec<&str> = raw.splitn(2, '@').collect();
    if parts.len() != 2 {
      return Err("Invalid ss URL format".to_string());
    }

    let user_part = parts[0];
    let host_part = parts[1];

    // Try to decode user part as base64
    let (method, password) = if user_part.contains(':') {
      // Plain text method:password
      let user_parts: Vec<&str> = user_part.splitn(2, ':').collect();
      (user_parts[0].to_string(), user_parts[1].to_string())
    } else {
      // Base64 encoded
      let decoded = STANDARD
        .decode(user_part.replace(['-', '_'], "+"))
        .map_err(|e| format!("Failed to decode ss base64: {}", e))?;
      let decoded_str = String::from_utf8(decoded)
        .map_err(|e| format!("Invalid UTF-8 in ss user part: {}", e))?;
      let user_parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
      if user_parts.len() != 2 {
        return Err("Invalid ss user part format".to_string());
      }
      (user_parts[0].to_string(), user_parts[1].to_string())
    };

    // Parse host:port (handle IPv6)
    let (host, port) = if host_part.starts_with('[') {
      // IPv6: [::1]:port
      let end_bracket = host_part.find(']').ok_or("Invalid IPv6 format")?;
      let host = &host_part[1..end_bracket];
      let port_str = &host_part[end_bracket + 2..]; // Skip ]:
      (
        host.to_string(),
        port_str.parse::<u16>().map_err(|e| format!("Invalid port: {}", e))?,
      )
    } else {
      // IPv4 or hostname
      let last_colon = host_part.rfind(':').ok_or("Missing port")?;
      let host = &host_part[..last_colon];
      let port_str = &host_part[last_colon + 1..];
      (
        host.to_string(),
        port_str.parse::<u16>().map_err(|e| format!("Invalid port: {}", e))?,
      )
    };

    (method, password, host, port)
  } else {
    // Legacy format: entire thing is base64 encoded
    let decoded = STANDARD
      .decode(raw.replace(['-', '_'], "+"))
      .map_err(|e| format!("Failed to decode ss base64: {}", e))?;
    let decoded_str =
      String::from_utf8(decoded).map_err(|e| format!("Invalid UTF-8 in ss config: {}", e))?;

    // Parse method:password@host:port
    let regex_pattern = r"^(.+?):(.+?)@(.+?):(\d+)$";
    let re = regex_lite::Regex::new(regex_pattern).unwrap();
    if let Some(caps) = re.captures(&decoded_str) {
      (
        caps.get(1).unwrap().as_str().to_string(),
        caps.get(2).unwrap().as_str().to_string(),
        caps.get(3).unwrap().as_str().to_string(),
        caps
          .get(4)
          .unwrap()
          .as_str()
          .parse()
          .map_err(|e| format!("Invalid port: {}", e))?,
      )
    } else {
      return Err("Invalid ss URL format".to_string());
    }
  };

  let outbound = json!({
    "tag": tag,
    "protocol": "shadowsocks",
    "settings": {
      "servers": [{
        "address": host,
        "port": port,
        "method": method,
        "password": password,
        "ota": false,
        "level": 1
      }]
    },
    "streamSettings": {
      "network": "tcp"
    },
    "mux": {
      "enabled": false,
      "concurrency": -1
    },
    "sniffing": {
      "enabled": true,
      "destOverride": ["http", "tls", "quic"],
      "routeOnly": true
    }
  });

  Ok(ParsedProxy {
    protocol: "shadowsocks".to_string(),
    tag: tag.to_string(),
    remark,
    outbound,
  })
}

/// Parse SOCKS5 URL (socks://user:pass@host:port or socks5://...)
fn parse_socks(url_str: &str, tag: &str) -> Result<ParsedProxy, String> {
  // Normalize URL scheme
  let normalized = if url_str.starts_with("socks5://") {
    url_str.replace("socks5://", "socks://")
  } else {
    url_str.to_string()
  };

  let url = Url::parse(&normalized).map_err(|e| format!("Invalid socks URL: {}", e))?;

  let host = url.host_str().unwrap_or("127.0.0.1");
  let port = url.port().unwrap_or(1080);
  let remark = url
    .fragment()
    .map(|f| urlencoding::decode(f).unwrap_or_default().to_string());

  // Handle authentication
  let users = if !url.username().is_empty() {
    let username = url.username();
    let password = url.password().unwrap_or("");

    // Check if username is base64 encoded (v2rayN style)
    let (user, pass) = if !username.contains(':') {
      if let Ok(decoded) = STANDARD.decode(username) {
        if let Ok(decoded_str) = String::from_utf8(decoded) {
          if let Some(idx) = decoded_str.find(':') {
            (
              decoded_str[..idx].to_string(),
              decoded_str[idx + 1..].to_string(),
            )
          } else {
            (username.to_string(), password.to_string())
          }
        } else {
          (username.to_string(), password.to_string())
        }
      } else {
        (username.to_string(), password.to_string())
      }
    } else {
      (username.to_string(), password.to_string())
    };

    json!([{ "user": user, "pass": pass }])
  } else {
    json!([])
  };

  let outbound = json!({
    "tag": tag,
    "protocol": "socks",
    "settings": {
      "servers": [{
        "address": host,
        "port": port,
        "users": users
      }]
    },
    "sniffing": {
      "enabled": true,
      "destOverride": ["http", "tls", "quic"],
      "routeOnly": true
    }
  });

  Ok(ParsedProxy {
    protocol: "socks".to_string(),
    tag: tag.to_string(),
    remark,
    outbound,
  })
}

/// Parse HTTP proxy URL
fn parse_http(url_str: &str, tag: &str) -> Result<ParsedProxy, String> {
  let url = Url::parse(url_str).map_err(|e| format!("Invalid http URL: {}", e))?;

  let host = url.host_str().unwrap_or("127.0.0.1");
  let port = url.port().unwrap_or(8080);
  let remark = url
    .fragment()
    .map(|f| urlencoding::decode(f).unwrap_or_default().to_string());

  let users = if !url.username().is_empty() {
    json!([{
      "user": url.username(),
      "pass": url.password().unwrap_or("")
    }])
  } else {
    json!([])
  };

  let outbound = json!({
    "tag": tag,
    "protocol": "http",
    "settings": {
      "servers": [{
        "address": host,
        "port": port,
        "users": users
      }]
    },
    "sniffing": {
      "enabled": true,
      "destOverride": ["http", "tls", "quic"],
      "routeOnly": true
    }
  });

  Ok(ParsedProxy {
    protocol: "http".to_string(),
    tag: tag.to_string(),
    remark,
    outbound,
  })
}

/// Parse IP:Port:User:Pass format
fn parse_ip_port_format(url_str: &str, tag: &str) -> Result<ParsedProxy, String> {
  let parts: Vec<&str> = url_str.split(':').collect();

  let (host, port, users) = match parts.len() {
    2 => {
      // IP:Port
      let port = parts[1]
        .parse::<u16>()
        .map_err(|e| format!("Invalid port: {}", e))?;
      (parts[0].to_string(), port, json!([]))
    }
    4 => {
      // IP:Port:User:Pass
      let port = parts[1]
        .parse::<u16>()
        .map_err(|e| format!("Invalid port: {}", e))?;
      (
        parts[0].to_string(),
        port,
        json!([{ "user": parts[2], "pass": parts[3] }]),
      )
    }
    _ => return Err("Invalid IP:Port format".to_string()),
  };

  let outbound = json!({
    "tag": tag,
    "protocol": "socks",
    "settings": {
      "servers": [{
        "address": host,
        "port": port,
        "users": users
      }]
    },
    "sniffing": {
      "enabled": true,
      "destOverride": ["http", "tls", "quic"],
      "routeOnly": true
    }
  });

  Ok(ParsedProxy {
    protocol: "socks".to_string(),
    tag: tag.to_string(),
    remark: None,
    outbound,
  })
}

/// Parse any supported proxy URL
pub fn parse_proxy_url(url: &str, tag: &str) -> Result<ParsedProxy, String> {
  let url = url.trim();

  if url.starts_with("vmess://") {
    parse_vmess(url, tag)
  } else if url.starts_with("vless://") {
    parse_vless(url, tag)
  } else if url.starts_with("trojan://") {
    parse_trojan(url, tag)
  } else if url.starts_with("ss://") {
    parse_shadowsocks(url, tag)
  } else if url.starts_with("socks://") || url.starts_with("socks5://") {
    parse_socks(url, tag)
  } else if url.starts_with("http://") || url.starts_with("https://") {
    parse_http(url, tag)
  } else if url.contains(':') && !url.contains("://") {
    parse_ip_port_format(url, tag)
  } else {
    Err(format!("Unsupported proxy protocol: {}", url))
  }
}

/// Get remark/name from proxy URL
pub fn get_proxy_remark(url: &str) -> Option<String> {
  let url = url.trim();

  if url.starts_with("vmess://") {
    // VMess: remark is in the JSON as "ps"
    if let Ok(proxy) = parse_vmess(url, "temp") {
      return proxy.remark;
    }
  } else if url.contains('#') {
    // Other protocols: remark is after #
    if let Some(idx) = url.find('#') {
      let remark = &url[idx + 1..];
      return Some(urlencoding::decode(remark).unwrap_or_default().to_string());
    }
  }

  None
}

/// Check if URL is an advanced protocol requiring Xray
pub fn is_xray_protocol(url: &str) -> bool {
  let url = url.trim().to_lowercase();
  url.starts_with("vmess://")
    || url.starts_with("vless://")
    || url.starts_with("trojan://")
    || url.starts_with("ss://")
}

/// Generate Xray config JSON for a proxy
pub fn generate_xray_config(
  main_proxy_url: &str,
  local_port: u16,
  pre_proxy_url: Option<&str>,
) -> Result<Value, String> {
  let mut outbounds = Vec::new();

  // Parse main proxy
  let mut main_outbound = parse_proxy_url(main_proxy_url, "proxy_main")?;

  // If pre-proxy is specified, set up proxy chain
  if let Some(pre_url) = pre_proxy_url {
    if !pre_url.is_empty() {
      let pre_outbound = parse_proxy_url(pre_url, "proxy_pre")?;
      outbounds.push(pre_outbound.outbound);

      // Add proxy chain setting to main outbound
      if let Some(obj) = main_outbound.outbound.as_object_mut() {
        obj.insert("proxySettings".to_string(), json!({ "tag": "proxy_pre" }));
      }
    }
  }

  outbounds.push(main_outbound.outbound);
  outbounds.push(json!({ "protocol": "freedom", "tag": "direct" }));

  let config = json!({
    "log": {
      "loglevel": "warning"
    },
    "inbounds": [{
      "port": local_port,
      "listen": "127.0.0.1",
      "protocol": "socks",
      "settings": {
        "udp": true
      }
    }],
    "outbounds": outbounds,
    "routing": {
      "domainStrategy": "IPIfNonMatch",
      "rules": [{
        "type": "field",
        "outboundTag": "proxy_main",
        "port": "0-65535"
      }]
    }
  });

  Ok(config)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_ss_url() {
    // Test new format: ss://base64@host:port#remark
    let url = "ss://YWVzLTI1Ni1nY206cGFzc3dvcmQ=@example.com:8388#MyProxy";
    let result = parse_proxy_url(url, "test");
    assert!(result.is_ok());
    let proxy = result.unwrap();
    assert_eq!(proxy.protocol, "shadowsocks");
    assert_eq!(proxy.remark, Some("MyProxy".to_string()));
  }

  #[test]
  fn test_parse_vless_url() {
    let url = "vless://uuid@example.com:443?type=ws&security=tls&path=/path#MyVLESS";
    let result = parse_proxy_url(url, "test");
    assert!(result.is_ok());
    let proxy = result.unwrap();
    assert_eq!(proxy.protocol, "vless");
    assert_eq!(proxy.remark, Some("MyVLESS".to_string()));
  }

  #[test]
  fn test_is_xray_protocol() {
    assert!(is_xray_protocol("vmess://abc123"));
    assert!(is_xray_protocol("vless://abc123"));
    assert!(is_xray_protocol("trojan://abc123"));
    assert!(is_xray_protocol("ss://abc123"));
    assert!(!is_xray_protocol("http://localhost:8080"));
    assert!(!is_xray_protocol("socks5://localhost:1080"));
  }

  #[test]
  fn test_generate_xray_config() {
    let url = "socks5://localhost:1080";
    let config = generate_xray_config(url, 10808, None);
    assert!(config.is_ok());
    let cfg = config.unwrap();
    assert!(cfg.get("inbounds").is_some());
    assert!(cfg.get("outbounds").is_some());
  }
}
