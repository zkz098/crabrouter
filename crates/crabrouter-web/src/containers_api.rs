use axum::Json;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path as AxumPath, Query, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use crabrouter_core::CoreError;
use crabrouter_model::{
    ContainerActionResult, ContainerConfig, ContainerCreateRequest, ContainerImagePullRequest,
    ContainerImageSummary, ContainerLogLine, ContainerNetworkCreateRequest,
    ContainerNetworkSummary, ContainerRuntimeInfo, ContainerSummary, ContainerVolumeSummary,
};
use crabrouter_services::{ContainerAction, ServiceError};
use serde::Deserialize;

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

#[derive(Debug, Deserialize)]
pub struct ListContainersQuery {
    #[serde(default = "default_true")]
    pub all: bool,
}

#[derive(Debug, Deserialize)]
pub struct ForceQuery {
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    #[serde(default = "default_tail")]
    pub tail: u32,
}

const fn default_true() -> bool {
    true
}

const fn default_tail() -> u32 {
    200
}

fn container_cfg(state: &AppState) -> ContainerConfig {
    state.engine.active_config().containers.clone()
}

fn map_container_err(err: ServiceError) -> ApiError {
    match err {
        ServiceError::Render(msg) => ApiError(CoreError::Transaction(msg)),
        other => ApiError(CoreError::Service(other)),
    }
}

pub async fn containers_info(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ContainerRuntimeInfo>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let info = state
        .containers
        .runtime_info(&cfg)
        .await
        .map_err(map_container_err)?;
    Ok(Json(info))
}

pub async fn list_containers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListContainersQuery>,
) -> Result<Json<Vec<ContainerSummary>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let list = state
        .containers
        .list_containers(&cfg, query.all)
        .await
        .map_err(map_container_err)?;
    Ok(Json(list))
}

pub async fn create_container(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ContainerCreateRequest>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let res = state
        .containers
        .create_container(&cfg, &req)
        .await
        .map_err(map_container_err)?;
    Ok(Json(res))
}

pub async fn inspect_container(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let detail = state
        .containers
        .inspect_container(&cfg, &id)
        .await
        .map_err(map_container_err)?;
    Ok(Json(detail))
}

async fn dispatch_container_action(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    action: ContainerAction,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(state, headers)?;
    let cfg = container_cfg(state);
    let res = state
        .containers
        .container_action(&cfg, id, action)
        .await
        .map_err(map_container_err)?;
    Ok(Json(res))
}

pub async fn start_container(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    dispatch_container_action(&state, &headers, &id, ContainerAction::Start).await
}

pub async fn stop_container(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    dispatch_container_action(&state, &headers, &id, ContainerAction::Stop).await
}

pub async fn restart_container(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    dispatch_container_action(&state, &headers, &id, ContainerAction::Restart).await
}

pub async fn remove_container(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<ForceQuery>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    dispatch_container_action(
        &state,
        &headers,
        &id,
        ContainerAction::Remove { force: query.force },
    )
    .await
}

pub async fn container_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<LogsQuery>,
) -> Result<Json<Vec<ContainerLogLine>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let lines = state
        .containers
        .container_logs(&cfg, &id, query.tail)
        .await
        .map_err(map_container_err)?;
    Ok(Json(lines))
}

pub async fn container_logs_ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<LogsQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let rx = state
        .containers
        .stream_container_logs(&cfg, &id, query.tail)
        .await
        .map_err(map_container_err)?;
    Ok(ws.on_upgrade(move |socket| handle_container_logs_ws(socket, rx)))
}

async fn handle_container_logs_ws(
    mut socket: WebSocket,
    mut rx: tokio::sync::mpsc::Receiver<ContainerLogLine>,
) {
    loop {
        tokio::select! {
            item = rx.recv() => {
                if !forward_container_log(&mut socket, item).await {
                    break;
                }
            }
            msg = socket.recv() => {
                if msg.is_none() || matches!(msg, Some(Ok(Message::Close(_)) | Err(_))) {
                    break;
                }
            }
        }
    }
}

async fn forward_container_log(socket: &mut WebSocket, item: Option<ContainerLogLine>) -> bool {
    let Some(entry) = item else {
        return false;
    };
    let Ok(json) = serde_json::to_string(&entry) else {
        return true;
    };
    socket.send(Message::Text(json.into())).await.is_ok()
}

pub async fn list_images(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ContainerImageSummary>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let list = state
        .containers
        .list_images(&cfg)
        .await
        .map_err(map_container_err)?;
    Ok(Json(list))
}

pub async fn pull_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ContainerImagePullRequest>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let res = state
        .containers
        .pull_image(&cfg, &req.reference)
        .await
        .map_err(map_container_err)?;
    Ok(Json(res))
}

pub async fn remove_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<ForceQuery>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let res = state
        .containers
        .remove_image(&cfg, &id, query.force)
        .await
        .map_err(map_container_err)?;
    Ok(Json(res))
}

pub async fn list_networks(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ContainerNetworkSummary>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let list = state
        .containers
        .list_networks(&cfg)
        .await
        .map_err(map_container_err)?;
    Ok(Json(list))
}

pub async fn create_network(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ContainerNetworkCreateRequest>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let res = state
        .containers
        .create_network(&cfg, &req)
        .await
        .map_err(map_container_err)?;
    Ok(Json(res))
}

pub async fn remove_network(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let res = state
        .containers
        .remove_network(&cfg, &id)
        .await
        .map_err(map_container_err)?;
    Ok(Json(res))
}

pub async fn list_volumes(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ContainerVolumeSummary>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let list = state
        .containers
        .list_volumes(&cfg)
        .await
        .map_err(map_container_err)?;
    Ok(Json(list))
}

pub async fn remove_volume(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(name): AxumPath<String>,
    Query(query): Query<ForceQuery>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = container_cfg(&state);
    let res = state
        .containers
        .remove_volume(&cfg, &name, query.force)
        .await
        .map_err(map_container_err)?;
    Ok(Json(res))
}
