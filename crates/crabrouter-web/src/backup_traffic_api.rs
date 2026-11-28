use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use crabrouter_core::{CommitStatus, DailyTrafficRecord};
use serde::Deserialize;
use ts_rs::TS;

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ConfigImportRequest {
    pub config_toml: String,
}

pub async fn export_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let body = state.engine.export_config_toml()?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/toml; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"crabrouter-config.toml\"",
            ),
        ],
        body,
    )
        .into_response())
}

pub async fn import_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ConfigImportRequest>,
) -> Result<Json<CommitStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let status = state.engine.import_config_toml(&req.config_toml).await?;
    let cfg = state.engine.active_config();
    if let Err(err) = state.ebpf.sync_config(&cfg).await {
        tracing::warn!(error = %err, "eBPF sync after config import failed");
    }
    Ok(Json(status))
}

pub async fn daily_traffic(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DailyTrafficRecord>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let records = state.engine.db().list_daily_traffic(90)?;
    Ok(Json(records))
}
