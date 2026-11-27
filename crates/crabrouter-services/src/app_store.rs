use crabrouter_model::{
    APP_CONTAINER_PREFIX, AppInstallRequest, ContainerActionResult, ContainerConfig, InstalledApp,
    app_container_name, build_app_container_request, builtin_app_catalog,
};

use crate::containers::{ContainerAction, ContainerBackend};
use crate::{Result, ServiceError};

/// Lists all installed App Store containers by matching `crabrouter-app-*` container names.
pub async fn list_installed_apps(
    backend: &dyn ContainerBackend,
    cfg: &ContainerConfig,
) -> Result<Vec<InstalledApp>> {
    let containers = backend.list_containers(cfg, true).await?;
    let catalog = builtin_app_catalog();
    let mut out = Vec::new();
    for c in containers {
        let container_name = c.name.trim_start_matches('/').to_string();
        if !container_name.starts_with(APP_CONTAINER_PREFIX) {
            continue;
        }
        let app_id = container_name
            .strip_prefix(APP_CONTAINER_PREFIX)
            .unwrap_or(&container_name)
            .to_string();
        let entry = catalog.iter().find(|e| e.id == app_id);
        out.push(InstalledApp {
            name: entry.map_or_else(|| app_id.clone(), |e| e.name.clone()),
            web_port: entry.and_then(|e| e.web_port),
            app_id,
            container_id: c.id,
            container_name,
            image: c.image,
            state: c.state,
            status: c.status,
        });
    }
    Ok(out)
}

/// Pulls the image (if missing), creates the host mount directories, and installs a catalog app.
pub async fn install_catalog_app(
    backend: &dyn ContainerBackend,
    cfg: &ContainerConfig,
    req: &AppInstallRequest,
) -> Result<ContainerActionResult> {
    let catalog = builtin_app_catalog();
    let entry = catalog
        .iter()
        .find(|e| e.id == req.app_id)
        .ok_or_else(|| ServiceError::Render(format!("unknown catalog app '{}'", req.app_id)))?;

    let create_req =
        build_app_container_request(entry, req).map_err(|e| ServiceError::Render(e.to_string()))?;

    for mount in &create_req.mounts {
        if let Err(err) = tokio::fs::create_dir_all(&mount.source).await {
            tracing::debug!(path = %mount.source, error = %err, "app mount directory pre-create skipped");
        }
    }

    backend.pull_image(cfg, &create_req.image).await?;
    backend.create_container(cfg, &create_req).await
}

/// Starts or stops an installed App Store container.
pub async fn toggle_catalog_app(
    backend: &dyn ContainerBackend,
    cfg: &ContainerConfig,
    app_id: &str,
    start: bool,
) -> Result<ContainerActionResult> {
    let name = app_container_name(app_id);
    let action = if start {
        ContainerAction::Start
    } else {
        ContainerAction::Stop
    };
    backend.container_action(cfg, &name, action).await
}

/// Stops and removes an installed App Store container (preserving `/var` data volumes).
pub async fn uninstall_catalog_app(
    backend: &dyn ContainerBackend,
    cfg: &ContainerConfig,
    app_id: &str,
) -> Result<ContainerActionResult> {
    let name = app_container_name(app_id);
    backend
        .container_action(cfg, &name, ContainerAction::Remove { force: true })
        .await
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crabrouter_model::{AppInstallRequest, ContainerConfig};

    use super::{install_catalog_app, list_installed_apps, uninstall_catalog_app};
    use crate::containers::dry_run::DryRunContainerBackend;

    #[tokio::test]
    async fn installs_lists_and_uninstalls_catalog_app_in_dry_run() {
        let backend = DryRunContainerBackend::new();
        let cfg = ContainerConfig {
            enabled: true,
            ..ContainerConfig::default()
        };
        let mut settings = BTreeMap::new();
        settings.insert("web_port".to_string(), "5244".to_string());
        let req = AppInstallRequest {
            app_id: "alist".to_string(),
            settings,
            start_immediately: true,
        };
        let res = install_catalog_app(&backend, &cfg, &req)
            .await
            .expect("install app");
        assert!(res.ok);

        let installed = list_installed_apps(&backend, &cfg)
            .await
            .expect("list apps");
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].app_id, "alist");

        let rm = uninstall_catalog_app(&backend, &cfg, "alist")
            .await
            .expect("uninstall app");
        assert!(rm.ok);
    }
}
