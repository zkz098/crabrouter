use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use crabrouter_model::{
    ContainerActionResult, ContainerConfig, ContainerCreateRequest, ContainerImageSummary,
    ContainerLogLine, ContainerNetworkCreateRequest, ContainerNetworkSummary, ContainerPortBinding,
    ContainerRuntimeInfo, ContainerSummary, ContainerVolumeSummary,
};
use tokio::sync::Mutex;

use super::{
    BoxFuture, ContainerAction, ContainerBackend, validate_container_create, validate_image_ref,
    validate_network_create, validate_resource_id,
};
use crate::{Result, ServiceError};

#[derive(Debug)]
struct DryRunState {
    containers: Vec<ContainerSummary>,
    logs: HashMap<String, Vec<ContainerLogLine>>,
    images: Vec<ContainerImageSummary>,
    networks: Vec<ContainerNetworkSummary>,
    volumes: Vec<ContainerVolumeSummary>,
}

impl Default for DryRunState {
    fn default() -> Self {
        let mut logs = HashMap::new();
        logs.insert(
            "ha-001".to_string(),
            vec![
                ContainerLogLine {
                    stream: "stdout".to_string(),
                    line: "s6-rc: info: service init-homeassistant successfully started"
                        .to_string(),
                },
                ContainerLogLine {
                    stream: "stdout".to_string(),
                    line: "Home Assistant core initialized on :8123".to_string(),
                },
            ],
        );
        logs.insert(
            "kuma-002".to_string(),
            vec![ContainerLogLine {
                stream: "stdout".to_string(),
                line: "Listening on 3001".to_string(),
            }],
        );

        Self {
            containers: vec![
                ContainerSummary {
                    id: "ha-001".to_string(),
                    name: "home-assistant".to_string(),
                    image: "ghcr.io/home-assistant/home-assistant:stable".to_string(),
                    state: "running".to_string(),
                    status: "Up 2 hours (dry-run)".to_string(),
                    created_at: "2026-11-27T08:00:00Z".to_string(),
                    ports: Vec::new(),
                    networks: vec!["host".to_string()],
                },
                ContainerSummary {
                    id: "kuma-002".to_string(),
                    name: "uptime-kuma".to_string(),
                    image: "docker.io/louislam/uptime-kuma:1".to_string(),
                    state: "exited".to_string(),
                    status: "Exited (0) 10 minutes ago (dry-run)".to_string(),
                    created_at: "2026-11-26T12:00:00Z".to_string(),
                    ports: vec![ContainerPortBinding {
                        host_ip: Some("0.0.0.0".to_string()),
                        host_port: 3001,
                        container_port: 3001,
                        protocol: "tcp".to_string(),
                    }],
                    networks: vec!["podman".to_string()],
                },
            ],
            logs,
            images: vec![
                ContainerImageSummary {
                    id: "sha256:aa11bb22cc33".to_string(),
                    repo_tags: vec!["ghcr.io/home-assistant/home-assistant:stable".to_string()],
                    size_bytes: 642_252_800,
                    created_at: "2026-11-25T10:00:00Z".to_string(),
                },
                ContainerImageSummary {
                    id: "sha256:dd44ee55ff66".to_string(),
                    repo_tags: vec!["docker.io/louislam/uptime-kuma:1".to_string()],
                    size_bytes: 289_406_976,
                    created_at: "2026-11-24T10:00:00Z".to_string(),
                },
            ],
            networks: vec![ContainerNetworkSummary {
                id: "net-podman0".to_string(),
                name: "podman".to_string(),
                driver: "bridge".to_string(),
                subnet: Some("10.88.0.0/16".to_string()),
                gateway: Some("10.88.0.1".to_string()),
                internal: false,
            }],
            volumes: vec![ContainerVolumeSummary {
                name: "uptime-kuma-data".to_string(),
                driver: "local".to_string(),
                mountpoint: "/var/lib/crabrouter/volumes/uptime-kuma".to_string(),
                created_at: Some("2026-11-26T12:00:00Z".to_string()),
            }],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DryRunContainerBackend {
    state: Arc<Mutex<DryRunState>>,
}

impl DryRunContainerBackend {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ContainerBackend for DryRunContainerBackend {
    fn runtime_info<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<ContainerRuntimeInfo>> {
        Box::pin(async move {
            let guard = self.state.lock().await;
            let running = u32::try_from(
                guard
                    .containers
                    .iter()
                    .filter(|c| c.state == "running")
                    .count(),
            )
            .unwrap_or(u32::MAX);
            let total = u32::try_from(guard.containers.len()).unwrap_or(u32::MAX);
            let images_count = u32::try_from(guard.images.len()).unwrap_or(u32::MAX);
            Ok(ContainerRuntimeInfo {
                enabled: cfg.enabled,
                available: true,
                runtime: cfg.runtime,
                socket_path: cfg.effective_socket_path().to_string(),
                version: Some("5.4.2 (dry-run)".to_string()),
                api_version: Some("1.44".to_string()),
                containers_running: running,
                containers_stopped: total.saturating_sub(running),
                containers_total: total,
                images_count,
                storage_root: cfg.storage_root.clone(),
                error: None,
            })
        })
    }

    fn list_containers<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
        all: bool,
    ) -> BoxFuture<'a, Result<Vec<ContainerSummary>>> {
        Box::pin(async move {
            let guard = self.state.lock().await;
            let list = if all {
                guard.containers.clone()
            } else {
                guard
                    .containers
                    .iter()
                    .filter(|c| c.state == "running")
                    .cloned()
                    .collect()
            };
            Ok(list)
        })
    }

    fn inspect_container<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
        id: &'a str,
    ) -> BoxFuture<'a, Result<serde_json::Value>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "container")?;
            let guard = self.state.lock().await;
            let found = guard
                .containers
                .iter()
                .find(|c| c.id == clean || c.name == clean)
                .ok_or_else(|| {
                    ServiceError::Orchestration(format!("container '{clean}' not found"))
                })?;
            serde_json::to_value(found).map_err(|e| ServiceError::Render(e.to_string()))
        })
    }

    fn create_container<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        req: &'a ContainerCreateRequest,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            validate_container_create(cfg, req)?;
            let mut guard = self.state.lock().await;
            if guard.containers.iter().any(|c| c.name == req.name) {
                return Err(ServiceError::Orchestration(format!(
                    "container '{}' already exists",
                    req.name
                )));
            }
            let id = format!("dry-{}", req.name);
            let state = if req.start_immediately {
                "running"
            } else {
                "created"
            };
            let status = if req.start_immediately {
                "Up 1 second (dry-run)"
            } else {
                "Created (dry-run)"
            };
            let networks = vec![
                req.network
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "podman".to_string()),
            ];
            guard.containers.push(ContainerSummary {
                id: id.clone(),
                name: req.name.clone(),
                image: req.image.clone(),
                state: state.to_string(),
                status: status.to_string(),
                created_at: Utc::now().to_rfc3339(),
                ports: req.ports.clone(),
                networks,
            });
            guard.logs.insert(
                id.clone(),
                vec![ContainerLogLine {
                    stream: "stdout".to_string(),
                    line: format!("dry-run: container '{}' initialized", req.name),
                }],
            );
            Ok(ContainerActionResult {
                ok: true,
                id: Some(id),
                action: "create".to_string(),
                message: format!("created container '{}' (dry-run)", req.name),
            })
        })
    }

    fn container_action<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
        id: &'a str,
        action: ContainerAction,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "container")?;
            let mut guard = self.state.lock().await;
            let idx = guard
                .containers
                .iter()
                .position(|c| c.id == clean || c.name == clean)
                .ok_or_else(|| {
                    ServiceError::Orchestration(format!("container '{clean}' not found"))
                })?;

            match action {
                ContainerAction::Start => {
                    guard.containers[idx].state = "running".to_string();
                    guard.containers[idx].status = "Up 1 second (dry-run)".to_string();
                    Ok(action_ok(&clean, "start", "container started (dry-run)"))
                }
                ContainerAction::Stop => {
                    guard.containers[idx].state = "exited".to_string();
                    guard.containers[idx].status = "Exited (0) just now (dry-run)".to_string();
                    Ok(action_ok(&clean, "stop", "container stopped (dry-run)"))
                }
                ContainerAction::Restart => {
                    guard.containers[idx].state = "running".to_string();
                    guard.containers[idx].status = "Up 1 second (restarted dry-run)".to_string();
                    Ok(action_ok(
                        &clean,
                        "restart",
                        "container restarted (dry-run)",
                    ))
                }
                ContainerAction::Remove { force } => {
                    if !force && guard.containers[idx].state == "running" {
                        return Err(ServiceError::Orchestration(format!(
                            "cannot remove running container '{clean}' without force"
                        )));
                    }
                    let removed = guard.containers.remove(idx);
                    guard.logs.remove(&removed.id);
                    Ok(action_ok(
                        &removed.id,
                        "remove",
                        "container removed (dry-run)",
                    ))
                }
            }
        })
    }

    fn container_logs<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
        id: &'a str,
        tail: u32,
    ) -> BoxFuture<'a, Result<Vec<ContainerLogLine>>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "container")?;
            let guard = self.state.lock().await;
            let cid = guard
                .containers
                .iter()
                .find(|c| c.id == clean || c.name == clean)
                .map(|c| c.id.clone())
                .ok_or_else(|| {
                    ServiceError::Orchestration(format!("container '{clean}' not found"))
                })?;
            let lines = guard.logs.get(&cid).cloned().unwrap_or_default();
            let keep = usize::try_from(tail).unwrap_or(usize::MAX);
            let skip = lines.len().saturating_sub(keep);
            Ok(lines.into_iter().skip(skip).collect())
        })
    }

    fn stream_container_logs<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
        tail: u32,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<ContainerLogLine>>> {
        Box::pin(async move {
            let lines = self.container_logs(cfg, id, tail).await?;
            let (tx, rx) = tokio::sync::mpsc::channel(64);
            tokio::spawn(async move {
                for line in lines {
                    if tx.send(line).await.is_err() {
                        break;
                    }
                }
            });
            Ok(rx)
        })
    }

    fn list_images<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<Vec<ContainerImageSummary>>> {
        Box::pin(async move { Ok(self.state.lock().await.images.clone()) })
    }

    fn pull_image<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
        reference: &'a str,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_image_ref(reference)?;
            let mut guard = self.state.lock().await;
            if !guard
                .images
                .iter()
                .any(|img| img.repo_tags.iter().any(|t| t == &clean))
            {
                let id = format!("sha256:dry{:08x}", guard.images.len() + 1);
                guard.images.push(ContainerImageSummary {
                    id: id.clone(),
                    repo_tags: vec![clean.clone()],
                    size_bytes: 128_000_000,
                    created_at: Utc::now().to_rfc3339(),
                });
            }
            Ok(ContainerActionResult {
                ok: true,
                id: Some(clean.clone()),
                action: "pull".to_string(),
                message: format!("pulled image '{clean}' (dry-run)"),
            })
        })
    }

    fn remove_image<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
        id: &'a str,
        _force: bool,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "image")?;
            let mut guard = self.state.lock().await;
            let before = guard.images.len();
            guard
                .images
                .retain(|img| img.id != clean && !img.repo_tags.iter().any(|t| t == &clean));
            if guard.images.len() == before {
                return Err(ServiceError::Orchestration(format!(
                    "image '{clean}' not found"
                )));
            }
            Ok(action_ok(&clean, "remove_image", "image removed (dry-run)"))
        })
    }

    fn list_networks<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<Vec<ContainerNetworkSummary>>> {
        Box::pin(async move { Ok(self.state.lock().await.networks.clone()) })
    }

    fn create_network<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
        req: &'a ContainerNetworkCreateRequest,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            validate_network_create(req)?;
            let mut guard = self.state.lock().await;
            if guard.networks.iter().any(|n| n.name == req.name) {
                return Err(ServiceError::Orchestration(format!(
                    "network '{}' already exists",
                    req.name
                )));
            }
            let id = format!("net-{}", req.name);
            guard.networks.push(ContainerNetworkSummary {
                id: id.clone(),
                name: req.name.clone(),
                driver: req.driver.clone().unwrap_or_else(|| "bridge".to_string()),
                subnet: req.subnet.clone(),
                gateway: req.gateway.clone(),
                internal: req.internal,
            });
            Ok(action_ok(
                &id,
                "create_network",
                "network created (dry-run)",
            ))
        })
    }

    fn remove_network<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
        id: &'a str,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "network")?;
            let mut guard = self.state.lock().await;
            let before = guard.networks.len();
            guard.networks.retain(|n| n.id != clean && n.name != clean);
            if guard.networks.len() == before {
                return Err(ServiceError::Orchestration(format!(
                    "network '{clean}' not found"
                )));
            }
            Ok(action_ok(
                &clean,
                "remove_network",
                "network removed (dry-run)",
            ))
        })
    }

    fn list_volumes<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<Vec<ContainerVolumeSummary>>> {
        Box::pin(async move { Ok(self.state.lock().await.volumes.clone()) })
    }

    fn remove_volume<'a>(
        &'a self,
        _cfg: &'a ContainerConfig,
        name: &'a str,
        _force: bool,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_resource_id(name, "volume")?;
            let mut guard = self.state.lock().await;
            let before = guard.volumes.len();
            guard.volumes.retain(|v| v.name != clean);
            if guard.volumes.len() == before {
                return Err(ServiceError::Orchestration(format!(
                    "volume '{clean}' not found"
                )));
            }
            Ok(action_ok(
                &clean,
                "remove_volume",
                "volume removed (dry-run)",
            ))
        })
    }
}

fn action_ok(id: &str, action: &str, message: &str) -> ContainerActionResult {
    ContainerActionResult {
        ok: true,
        id: Some(id.to_string()),
        action: action.to_string(),
        message: message.to_string(),
    }
}
