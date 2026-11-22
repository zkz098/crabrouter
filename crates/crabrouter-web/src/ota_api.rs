use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

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
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use ts_rs::TS;

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

const MAX_OTA_UPLOAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MIN_OTA_FREE_BYTES: u64 = 128 * 1024 * 1024;
static OTA_UPLOAD_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

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
    let _upload_guard = OTA_UPLOAD_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .try_lock()
        .map_err(|_| {
            ApiError(CoreError::Transaction(
                "another OTA upload is already in progress".to_string(),
            ))
        })?;

    tokio::fs::create_dir_all(&state.ota_dir)
        .await
        .map_err(CoreError::Io)?;
    ensure_ota_disk_space(&state.ota_dir).await?;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(CoreError::Transaction(format!("multipart error: {e}"))))?
    {
        let Some(raw_name) = field.file_name().map(str::to_string) else {
            continue;
        };
        let safe_name = sanitize_ota_filename(&raw_name)?;
        let staged = stream_ota_field_to_disk(&state.ota_dir, &safe_name, field).await?;
        return Ok(Json(staged));
    }

    Err(ApiError(CoreError::Transaction(
        "no OTA firmware file field found in multipart payload".to_string(),
    )))
}

async fn ensure_ota_disk_space(ota_dir: &Path) -> Result<(), ApiError> {
    let mut cmd = Command::new("df");
    cmd.args(["-B1", "--output=avail"])
        .arg(ota_dir)
        .kill_on_drop(true);
    let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(5), cmd.output()).await else {
        return Ok(());
    };
    if !out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if let Some(avail) = text
        .lines()
        .nth(1)
        .and_then(|s| s.trim().parse::<u64>().ok())
        && avail < MIN_OTA_FREE_BYTES
    {
        return Err(ApiError(CoreError::Transaction(format!(
            "insufficient disk space for OTA upload ({avail} bytes available)"
        ))));
    }
    Ok(())
}

async fn stream_ota_field_to_disk(
    ota_dir: &Path,
    safe_name: &str,
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<StagedOtaImage, ApiError> {
    let tmp_path = ota_dir.join(format!(".{safe_name}.uploading"));
    let final_path = ota_dir.join(safe_name);

    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(CoreError::Io)?;
    let mut hasher = Sha256::new();
    let mut total_bytes = 0_u64;

    loop {
        let chunk = match field.chunk().await {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(err) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(ApiError(CoreError::Transaction(format!(
                    "upload chunk error: {err}"
                ))));
            }
        };
        total_bytes = total_bytes.saturating_add(chunk.len() as u64);
        if total_bytes > MAX_OTA_UPLOAD_BYTES {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(ApiError(CoreError::Transaction(format!(
                "OTA image exceeds maximum allowed size ({MAX_OTA_UPLOAD_BYTES} bytes)"
            ))));
        }
        if let Err(err) = file.write_all(&chunk).await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(ApiError(CoreError::Io(err)));
        }
        hasher.update(&chunk);
    }

    if let Err(err) = file.sync_all().await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(ApiError(CoreError::Io(err)));
    }
    tokio::fs::rename(&tmp_path, &final_path)
        .await
        .map_err(CoreError::Io)?;

    Ok(StagedOtaImage {
        filename: safe_name.to_string(),
        size_bytes: total_bytes,
        sha256: hex::encode(hasher.finalize()),
        modified_at: Utc::now().to_rfc3339(),
    })
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
