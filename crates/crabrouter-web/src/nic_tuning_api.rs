use axum::Json;
use axum::extract::{Path as AxumPath, State};
use axum::http::HeaderMap;
use crabrouter_model::NicCapabilitiesSnapshot;
use crabrouter_net::probe_nic_capabilities;

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

/// Probes read-only `NIC` driver capabilities (`ethtool` + `sysfs` `RPS`/`XPS`/`RFS` state).
pub async fn interface_capabilities(
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<NicCapabilitiesSnapshot>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let snap = probe_nic_capabilities(&name).await?;
    Ok(Json(snap))
}
