use std::fs;
use std::io::Write;

use axum::Json;
use axum::extract::{Multipart, State};
use axum::http::HeaderMap;
use chrono::Utc;
use crabrouter_core::CoreError;
use crabrouter_services::{
    OtaCommandResult, OtaStatus, StagedOtaImage, apply_ota_update, bless_current_boot,
    check_ota_updates, query_ota_status, sanitize_ota_filename, trigger_reboot_via_dbus,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct OtaApplyRequest {
    pub source_url: Option<String>,
    pub staged_file: Option<String>,
    #[serde(default)]
    pub verify_signature: bool,
}

pub async fn ota_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OtaStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let source = state.engine.active_config().system.ota_update_url.clone();
    Ok(Json(
        query_ota_status(&state.ota_dir, &source, state.dry_run).await,
    ))
}

pub async fn ota_check(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OtaCommandResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let source = state.engine.active_config().system.ota_update_url.clone();
    let res = check_ota_updates(&state.ota_dir, &source, state.dry_run).await?;
    Ok(Json(res))
}

pub async fn ota_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<StagedOtaImage>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    fs::create_dir_all(&state.ota_dir).map_err(CoreError::Io)?;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(CoreError::Transaction(format!("multipart error: {e}"))))?
    {
        let Some(raw_name) = field.file_name().map(str::to_string) else {
            continue;
        };
        let safe_name = sanitize_ota_filename(&raw_name)?;
        let tmp_path = state.ota_dir.join(format!(".{safe_name}.uploading"));
        let final_path = state.ota_dir.join(&safe_name);

        let mut file = fs::File::create(&tmp_path).map_err(CoreError::Io)?;
        let mut hasher = Sha256::new();
        let mut total_bytes = 0_u64;

        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| ApiError(CoreError::Transaction(format!("upload chunk error: {e}"))))?
        {
            file.write_all(&chunk).map_err(CoreError::Io)?;
            hasher.update(&chunk);
            total_bytes = total_bytes.saturating_add(chunk.len() as u64);
        }
        file.sync_all().map_err(CoreError::Io)?;
        fs::rename(&tmp_path, &final_path).map_err(CoreError::Io)?;

        return Ok(Json(StagedOtaImage {
            filename: safe_name,
            size_bytes: total_bytes,
            sha256: hex::encode(hasher.finalize()),
            modified_at: Utc::now().to_rfc3339(),
        }));
    }

    Err(ApiError(CoreError::Transaction(
        "no OTA firmware file field found in multipart payload".to_string(),
    )))
}

pub async fn ota_apply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<OtaApplyRequest>,
) -> Result<Json<OtaCommandResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let default_source = state.engine.active_config().system.ota_update_url.clone();
    let source = req
        .source_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default_source.as_str());

    let res = apply_ota_update(
        &state.ota_dir,
        source,
        req.staged_file.as_deref(),
        req.verify_signature,
        &state.sysupdate_dir,
        state.dry_run,
    )
    .await?;
    Ok(Json(res))
}

pub async fn ota_bless(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OtaCommandResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(bless_current_boot(state.dry_run).await?))
}

pub async fn ota_reboot(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OtaCommandResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(trigger_reboot_via_dbus(state.dry_run).await?))
}
