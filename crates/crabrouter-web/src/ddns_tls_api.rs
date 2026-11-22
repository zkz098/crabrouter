use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use crabrouter_model::{DdnsRuntimeStatus, TlsRuntimeStatus};

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

pub async fn ddns_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DdnsRuntimeStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(state.ddns.snapshot().await))
}

pub async fn ddns_sync_now(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DdnsRuntimeStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = state.engine.active_config();
    let wans = state.wan_bus.snapshot().await;
    Ok(Json(state.ddns.sync_with_wans(&cfg, &wans, true).await))
}

pub async fn tls_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TlsRuntimeStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = state.engine.active_config();
    Ok(Json(state.tls.snapshot(&cfg).await))
}

pub async fn tls_renew_now(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TlsRuntimeStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = state.engine.active_config();
    Ok(Json(state.tls.renew_now(&cfg).await))
}
