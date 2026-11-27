mod codec;
pub mod config;
pub mod dry_run;
pub mod socket;

use std::future::Future;
use std::path::{Component, Path};
use std::pin::Pin;

use crabrouter_model::{
    ContainerActionResult, ContainerConfig, ContainerCreateRequest, ContainerImageSummary,
    ContainerLogLine, ContainerNetworkCreateRequest, ContainerNetworkSummary, ContainerRuntimeInfo,
    ContainerSummary, ContainerVolumeSummary,
};
use ipnet::IpNet;

use crate::{Result, ServiceError};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
    Remove { force: bool },
}

pub trait ContainerBackend: Send + Sync {
    fn runtime_info<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<ContainerRuntimeInfo>>;

    fn list_containers<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        all: bool,
    ) -> BoxFuture<'a, Result<Vec<ContainerSummary>>>;

    fn inspect_container<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
    ) -> BoxFuture<'a, Result<serde_json::Value>>;

    fn create_container<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        req: &'a ContainerCreateRequest,
    ) -> BoxFuture<'a, Result<ContainerActionResult>>;

    fn container_action<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
        action: ContainerAction,
    ) -> BoxFuture<'a, Result<ContainerActionResult>>;

    fn container_logs<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
        tail: u32,
    ) -> BoxFuture<'a, Result<Vec<ContainerLogLine>>>;

    fn stream_container_logs<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
        tail: u32,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<ContainerLogLine>>>;

    fn list_images<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<Vec<ContainerImageSummary>>>;

    fn pull_image<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        reference: &'a str,
    ) -> BoxFuture<'a, Result<ContainerActionResult>>;

    fn remove_image<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
        force: bool,
    ) -> BoxFuture<'a, Result<ContainerActionResult>>;

    fn list_networks<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<Vec<ContainerNetworkSummary>>>;

    fn create_network<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        req: &'a ContainerNetworkCreateRequest,
    ) -> BoxFuture<'a, Result<ContainerActionResult>>;

    fn remove_network<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
    ) -> BoxFuture<'a, Result<ContainerActionResult>>;

    fn list_volumes<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<Vec<ContainerVolumeSummary>>>;

    fn remove_volume<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        name: &'a str,
        force: bool,
    ) -> BoxFuture<'a, Result<ContainerActionResult>>;
}

pub fn validate_resource_id(raw: &str, label: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > 128
        || trimmed == "."
        || trimmed == ".."
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
    {
        return Err(ServiceError::Render(format!(
            "invalid {label} identifier '{raw}'"
        )));
    }
    Ok(trimmed.to_string())
}

pub fn validate_image_ref(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > 256
        || trimmed
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '?' | '#'))
    {
        return Err(ServiceError::Render(format!(
            "invalid container image reference '{raw}'"
        )));
    }
    Ok(trimmed.to_string())
}

pub fn validate_bind_mount_source(cfg: &ContainerConfig, source: &str) -> Result<()> {
    let trimmed = source.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return Err(ServiceError::Render(
            "mount source cannot be empty".to_string(),
        ));
    }

    if trimmed.starts_with('/') {
        if trimmed.chars().any(char::is_whitespace) {
            return Err(ServiceError::Render(format!(
                "bind mount source '{trimmed}' cannot contain whitespace"
            )));
        }
        let path = Path::new(trimmed);
        if path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(ServiceError::Render(format!(
                "bind mount source '{trimmed}' cannot contain relative path segments"
            )));
        }
        let allowed = cfg
            .allowed_volume_roots
            .iter()
            .any(|root| path.starts_with(Path::new(root)));
        if !allowed {
            return Err(ServiceError::Render(format!(
                "bind mount source '{trimmed}' is outside allowed_volume_roots"
            )));
        }
        return Ok(());
    }

    let valid_named = trimmed.len() <= 128
        && trimmed
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if !valid_named {
        return Err(ServiceError::Render(format!(
            "invalid named volume '{trimmed}'"
        )));
    }
    Ok(())
}

pub fn validate_container_create(
    cfg: &ContainerConfig,
    req: &ContainerCreateRequest,
) -> Result<()> {
    validate_resource_id(&req.name, "container name")?;
    validate_image_ref(&req.image)?;

    if let Some(net) = req
        .network
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        validate_resource_id(net, "network")?;
    }

    if let Some(policy) = req
        .restart_policy
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        && !matches!(policy, "no" | "always" | "on-failure" | "unless-stopped")
    {
        return Err(ServiceError::Render(format!(
            "unsupported restart_policy '{policy}'"
        )));
    }

    for port in &req.ports {
        if port.container_port == 0 {
            return Err(ServiceError::Render(
                "container_port must be non-zero".to_string(),
            ));
        }
        let proto = port.protocol.trim().to_ascii_lowercase();
        if !matches!(proto.as_str(), "tcp" | "udp") {
            return Err(ServiceError::Render(format!(
                "unsupported port protocol '{}'",
                port.protocol
            )));
        }
    }

    for mount in &req.mounts {
        validate_bind_mount_source(cfg, &mount.source)?;
        let target = mount.target.trim();
        if !target.starts_with('/')
            || target == "/"
            || Path::new(target)
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(ServiceError::Render(format!(
                "invalid mount target path '{}'",
                mount.target
            )));
        }
    }

    Ok(())
}

pub fn validate_network_create(req: &ContainerNetworkCreateRequest) -> Result<()> {
    validate_resource_id(&req.name, "network name")?;
    if let Some(driver) = req
        .driver
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        && !matches!(driver, "bridge" | "macvlan" | "ipvlan")
    {
        return Err(ServiceError::Render(format!(
            "unsupported network driver '{driver}'"
        )));
    }
    if let Some(subnet) = req
        .subnet
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        subnet
            .parse::<IpNet>()
            .map_err(|e| ServiceError::Render(format!("invalid network subnet '{subnet}': {e}")))?;
    }
    if let Some(gw) = req
        .gateway
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        gw.parse::<std::net::IpAddr>()
            .map_err(|e| ServiceError::Render(format!("invalid network gateway '{gw}': {e}")))?;
    }
    if let Some(parent) = req
        .parent_interface
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        validate_resource_id(parent, "parent_interface")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crabrouter_model::{ContainerConfig, ContainerCreateRequest, ContainerMountSpec};

    use super::*;

    #[test]
    fn validates_mount_paths_and_rejects_escape() {
        let cfg = ContainerConfig::default();
        assert!(validate_bind_mount_source(&cfg, "/var/lib/crabrouter/volumes/ha").is_ok());
        assert!(validate_bind_mount_source(&cfg, "/mnt/nvme/media").is_ok());
        assert!(validate_bind_mount_source(&cfg, "app_data-1").is_ok());

        assert!(validate_bind_mount_source(&cfg, "/etc/shadow").is_err());
        assert!(validate_bind_mount_source(&cfg, "/var/lib/crabrouter/router.toml").is_err());
        assert!(validate_bind_mount_source(&cfg, "/mnt/../etc/passwd").is_err());
    }

    #[test]
    fn validates_resource_ids_and_create_request() {
        let cfg = ContainerConfig::default();
        assert!(validate_resource_id("uptime-kuma_1", "container").is_ok());
        assert!(validate_resource_id("../etc", "container").is_err());
        assert!(validate_resource_id("foo/bar", "container").is_err());

        let req = ContainerCreateRequest {
            name: "my-app".to_string(),
            image: "docker.io/library/nginx:alpine".to_string(),
            cmd: Vec::new(),
            env: Vec::new(),
            ports: Vec::new(),
            mounts: vec![ContainerMountSpec {
                source: "/var/lib/crabrouter/volumes/my-app".to_string(),
                target: "/data".to_string(),
                read_only: false,
            }],
            network: Some("podman".to_string()),
            restart_policy: Some("unless-stopped".to_string()),
            privileged: false,
            start_immediately: true,
        };
        assert!(validate_container_create(&cfg, &req).is_ok());
    }
}
