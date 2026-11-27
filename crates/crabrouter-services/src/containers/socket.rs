#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;

use std::time::Duration;

use crabrouter_model::{
    ContainerActionResult, ContainerConfig, ContainerCreateRequest, ContainerImageSummary,
    ContainerLogLine, ContainerNetworkCreateRequest, ContainerNetworkSummary, ContainerRuntimeInfo,
    ContainerSummary, ContainerVolumeSummary,
};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use tokio::net::UnixStream;

use super::codec::{
    build_create_container_payload, build_create_network_payload, decode_docker_log_frames,
    drain_multiplexed_frames, is_multiplexed_stream, json_str, json_u32, parse_container_summary,
    parse_image_summary, parse_network_summary, parse_volume_summary, percent_encode_query,
};
use super::{
    BoxFuture, ContainerAction, ContainerBackend, validate_container_create, validate_image_ref,
    validate_network_create, validate_resource_id,
};
use crate::{Result, ServiceError};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const PULL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Default, Clone, Copy)]
pub struct SocketContainerBackend;

impl ContainerBackend for SocketContainerBackend {
    fn runtime_info<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<ContainerRuntimeInfo>> {
        Box::pin(async move {
            let sock = cfg.effective_socket_path();
            if !std::path::Path::new(sock).exists() {
                return Ok(unavailable_runtime_info(
                    cfg,
                    format!("container socket '{sock}' not found"),
                ));
            }
            let ver = match uds_request_json(sock, Method::GET, "/version", None, DEFAULT_TIMEOUT)
                .await
            {
                Ok(v) => v,
                Err(err) => return Ok(unavailable_runtime_info(cfg, err.to_string())),
            };
            let info =
                match uds_request_json(sock, Method::GET, "/info", None, DEFAULT_TIMEOUT).await {
                    Ok(i) => i,
                    Err(err) => return Ok(unavailable_runtime_info(cfg, err.to_string())),
                };
            Ok(ContainerRuntimeInfo {
                enabled: cfg.enabled,
                available: true,
                runtime: cfg.runtime,
                socket_path: sock.to_string(),
                version: json_str(&ver, "Version"),
                api_version: json_str(&ver, "ApiVersion"),
                containers_running: json_u32(&info, "ContainersRunning"),
                containers_stopped: json_u32(&info, "ContainersStopped"),
                containers_total: json_u32(&info, "Containers"),
                images_count: json_u32(&info, "Images"),
                storage_root: cfg.storage_root.clone(),
                error: None,
            })
        })
    }

    fn list_containers<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        all: bool,
    ) -> BoxFuture<'a, Result<Vec<ContainerSummary>>> {
        Box::pin(async move {
            let path = format!("/containers/json?all={all}");
            let val = uds_request_json(
                cfg.effective_socket_path(),
                Method::GET,
                &path,
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;
            let items = val.as_array().ok_or_else(|| {
                ServiceError::Orchestration("expected array from /containers/json".into())
            })?;
            Ok(items.iter().map(parse_container_summary).collect())
        })
    }

    fn inspect_container<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Value>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "container")?;
            let path = format!("/containers/{clean}/json");
            uds_request_json(
                cfg.effective_socket_path(),
                Method::GET,
                &path,
                None,
                DEFAULT_TIMEOUT,
            )
            .await
        })
    }

    fn create_container<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        req: &'a ContainerCreateRequest,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            validate_container_create(cfg, req)?;
            let sock = cfg.effective_socket_path();
            let payload = build_create_container_payload(req);
            let encoded_name = percent_encode_query(&req.name);
            let path = format!("/containers/create?name={encoded_name}");
            let resp = uds_request_json(
                sock,
                Method::POST,
                &path,
                Some(payload.to_string()),
                DEFAULT_TIMEOUT,
            )
            .await?;
            let id = json_str(&resp, "Id").unwrap_or_else(|| req.name.clone());

            if req.start_immediately {
                let start_path = format!("/containers/{id}/start");
                uds_request_ok(sock, Method::POST, &start_path, None, DEFAULT_TIMEOUT).await?;
            }

            Ok(ContainerActionResult {
                ok: true,
                id: Some(id),
                action: "create".to_string(),
                message: format!("created container '{}'", req.name),
            })
        })
    }

    fn container_action<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
        action: ContainerAction,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "container")?;
            let sock = cfg.effective_socket_path();
            let (method, path, action_name) = match action {
                ContainerAction::Start => {
                    (Method::POST, format!("/containers/{clean}/start"), "start")
                }
                ContainerAction::Stop => {
                    (Method::POST, format!("/containers/{clean}/stop"), "stop")
                }
                ContainerAction::Restart => (
                    Method::POST,
                    format!("/containers/{clean}/restart"),
                    "restart",
                ),
                ContainerAction::Remove { force } => (
                    Method::DELETE,
                    format!("/containers/{clean}?force={force}"),
                    "remove",
                ),
            };
            uds_request_ok(sock, method, &path, None, DEFAULT_TIMEOUT).await?;
            Ok(ContainerActionResult {
                ok: true,
                id: Some(clean.clone()),
                action: action_name.to_string(),
                message: format!("container '{clean}' {action_name} succeeded"),
            })
        })
    }

    fn container_logs<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
        tail: u32,
    ) -> BoxFuture<'a, Result<Vec<ContainerLogLine>>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "container")?;
            let tail_val = tail.clamp(1, 5000);
            let path = format!("/containers/{clean}/logs?stdout=1&stderr=1&tail={tail_val}");
            let (status, bytes) = uds_request_raw(
                cfg.effective_socket_path(),
                Method::GET,
                &path,
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;
            if !status.is_success() {
                return Err(http_status_error(status, &bytes));
            }
            Ok(decode_docker_log_frames(&bytes))
        })
    }

    fn stream_container_logs<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
        tail: u32,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<ContainerLogLine>>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "container")?;
            let tail_val = tail.clamp(1, 2000);
            let path =
                format!("/containers/{clean}/logs?follow=1&stdout=1&stderr=1&tail={tail_val}");
            let resp = uds_send_request(
                cfg.effective_socket_path(),
                Method::GET,
                &path,
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;
            let status = resp.status();
            if !status.is_success() {
                let bytes = collect_body_limited(resp.into_body(), DEFAULT_TIMEOUT).await?;
                return Err(http_status_error(status, &bytes));
            }

            let (tx, rx) = tokio::sync::mpsc::channel(128);
            tokio::spawn(async move {
                pump_log_stream(resp.into_body(), tx).await;
            });
            Ok(rx)
        })
    }

    fn list_images<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<Vec<ContainerImageSummary>>> {
        Box::pin(async move {
            let val = uds_request_json(
                cfg.effective_socket_path(),
                Method::GET,
                "/images/json",
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;
            let items = val.as_array().ok_or_else(|| {
                ServiceError::Orchestration("expected array from /images/json".into())
            })?;
            Ok(items.iter().map(parse_image_summary).collect())
        })
    }

    fn pull_image<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        reference: &'a str,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_image_ref(reference)?;
            let encoded = percent_encode_query(&clean);
            let path = format!("/images/create?fromImage={encoded}");
            uds_request_ok(
                cfg.effective_socket_path(),
                Method::POST,
                &path,
                None,
                PULL_TIMEOUT,
            )
            .await?;
            Ok(ContainerActionResult {
                ok: true,
                id: Some(clean.clone()),
                action: "pull".to_string(),
                message: format!("pulled image '{clean}'"),
            })
        })
    }

    fn remove_image<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
        force: bool,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "image")?;
            let path = format!("/images/{clean}?force={force}");
            uds_request_ok(
                cfg.effective_socket_path(),
                Method::DELETE,
                &path,
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;
            Ok(ContainerActionResult {
                ok: true,
                id: Some(clean.clone()),
                action: "remove_image".to_string(),
                message: format!("removed image '{clean}'"),
            })
        })
    }

    fn list_networks<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<Vec<ContainerNetworkSummary>>> {
        Box::pin(async move {
            let val = uds_request_json(
                cfg.effective_socket_path(),
                Method::GET,
                "/networks",
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;
            let items = val.as_array().ok_or_else(|| {
                ServiceError::Orchestration("expected array from /networks".into())
            })?;
            Ok(items.iter().map(parse_network_summary).collect())
        })
    }

    fn create_network<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        req: &'a ContainerNetworkCreateRequest,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            validate_network_create(req)?;
            let payload = build_create_network_payload(req);
            let val = uds_request_json(
                cfg.effective_socket_path(),
                Method::POST,
                "/networks/create",
                Some(payload.to_string()),
                DEFAULT_TIMEOUT,
            )
            .await?;
            let id = json_str(&val, "Id").unwrap_or_else(|| req.name.clone());
            Ok(ContainerActionResult {
                ok: true,
                id: Some(id),
                action: "create_network".to_string(),
                message: format!("created network '{}'", req.name),
            })
        })
    }

    fn remove_network<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        id: &'a str,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_resource_id(id, "network")?;
            let path = format!("/networks/{clean}");
            uds_request_ok(
                cfg.effective_socket_path(),
                Method::DELETE,
                &path,
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;
            Ok(ContainerActionResult {
                ok: true,
                id: Some(clean.clone()),
                action: "remove_network".to_string(),
                message: format!("removed network '{clean}'"),
            })
        })
    }

    fn list_volumes<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
    ) -> BoxFuture<'a, Result<Vec<ContainerVolumeSummary>>> {
        Box::pin(async move {
            let val = uds_request_json(
                cfg.effective_socket_path(),
                Method::GET,
                "/volumes",
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;
            let list = val
                .get("Volumes")
                .and_then(Value::as_array)
                .map_or_else(Vec::new, |arr| {
                    arr.iter().map(parse_volume_summary).collect()
                });
            Ok(list)
        })
    }

    fn remove_volume<'a>(
        &'a self,
        cfg: &'a ContainerConfig,
        name: &'a str,
        force: bool,
    ) -> BoxFuture<'a, Result<ContainerActionResult>> {
        Box::pin(async move {
            let clean = validate_resource_id(name, "volume")?;
            let path = format!("/volumes/{clean}?force={force}");
            uds_request_ok(
                cfg.effective_socket_path(),
                Method::DELETE,
                &path,
                None,
                DEFAULT_TIMEOUT,
            )
            .await?;
            Ok(ContainerActionResult {
                ok: true,
                id: Some(clean.clone()),
                action: "remove_volume".to_string(),
                message: format!("removed volume '{clean}'"),
            })
        })
    }
}

fn unavailable_runtime_info(cfg: &ContainerConfig, error: String) -> ContainerRuntimeInfo {
    ContainerRuntimeInfo {
        enabled: cfg.enabled,
        available: false,
        runtime: cfg.runtime,
        socket_path: cfg.effective_socket_path().to_string(),
        version: None,
        api_version: None,
        containers_running: 0,
        containers_stopped: 0,
        containers_total: 0,
        images_count: 0,
        storage_root: cfg.storage_root.clone(),
        error: Some(error),
    }
}

async fn uds_send_request(
    socket_path: &str,
    method: Method,
    path: &str,
    body: Option<String>,
    timeout: Duration,
) -> Result<Response<Incoming>> {
    let stream = tokio::time::timeout(timeout, UnixStream::connect(socket_path))
        .await
        .map_err(|_| ServiceError::Orchestration(format!("connect {socket_path} timed out")))?
        .map_err(|e| ServiceError::Orchestration(format!("connect {socket_path} failed: {e}")))?;

    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(|e| ServiceError::Orchestration(format!("UDS HTTP handshake failed: {e}")))?;

    tokio::spawn(async move {
        if let Err(err) = conn.await {
            tracing::debug!(error = %err, "container UDS HTTP connection closed");
        }
    });

    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("Host", "localhost");
    let full_body = if let Some(json_body) = body {
        builder = builder.header("Content-Type", "application/json");
        Full::new(Bytes::from(json_body))
    } else {
        Full::new(Bytes::new())
    };
    let req = builder
        .body(full_body)
        .map_err(|e| ServiceError::Render(format!("build UDS HTTP request failed: {e}")))?;

    tokio::time::timeout(timeout, sender.send_request(req))
        .await
        .map_err(|_| ServiceError::Orchestration(format!("container request {path} timed out")))?
        .map_err(|e| ServiceError::Orchestration(format!("container request {path} failed: {e}")))
}

async fn collect_body_limited(body: Incoming, timeout: Duration) -> Result<Bytes> {
    let collected = tokio::time::timeout(timeout, body.collect())
        .await
        .map_err(|_| ServiceError::Orchestration("reading container response timed out".into()))?
        .map_err(|e| ServiceError::Orchestration(format!("read container response failed: {e}")))?;
    Ok(collected.to_bytes())
}

async fn uds_request_raw(
    socket_path: &str,
    method: Method,
    path: &str,
    body: Option<String>,
    timeout: Duration,
) -> Result<(StatusCode, Bytes)> {
    let resp = uds_send_request(socket_path, method, path, body, timeout).await?;
    let status = resp.status();
    let bytes = collect_body_limited(resp.into_body(), timeout).await?;
    Ok((status, bytes))
}

async fn uds_request_ok(
    socket_path: &str,
    method: Method,
    path: &str,
    body: Option<String>,
    timeout: Duration,
) -> Result<Bytes> {
    let (status, bytes) = uds_request_raw(socket_path, method, path, body, timeout).await?;
    if status.is_success() || status == StatusCode::NOT_MODIFIED {
        Ok(bytes)
    } else {
        Err(http_status_error(status, &bytes))
    }
}

async fn uds_request_json(
    socket_path: &str,
    method: Method,
    path: &str,
    body: Option<String>,
    timeout: Duration,
) -> Result<Value> {
    let bytes = uds_request_ok(socket_path, method, path, body, timeout).await?;
    serde_json::from_slice(&bytes)
        .map_err(|e| ServiceError::Orchestration(format!("invalid JSON from container API: {e}")))
}

fn http_status_error(status: StatusCode, bytes: &[u8]) -> ServiceError {
    let parsed_msg = serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|v| json_str(&v, "message"));
    let detail = parsed_msg.unwrap_or_else(|| {
        let txt = String::from_utf8_lossy(bytes).trim().to_string();
        if txt.is_empty() {
            status.to_string()
        } else {
            txt
        }
    });
    ServiceError::Orchestration(format!("container API HTTP {}: {detail}", status.as_u16()))
}

async fn pump_log_stream(mut body: Incoming, tx: tokio::sync::mpsc::Sender<ContainerLogLine>) {
    let mut pending = Vec::new();
    while let Some(frame_res) = body.frame().await {
        let Ok(frame) = frame_res else {
            break;
        };
        let Ok(chunk) = frame.into_data() else {
            continue;
        };
        pending.extend_from_slice(&chunk);
        if !flush_log_buffer(&mut pending, &tx).await {
            return;
        }
    }
    if !pending.is_empty() {
        for item in decode_docker_log_frames(&pending) {
            if tx.send(item).await.is_err() {
                break;
            }
        }
    }
}

async fn flush_log_buffer(
    pending: &mut Vec<u8>,
    tx: &tokio::sync::mpsc::Sender<ContainerLogLine>,
) -> bool {
    if is_multiplexed_stream(pending) {
        let (lines, consumed) = drain_multiplexed_frames(pending);
        if consumed > 0 {
            pending.drain(..consumed);
        }
        for line in lines {
            if tx.send(line).await.is_err() {
                return false;
            }
        }
        return true;
    }

    while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
        let raw_line: Vec<u8> = pending.drain(..=pos).collect();
        let text = String::from_utf8_lossy(&raw_line)
            .trim_end_matches(['\r', '\n'])
            .to_string();
        if tx
            .send(ContainerLogLine {
                stream: "stdout".to_string(),
                line: text,
            })
            .await
            .is_err()
        {
            return false;
        }
    }
    true
}
