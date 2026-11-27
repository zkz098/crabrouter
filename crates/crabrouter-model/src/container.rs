use std::collections::HashSet;
use std::path::Path;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ContainerRuntimeKind {
    #[default]
    Podman,
    Docker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[allow(clippy::struct_excessive_bools)]
pub struct ContainerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub runtime: ContainerRuntimeKind,
    #[serde(default)]
    pub socket_path: String,
    #[serde(default = "default_storage_root")]
    pub storage_root: String,
    #[serde(default = "default_allowed_volume_roots")]
    pub allowed_volume_roots: Vec<String>,
    #[serde(default = "default_subnet")]
    pub default_subnet: String,
    #[serde(default = "default_bridge_interfaces")]
    pub bridge_interfaces: Vec<String>,
    #[serde(default = "default_dns_bind_port")]
    pub dns_bind_port: u16,
    #[serde(default = "default_true")]
    pub allow_wan_access: bool,
    #[serde(default)]
    pub allow_lan_access: bool,
    #[serde(default)]
    pub multi_wan_enabled: bool,
    #[serde(default)]
    pub memory_max_mb: u32,
    #[serde(default)]
    pub cpu_quota_percent: u32,
    #[serde(default)]
    pub registry_mirrors: Vec<String>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            runtime: ContainerRuntimeKind::Podman,
            socket_path: String::new(),
            storage_root: default_storage_root(),
            allowed_volume_roots: default_allowed_volume_roots(),
            default_subnet: default_subnet(),
            bridge_interfaces: default_bridge_interfaces(),
            dns_bind_port: default_dns_bind_port(),
            allow_wan_access: true,
            allow_lan_access: false,
            multi_wan_enabled: false,
            memory_max_mb: 0,
            cpu_quota_percent: 0,
            registry_mirrors: Vec::new(),
        }
    }
}

impl ContainerConfig {
    pub fn effective_socket_path(&self) -> &str {
        let trimmed = self.socket_path.trim();
        if !trimmed.is_empty() {
            return trimmed;
        }
        match self.runtime {
            ContainerRuntimeKind::Podman => "/run/podman/podman.sock",
            ContainerRuntimeKind::Docker => "/run/docker.sock",
        }
    }

    pub fn effective_bridge_interfaces(&self) -> Vec<String> {
        let cleaned: Vec<String> = self
            .bridge_interfaces
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();
        if !cleaned.is_empty() {
            return cleaned;
        }
        match self.runtime {
            ContainerRuntimeKind::Podman => vec!["podman*".to_string()],
            ContainerRuntimeKind::Docker => vec!["docker0".to_string(), "br-*".to_string()],
        }
    }

    pub fn validate(&self, crabdns_port: u16, dnsmasq_port: u16) -> Result<()> {
        let net: IpNet = self.default_subnet.parse().map_err(|e| {
            ModelError::Validation(format!(
                "containers.default_subnet '{}' is invalid: {e}",
                self.default_subnet
            ))
        })?;
        match net {
            IpNet::V4(v4) if v4.prefix_len() <= 28 => {}
            IpNet::V4(_) => {
                return Err(ModelError::Validation(format!(
                    "containers.default_subnet '{}' prefix length must be <= 28",
                    self.default_subnet
                )));
            }
            IpNet::V6(_) => {
                return Err(ModelError::Validation(format!(
                    "containers.default_subnet '{}' must be an IPv4 CIDR",
                    self.default_subnet
                )));
            }
        }

        if self.dns_bind_port < 1024
            || self.dns_bind_port == crabdns_port
            || self.dns_bind_port == dnsmasq_port
        {
            return Err(ModelError::Validation(format!(
                "containers.dns_bind_port {} must be >= 1024 and distinct from DNS ports ({crabdns_port}, {dnsmasq_port})",
                self.dns_bind_port
            )));
        }

        validate_abs_clean_path(&self.storage_root, "containers.storage_root")?;

        let forbidden_roots: HashSet<&str> = [
            "/",
            "/etc",
            "/usr",
            "/boot",
            "/var",
            "/var/lib/crabrouter",
            "/proc",
            "/sys",
            "/dev",
        ]
        .into_iter()
        .collect();

        for root in &self.allowed_volume_roots {
            validate_abs_clean_path(root, "containers.allowed_volume_roots")?;
            let normalized = root.trim_end_matches('/');
            let normalized = if normalized.is_empty() {
                "/"
            } else {
                normalized
            };
            if forbidden_roots.contains(normalized) {
                return Err(ModelError::Validation(format!(
                    "containers.allowed_volume_roots cannot include sensitive system path '{root}'"
                )));
            }
        }

        for iface in &self.bridge_interfaces {
            let trimmed = iface.trim();
            if trimmed.is_empty()
                || trimmed.len() > 15
                || !trimmed
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '*'))
            {
                return Err(ModelError::Validation(format!(
                    "containers.bridge_interfaces contains invalid pattern '{iface}'"
                )));
            }
        }

        for mirror in &self.registry_mirrors {
            if mirror.is_empty()
                || mirror
                    .chars()
                    .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\n' | '\r'))
            {
                return Err(ModelError::Validation(format!(
                    "containers.registry_mirrors contains invalid entry '{mirror}'"
                )));
            }
        }

        Ok(())
    }
}

fn validate_abs_clean_path(raw: &str, field: &str) -> Result<()> {
    if !raw.starts_with('/')
        || raw.chars().any(char::is_whitespace)
        || Path::new(raw)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ModelError::Validation(format!(
            "{field} path '{raw}' must be an absolute path without '..' or whitespace"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerRuntimeInfo {
    pub enabled: bool,
    pub available: bool,
    pub runtime: ContainerRuntimeKind,
    pub socket_path: String,
    pub version: Option<String>,
    pub api_version: Option<String>,
    pub containers_running: u32,
    pub containers_stopped: u32,
    pub containers_total: u32,
    pub images_count: u32,
    pub storage_root: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerPortBinding {
    pub host_ip: Option<String>,
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerMountSpec {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerSummary {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub created_at: String,
    pub ports: Vec<ContainerPortBinding>,
    pub networks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerCreateRequest {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub cmd: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub ports: Vec<ContainerPortBinding>,
    #[serde(default)]
    pub mounts: Vec<ContainerMountSpec>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub restart_policy: Option<String>,
    #[serde(default)]
    pub privileged: bool,
    #[serde(default = "default_true")]
    pub start_immediately: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerActionResult {
    pub ok: bool,
    pub id: Option<String>,
    pub action: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerLogLine {
    pub stream: String,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerImageSummary {
    pub id: String,
    pub repo_tags: Vec<String>,
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerImagePullRequest {
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerNetworkSummary {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub subnet: Option<String>,
    pub gateway: Option<String>,
    pub internal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerNetworkCreateRequest {
    pub name: String,
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub subnet: Option<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub parent_interface: Option<String>,
    #[serde(default)]
    pub internal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContainerVolumeSummary {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: Option<String>,
}

fn default_storage_root() -> String {
    "/var/lib/containers/storage".to_string()
}

fn default_allowed_volume_roots() -> Vec<String> {
    vec![
        "/var/lib/crabrouter/volumes".to_string(),
        "/mnt".to_string(),
    ]
}

fn default_subnet() -> String {
    "10.88.0.0/16".to_string()
}

fn default_bridge_interfaces() -> Vec<String> {
    vec!["podman*".to_string()]
}

const fn default_dns_bind_port() -> u16 {
    5355
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid_and_has_expected_socket() {
        let cfg = ContainerConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.runtime, ContainerRuntimeKind::Podman);
        assert_eq!(cfg.effective_socket_path(), "/run/podman/podman.sock");
        assert_eq!(cfg.effective_bridge_interfaces(), vec!["podman*"]);
        assert!(cfg.validate(53, 5354).is_ok());

        let docker = ContainerConfig {
            runtime: ContainerRuntimeKind::Docker,
            bridge_interfaces: Vec::new(),
            ..Default::default()
        };
        assert_eq!(docker.effective_socket_path(), "/run/docker.sock");
        assert_eq!(
            docker.effective_bridge_interfaces(),
            vec!["docker0", "br-*"]
        );
    }

    #[test]
    fn rejects_dns_port_collision_and_bad_subnet() {
        let mut cfg = ContainerConfig {
            dns_bind_port: 53,
            ..Default::default()
        };
        assert!(cfg.validate(53, 5354).is_err());

        cfg.dns_bind_port = 5354;
        assert!(cfg.validate(53, 5354).is_err());

        cfg.dns_bind_port = 5355;
        cfg.default_subnet = "10.88.0.0/30".to_string();
        assert!(cfg.validate(53, 5354).is_err());

        cfg.default_subnet = "fd00::/64".to_string();
        assert!(cfg.validate(53, 5354).is_err());
    }

    #[test]
    fn rejects_forbidden_volume_roots_and_invalid_patterns() {
        for bad_root in ["/", "/etc", "/var/lib/crabrouter", "/var/lib/../etc"] {
            let cfg = ContainerConfig {
                allowed_volume_roots: vec![bad_root.to_string()],
                ..Default::default()
            };
            assert!(
                cfg.validate(53, 5354).is_err(),
                "expected {bad_root} to be rejected"
            );
        }

        let cfg = ContainerConfig {
            bridge_interfaces: vec!["waytoolongbridgepattern".to_string()],
            ..Default::default()
        };
        assert!(cfg.validate(53, 5354).is_err());

        let cfg = ContainerConfig {
            registry_mirrors: vec!["https://mirror.example.com\"".to_string()],
            ..Default::default()
        };
        assert!(cfg.validate(53, 5354).is_err());
    }
}
