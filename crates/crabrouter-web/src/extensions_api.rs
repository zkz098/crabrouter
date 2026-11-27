use axum::Json;
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::HeaderMap;
use crabrouter_core::CoreError;
use crabrouter_model::{
    AppCatalogEntry, AppInstallRequest, ContainerActionResult, InstalledApp, SysextActionResult,
    SysextRuntimeStatus, builtin_app_catalog, sanitize_sysext_filename,
};
use crabrouter_services::{
    ServiceError, install_catalog_app, list_installed_apps, query_sysext_status, refresh_sysext,
    remove_sysext_image, toggle_catalog_app, uninstall_catalog_app, unmerge_sysext,
};
use tokio::io::AsyncWriteExt;

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

fn map_svc_err(err: ServiceError) -> ApiError {
    match err {
        ServiceError::Render(msg) => ApiError(CoreError::Transaction(msg)),
        other => ApiError(CoreError::Service(other)),
    }
}

/// Returns the curated App Store catalog.
pub async fn apps_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AppCatalogEntry>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(builtin_app_catalog()))
}

/// Lists installed App Store containers.
pub async fn apps_installed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<InstalledApp>>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = &state.engine.active_config().containers;
    let apps = list_installed_apps(state.containers.as_ref(), cfg)
        .await
        .map_err(map_svc_err)?;
    Ok(Json(apps))
}

/// Installs and optionally starts an App Store application.
pub async fn apps_install(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AppInstallRequest>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = &state.engine.active_config().containers;
    let res = install_catalog_app(state.containers.as_ref(), cfg, &req)
        .await
        .map_err(map_svc_err)?;
    Ok(Json(res))
}

/// Starts an installed App Store application.
pub async fn apps_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = &state.engine.active_config().containers;
    let res = toggle_catalog_app(state.containers.as_ref(), cfg, &id, true)
        .await
        .map_err(map_svc_err)?;
    Ok(Json(res))
}

/// Stops an installed App Store application.
pub async fn apps_stop(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = &state.engine.active_config().containers;
    let res = toggle_catalog_app(state.containers.as_ref(), cfg, &id, false)
        .await
        .map_err(map_svc_err)?;
    Ok(Json(res))
}

/// Uninstalls an App Store application container.
pub async fn apps_uninstall(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ContainerActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let cfg = &state.engine.active_config().containers;
    let res = uninstall_catalog_app(state.containers.as_ref(), cfg, &id)
        .await
        .map_err(map_svc_err)?;
    Ok(Json(res))
}

/// Returns current `systemd-sysext` status and `.raw` images in `sysext_dir`.
pub async fn sysext_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SysextRuntimeStatus>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(Json(
        query_sysext_status(&state.sysext_dir, state.dry_run).await,
    ))
}

/// Uploads a `.raw` `systemd-sysext` extension image into `sysext_dir`.
pub async fn sysext_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<SysextActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    tokio::fs::create_dir_all(&state.sysext_dir)
        .await
        .map_err(|e| ApiError(CoreError::Io(e)))?;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(CoreError::Transaction(format!("multipart error: {e}"))))?
    {
        let Some(raw_name) = field.file_name().map(ToString::to_string) else {
            continue;
        };
        let filename = sanitize_sysext_filename(&raw_name)
            .map_err(|e| ApiError(CoreError::Transaction(e.to_string())))?;
        let target = state.sysext_dir.join(&filename);
        let tmp = state.sysext_dir.join(format!("{filename}.tmp"));
        let mut file = tokio::fs::File::create(&tmp)
            .await
            .map_err(|e| ApiError(CoreError::Io(e)))?;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| ApiError(CoreError::Transaction(format!("upload chunk error: {e}"))))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| ApiError(CoreError::Io(e)))?;
        }
        file.flush().await.map_err(|e| ApiError(CoreError::Io(e)))?;
        tokio::fs::rename(&tmp, &target)
            .await
            .map_err(|e| ApiError(CoreError::Io(e)))?;
        return Ok(Json(SysextActionResult {
            ok: true,
            action: "upload".to_string(),
            message: format!("uploaded {filename}"),
        }));
    }

    Err(ApiError(CoreError::Transaction(
        "no .raw extension file found in multipart upload".to_string(),
    )))
}

/// Runs `systemd-sysext refresh` to activate uploaded extensions.
pub async fn sysext_refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SysextActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let res = refresh_sysext(state.dry_run).await.map_err(map_svc_err)?;
    Ok(Json(res))
}

/// Runs `systemd-sysext unmerge` to deactivate all active extensions.
pub async fn sysext_unmerge(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SysextActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let res = unmerge_sysext(state.dry_run).await.map_err(map_svc_err)?;
    Ok(Json(res))
}

/// Deletes a `.raw` extension image from `sysext_dir`.
pub async fn sysext_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<SysextActionResult>, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    let res = remove_sysext_image(&state.sysext_dir, &name, state.dry_run)
        .await
        .map_err(map_svc_err)?;
    Ok(Json(res))
}
