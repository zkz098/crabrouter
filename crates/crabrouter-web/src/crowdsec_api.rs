use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use crabrouter_model::CrowdSecStatus;

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

/// Returns the cached `CrowdSec` agent/bouncer runtime status and active decisions.
pub async fn crowdsec_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CrowdSecStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(state.crowdsec.status().await))
}

/// Refreshes `CrowdSec` decisions from `cscli` and mirrors them to eBPF XDP if enabled.
pub async fn crowdsec_sync_now(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CrowdSecStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = state.engine.active_config();
    let status = state.crowdsec.refresh_decisions(&cfg).await;
    if cfg.crowdsec.enabled
        && cfg.crowdsec.sync_to_xdp
        && cfg.ebpf.enabled
        && cfg.ebpf.xdp_guard_enabled
    {
        let extra: Vec<String> = status
            .decisions
            .iter()
            .filter(|d| d.decision_type == "ban")
            .map(|d| d.value.clone())
            .collect();
        if let Err(err) = state
            .ebpf
            .sync_extra_xdp_blocklist(&cfg.ebpf.xdp_blocklist, &extra)
            .await
        {
            tracing::warn!(error = %err, "failed to mirror CrowdSec decisions to XDP blocklist");
        }
    }
    Ok(Json(status))
}
