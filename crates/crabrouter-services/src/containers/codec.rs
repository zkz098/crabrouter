use std::collections::HashMap;
use std::fmt::Write;

use chrono::{DateTime, Utc};
use crabrouter_model::{
    ContainerCreateRequest, ContainerImageSummary, ContainerLogLine, ContainerNetworkCreateRequest,
    ContainerNetworkSummary, ContainerPortBinding, ContainerSummary, ContainerVolumeSummary,
};
use serde_json::{Value, json};

pub(super) fn decode_docker_log_frames(raw: &[u8]) -> Vec<ContainerLogLine> {
    if is_multiplexed_stream(raw) {
        let (lines, _) = drain_multiplexed_frames(raw);
        return lines;
    }
    let text = String::from_utf8_lossy(raw);
    text.lines()
        .map(|line| ContainerLogLine {
            stream: "stdout".to_string(),
            line: line.trim_end_matches('\r').to_string(),
        })
        .collect()
}

pub(super) fn is_multiplexed_stream(raw: &[u8]) -> bool {
    raw.len() >= 8 && matches!(raw[0], 0..=2) && raw[1] == 0 && raw[2] == 0 && raw[3] == 0
}

pub(super) fn drain_multiplexed_frames(buf: &[u8]) -> (Vec<ContainerLogLine>, usize) {
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset + 8 <= buf.len() {
        if !matches!(buf[offset], 0..=2)
            || buf[offset + 1] != 0
            || buf[offset + 2] != 0
            || buf[offset + 3] != 0
        {
            break;
        }
        let len = u32::from_be_bytes([
            buf[offset + 4],
            buf[offset + 5],
            buf[offset + 6],
            buf[offset + 7],
        ]) as usize;
        if offset + 8 + len > buf.len() {
            break;
        }
        let stream = if buf[offset] == 2 { "stderr" } else { "stdout" };
        let payload = String::from_utf8_lossy(&buf[offset + 8..offset + 8 + len]);
        for line in payload.lines() {
            out.push(ContainerLogLine {
                stream: stream.to_string(),
                line: line.trim_end_matches('\r').to_string(),
            });
        }
        offset += 8 + len;
    }
    (out, offset)
}

pub(super) fn build_create_container_payload(req: &ContainerCreateRequest) -> Value {
    let mut exposed_ports = serde_json::Map::new();
    let mut port_bindings = serde_json::Map::new();
    for port in &req.ports {
        let proto = port.protocol.trim().to_ascii_lowercase();
        let key = format!("{}/{}", port.container_port, proto);
        exposed_ports.insert(key.clone(), json!({}));
        let mut binding = serde_json::Map::new();
        if let Some(ip) = port
            .host_ip
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            binding.insert("HostIp".to_string(), json!(ip));
        }
        binding.insert("HostPort".to_string(), json!(port.host_port.to_string()));
        port_bindings.insert(key, Value::Array(vec![Value::Object(binding)]));
    }

    let binds: Vec<String> = req
        .mounts
        .iter()
        .map(|m| {
            if m.read_only {
                format!("{}:{}:ro", m.source, m.target)
            } else {
                format!("{}:{}", m.source, m.target)
            }
        })
        .collect();

    let mut host_config = serde_json::Map::new();
    host_config.insert("Binds".to_string(), json!(binds));
    host_config.insert("PortBindings".to_string(), Value::Object(port_bindings));
    host_config.insert("Privileged".to_string(), json!(req.privileged));
    if let Some(policy) = req
        .restart_policy
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        host_config.insert("RestartPolicy".to_string(), json!({ "Name": policy }));
    }
    if let Some(net) = req
        .network
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        host_config.insert("NetworkMode".to_string(), json!(net));
    }

    let mut root = serde_json::Map::new();
    root.insert("Image".to_string(), json!(req.image));
    root.insert("Env".to_string(), json!(req.env));
    root.insert("ExposedPorts".to_string(), Value::Object(exposed_ports));
    root.insert("HostConfig".to_string(), Value::Object(host_config));
    if !req.cmd.is_empty() {
        root.insert("Cmd".to_string(), json!(req.cmd));
    }
    Value::Object(root)
}

pub(super) fn build_create_network_payload(req: &ContainerNetworkCreateRequest) -> Value {
    let driver = req
        .driver
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("bridge");

    let mut ipam_cfg = serde_json::Map::new();
    if let Some(subnet) = req
        .subnet
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        ipam_cfg.insert("Subnet".to_string(), json!(subnet));
    }
    if let Some(gw) = req
        .gateway
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        ipam_cfg.insert("Gateway".to_string(), json!(gw));
    }

    let mut options = HashMap::new();
    if let Some(parent) = req
        .parent_interface
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        options.insert("parent".to_string(), parent.to_string());
    }

    let mut body = serde_json::Map::new();
    body.insert("Name".to_string(), json!(req.name));
    body.insert("Driver".to_string(), json!(driver));
    body.insert("Internal".to_string(), json!(req.internal));
    if !ipam_cfg.is_empty() {
        body.insert(
            "IPAM".to_string(),
            json!({ "Config": [Value::Object(ipam_cfg)] }),
        );
    }
    if !options.is_empty() {
        body.insert("Options".to_string(), json!(options));
    }
    Value::Object(body)
}

pub(super) fn parse_container_summary(v: &Value) -> ContainerSummary {
    let id = json_str(v, "Id").unwrap_or_default();
    let short_id = if id.len() > 12 {
        id[..12].to_string()
    } else {
        id
    };
    let name = v
        .get("Names")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(Value::as_str)
        .map(|s| s.trim_start_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| short_id.clone());

    let ports = v
        .get("Ports")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |arr| {
            arr.iter()
                .filter_map(|p| {
                    let container_port = u16::try_from(p.get("PrivatePort")?.as_u64()?).ok()?;
                    let host_port = p
                        .get("PublicPort")
                        .and_then(Value::as_u64)
                        .and_then(|n| u16::try_from(n).ok())
                        .unwrap_or(0);
                    Some(ContainerPortBinding {
                        host_ip: json_str(p, "IP"),
                        host_port,
                        container_port,
                        protocol: json_str(p, "Type").unwrap_or_else(|| "tcp".to_string()),
                    })
                })
                .collect()
        });

    let networks = v
        .get("NetworkSettings")
        .and_then(|ns| ns.get("Networks"))
        .and_then(Value::as_object)
        .map_or_else(Vec::new, |obj| obj.keys().cloned().collect());

    ContainerSummary {
        id: short_id,
        name,
        image: json_str(v, "Image").unwrap_or_default(),
        state: json_str(v, "State")
            .unwrap_or_else(|| "unknown".to_string())
            .to_ascii_lowercase(),
        status: json_str(v, "Status").unwrap_or_default(),
        created_at: format_unix_or_iso(v.get("Created")),
        ports,
        networks,
    }
}

pub(super) fn parse_image_summary(v: &Value) -> ContainerImageSummary {
    let repo_tags = v
        .get("RepoTags")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        });
    let size_bytes = v
        .get("Size")
        .and_then(Value::as_u64)
        .or_else(|| {
            v.get("Size")
                .and_then(Value::as_i64)
                .and_then(|n| u64::try_from(n).ok())
        })
        .unwrap_or(0);
    ContainerImageSummary {
        id: json_str(v, "Id").unwrap_or_default(),
        repo_tags,
        size_bytes,
        created_at: format_unix_or_iso(v.get("Created")),
    }
}

pub(super) fn parse_network_summary(v: &Value) -> ContainerNetworkSummary {
    let cfg = v
        .get("IPAM")
        .and_then(|i| i.get("Config"))
        .and_then(Value::as_array)
        .and_then(|a| a.first());
    ContainerNetworkSummary {
        id: json_str(v, "Id").unwrap_or_default(),
        name: json_str(v, "Name").unwrap_or_default(),
        driver: json_str(v, "Driver").unwrap_or_else(|| "bridge".to_string()),
        subnet: cfg.and_then(|c| json_str(c, "Subnet")),
        gateway: cfg.and_then(|c| json_str(c, "Gateway")),
        internal: v.get("Internal").and_then(Value::as_bool).unwrap_or(false),
    }
}

pub(super) fn parse_volume_summary(v: &Value) -> ContainerVolumeSummary {
    ContainerVolumeSummary {
        name: json_str(v, "Name").unwrap_or_default(),
        driver: json_str(v, "Driver").unwrap_or_else(|| "local".to_string()),
        mountpoint: json_str(v, "Mountpoint").unwrap_or_default(),
        created_at: json_str(v, "CreatedAt"),
    }
}

pub(super) fn json_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(ToString::to_string)
}

pub(super) fn json_u32(v: &Value, key: &str) -> u32 {
    v.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

pub(super) fn format_unix_or_iso(v: Option<&Value>) -> String {
    let Some(val) = v else {
        return String::new();
    };
    if let Some(secs) = val.as_i64() {
        return DateTime::<Utc>::from_timestamp(secs, 0)
            .map_or_else(String::new, |dt| dt.to_rfc3339());
    }
    val.as_str().unwrap_or_default().to_string()
}

pub(super) fn percent_encode_query(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}
