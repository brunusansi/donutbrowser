use crate::xray_protocol_parser::{TransportSettings, XrayProxyConfig, XrayProtocol};
use serde_json::{json, Value};

/// Generate Xray-core JSON configuration from XrayProxyConfig
pub fn generate_xray_config(config: &XrayProxyConfig, local_port: u16) -> Value {
  let outbound = generate_outbound(config);

  json!({
    "log": {
      "loglevel": "warning"
    },
    "inbounds": [
      {
        "tag": "socks-in",
        "port": local_port,
        "listen": "127.0.0.1",
        "protocol": "socks",
        "settings": {
          "auth": "noauth",
          "udp": true
        },
        "sniffing": {
          "enabled": true,
          "destOverride": ["http", "tls"]
        }
      },
      {
        "tag": "http-in",
        "port": local_port + 1,
        "listen": "127.0.0.1",
        "protocol": "http",
        "settings": {}
      }
    ],
    "outbounds": [
      outbound,
      {
        "tag": "direct",
        "protocol": "freedom",
        "settings": {}
      },
      {
        "tag": "block",
        "protocol": "blackhole",
        "settings": {}
      }
    ],
    "routing": {
      "domainStrategy": "AsIs",
      "rules": [
        {
          "type": "field",
          "outboundTag": "proxy",
          "network": "tcp,udp"
        }
      ]
    }
  })
}

/// Generate the outbound configuration for the proxy
fn generate_outbound(config: &XrayProxyConfig) -> Value {
  let protocol = match config.protocol {
    XrayProtocol::VMess => "vmess",
    XrayProtocol::VLESS => "vless",
    XrayProtocol::Trojan => "trojan",
    XrayProtocol::Shadowsocks => "shadowsocks",
  };

  let settings = generate_protocol_settings(config);
  let stream_settings = generate_stream_settings(config);

  json!({
    "tag": "proxy",
    "protocol": protocol,
    "settings": settings,
    "streamSettings": stream_settings
  })
}

/// Generate protocol-specific settings
fn generate_protocol_settings(config: &XrayProxyConfig) -> Value {
  match config.protocol {
    XrayProtocol::VMess => {
      json!({
        "vnext": [
          {
            "address": config.address,
            "port": config.port,
            "users": [
              {
                "id": config.uuid.clone().unwrap_or_default(),
                "alterId": config.alter_id.unwrap_or(0),
                "security": config.security.clone().unwrap_or_else(|| "auto".to_string())
              }
            ]
          }
        ]
      })
    }
    XrayProtocol::VLESS => {
      let mut user: Value = json!({
        "id": config.uuid.clone().unwrap_or_default(),
        "encryption": config.encryption.clone().unwrap_or_else(|| "none".to_string())
      });

      if let Some(ref flow) = config.flow {
        user["flow"] = json!(flow);
      }

      json!({
        "vnext": [
          {
            "address": config.address,
            "port": config.port,
            "users": [user]
          }
        ]
      })
    }
    XrayProtocol::Trojan => {
      json!({
        "servers": [
          {
            "address": config.address,
            "port": config.port,
            "password": config.password.clone().unwrap_or_default()
          }
        ]
      })
    }
    XrayProtocol::Shadowsocks => {
      json!({
        "servers": [
          {
            "address": config.address,
            "port": config.port,
            "method": config.encryption.clone().unwrap_or_else(|| "aes-256-gcm".to_string()),
            "password": config.password.clone().unwrap_or_default()
          }
        ]
      })
    }
  }
}

/// Generate stream settings (TLS and transport)
fn generate_stream_settings(config: &XrayProxyConfig) -> Value {
  let network = match &config.transport {
    TransportSettings::Tcp => "tcp",
    TransportSettings::Ws { .. } => "ws",
    TransportSettings::Grpc { .. } => "grpc",
    TransportSettings::Http { .. } => "http",
    TransportSettings::Quic { .. } => "quic",
  };

  let mut settings: Value = json!({
    "network": network
  });

  // TLS settings
  if config.tls.enabled {
    settings["security"] = json!("tls");

    let mut tls_settings: Value = json!({});

    if let Some(ref sni) = config.tls.server_name {
      tls_settings["serverName"] = json!(sni);
    }

    if config.tls.allow_insecure {
      tls_settings["allowInsecure"] = json!(true);
    }

    if let Some(ref fp) = config.tls.fingerprint {
      tls_settings["fingerprint"] = json!(fp);
    }

    if let Some(ref alpn) = config.tls.alpn {
      tls_settings["alpn"] = json!(alpn);
    }

    settings["tlsSettings"] = tls_settings;
  } else {
    settings["security"] = json!("none");
  }

  // Transport-specific settings
  match &config.transport {
    TransportSettings::Ws { path, host } => {
      let mut ws_settings: Value = json!({
        "path": path
      });
      if let Some(ref h) = host {
        ws_settings["headers"] = json!({
          "Host": h
        });
      }
      settings["wsSettings"] = ws_settings;
    }
    TransportSettings::Grpc { service_name } => {
      settings["grpcSettings"] = json!({
        "serviceName": service_name
      });
    }
    TransportSettings::Http { path, host } => {
      let mut http_settings: Value = json!({
        "path": path
      });
      if let Some(ref h) = host {
        http_settings["host"] = json!(h);
      }
      settings["httpSettings"] = http_settings;
    }
    TransportSettings::Quic {
      security,
      key,
      header_type,
    } => {
      let mut quic_settings: Value = json!({});
      if let Some(ref s) = security {
        quic_settings["security"] = json!(s);
      }
      if let Some(ref k) = key {
        quic_settings["key"] = json!(k);
      }
      if let Some(ref ht) = header_type {
        quic_settings["header"] = json!({
          "type": ht
        });
      }
      settings["quicSettings"] = quic_settings;
    }
    TransportSettings::Tcp => {}
  }

  settings
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::xray_protocol_parser::TlsSettings;

  #[test]
  fn test_generate_vmess_config() {
    let config = XrayProxyConfig {
      protocol: XrayProtocol::VMess,
      address: "example.com".to_string(),
      port: 443,
      uuid: Some("test-uuid".to_string()),
      alter_id: Some(0),
      security: Some("auto".to_string()),
      encryption: None,
      password: None,
      flow: None,
      tls: TlsSettings {
        enabled: true,
        server_name: Some("example.com".to_string()),
        allow_insecure: false,
        fingerprint: None,
        alpn: None,
      },
      transport: TransportSettings::Ws {
        path: "/path".to_string(),
        host: Some("example.com".to_string()),
      },
      remark: None,
    };

    let xray_config = generate_xray_config(&config, 10808);

    assert_eq!(xray_config["inbounds"][0]["port"], 10808);
    assert_eq!(xray_config["outbounds"][0]["protocol"], "vmess");
    assert_eq!(xray_config["outbounds"][0]["streamSettings"]["network"], "ws");
    assert_eq!(
      xray_config["outbounds"][0]["streamSettings"]["security"],
      "tls"
    );
  }

  #[test]
  fn test_generate_shadowsocks_config() {
    let config = XrayProxyConfig {
      protocol: XrayProtocol::Shadowsocks,
      address: "ss.example.com".to_string(),
      port: 8388,
      uuid: None,
      alter_id: None,
      security: None,
      encryption: Some("aes-256-gcm".to_string()),
      password: Some("testpassword".to_string()),
      flow: None,
      tls: TlsSettings::default(),
      transport: TransportSettings::Tcp,
      remark: None,
    };

    let xray_config = generate_xray_config(&config, 10808);

    assert_eq!(xray_config["outbounds"][0]["protocol"], "shadowsocks");
    assert_eq!(
      xray_config["outbounds"][0]["settings"]["servers"][0]["method"],
      "aes-256-gcm"
    );
    assert_eq!(
      xray_config["outbounds"][0]["settings"]["servers"][0]["password"],
      "testpassword"
    );
  }
}
