use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use crabrouter_model::RouterConfig;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zbus::OwnedObjectPath;

use crate::crabdns::render_crabdns_toml;
use crate::dnsmasq::render_dnsmasq_conf;
use crate::wireguard::{render_wg_quick_conf, sync_wireguard_files};
use crate::{Result, ServiceError};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SystemdUnitStatus {
    pub unit: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManager {
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn reload_or_restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
trait SystemdUnit {
    #[zbus(property)]
    fn load_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;
}

pub trait ServiceSupervisor: Send + Sync {
    fn sync_services<'a>(&'a self, cfg: &'a RouterConfig) -> BoxFuture<'a, Result<()>>;
    fn query_unit_statuses<'a>(
        &'a self,
        cfg: &'a RouterConfig,
    ) -> BoxFuture<'a, Result<Vec<SystemdUnitStatus>>>;
}

#[derive(Debug, Default)]
pub struct DryRunSupervisor;

impl ServiceSupervisor for DryRunSupervisor {
    fn sync_services<'a>(&'a self, cfg: &'a RouterConfig) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let dnsmasq = render_dnsmasq_conf(cfg)?;
            let crabdns = render_crabdns_toml(cfg)?;
            let wg = if cfg.wireguard.enabled {
                render_wg_quick_conf(
                    cfg,
                    cfg.wireguard.private_key.as_deref().unwrap_or("<auto>"),
                )?
            } else {
                String::new()
            };
            tracing::info!(
                dnsmasq_bytes = dnsmasq.len(),
                crabdns_bytes = crabdns.len(),
                wg_bytes = wg.len(),
                "dry-run: rendered crabdns, dnsmasq & wireguard configurations"
            );
            Ok(())
        })
    }

    fn query_unit_statuses<'a>(
        &'a self,
        cfg: &'a RouterConfig,
    ) -> BoxFuture<'a, Result<Vec<SystemdUnitStatus>>> {
        Box::pin(async move {
            let wg_unit = format!("wg-quick@{}.service", cfg.wireguard.interface);
            Ok(vec![
                SystemdUnitStatus {
                    unit: "crabdns.service".to_string(),
                    load_state: "dry-run".to_string(),
                    active_state: "active".to_string(),
                    sub_state: "running".to_string(),
                },
                SystemdUnitStatus {
                    unit: "dnsmasq.service".to_string(),
                    load_state: "dry-run".to_string(),
                    active_state: "active".to_string(),
                    sub_state: "running".to_string(),
                },
                SystemdUnitStatus {
                    unit: "chrony.service".to_string(),
                    load_state: "dry-run".to_string(),
                    active_state: "active".to_string(),
                    sub_state: "running".to_string(),
                },
                SystemdUnitStatus {
                    unit: wg_unit,
                    load_state: "dry-run".to_string(),
                    active_state: if cfg.wireguard.enabled {
                        "active"
                    } else {
                        "inactive"
                    }
                    .to_string(),
                    sub_state: if cfg.wireguard.enabled {
                        "exited"
                    } else {
                        "dead"
                    }
                    .to_string(),
                },
            ])
        })
    }
}

#[derive(Debug, Clone)]
pub struct SystemdSupervisor {
    pub runtime_dir: PathBuf,
    pub wireguard_dir: PathBuf,
}

impl Default for SystemdSupervisor {
    fn default() -> Self {
        Self {
            runtime_dir: PathBuf::from("/run/crabrouter"),
            wireguard_dir: PathBuf::from("/etc/wireguard"),
        }
    }
}

impl ServiceSupervisor for SystemdSupervisor {
    fn sync_services<'a>(&'a self, cfg: &'a RouterConfig) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            tokio::fs::create_dir_all(&self.runtime_dir).await?;
            let dnsmasq_path = self.runtime_dir.join("dnsmasq.conf");
            let crabdns_path = self.runtime_dir.join("crabdns.toml");

            write_atomic(&dnsmasq_path, &render_dnsmasq_conf(cfg)?).await?;
            write_atomic(&crabdns_path, &render_crabdns_toml(cfg)?).await?;

            if cfg.wireguard.enabled {
                sync_wireguard_files(cfg, &self.wireguard_dir)?;
            }

            let conn = zbus::Connection::system().await.map_err(|e| {
                ServiceError::Orchestration(format!("systemd D-Bus connect failed: {e}"))
            })?;
            let mgr = SystemdManagerProxy::new(&conn).await.map_err(|e| {
                ServiceError::Orchestration(format!("systemd Manager proxy failed: {e}"))
            })?;

            reload_unit_via_dbus(&mgr, "dnsmasq.service", true).await?;
            reload_unit_via_dbus(&mgr, "crabdns.service", true).await?;
            reload_unit_via_dbus(&mgr, "chrony.service", false).await?;

            let wg_unit = format!("wg-quick@{}.service", cfg.wireguard.interface);
            if cfg.wireguard.enabled {
                reload_unit_via_dbus(&mgr, &wg_unit, true).await?;
            } else {
                stop_unit_via_dbus(&mgr, &wg_unit).await?;
            }

            Ok(())
        })
    }

    fn query_unit_statuses<'a>(
        &'a self,
        cfg: &'a RouterConfig,
    ) -> BoxFuture<'a, Result<Vec<SystemdUnitStatus>>> {
        Box::pin(async move {
            let conn = match zbus::Connection::system().await {
                Ok(c) => c,
                Err(err) => {
                    tracing::debug!(error = %err, "system D-Bus unavailable while querying units");
                    return Ok(Vec::new());
                }
            };
            let mgr = SystemdManagerProxy::new(&conn).await.map_err(|e| {
                ServiceError::Orchestration(format!("systemd Manager proxy failed: {e}"))
            })?;

            let units = vec![
                "crabdns.service".to_string(),
                "dnsmasq.service".to_string(),
                "chrony.service".to_string(),
                format!("wg-quick@{}.service", cfg.wireguard.interface),
            ];
            let mut out = Vec::with_capacity(units.len());
            for unit in units {
                out.push(fetch_unit_status(&conn, &mgr, &unit).await);
            }
            Ok(out)
        })
    }
}

async fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, content).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

async fn reload_unit_via_dbus(
    mgr: &SystemdManagerProxy<'_>,
    unit: &str,
    required: bool,
) -> Result<()> {
    match mgr.reload_or_restart_unit(unit, "replace").await {
        Ok(job) => {
            tracing::info!(unit, job = %job.as_str(), "systemd D-Bus reload_or_restart dispatched");
            Ok(())
        }
        Err(err) => {
            let msg = err.to_string();
            if !required || msg.contains("NoSuchUnit") || msg.contains("UnitNotFound") {
                tracing::warn!(unit, error = %msg, "systemd unit skipped because it is not installed");
                Ok(())
            } else {
                Err(ServiceError::Orchestration(format!(
                    "systemd D-Bus reload_or_restart {unit} failed: {msg}"
                )))
            }
        }
    }
}

async fn stop_unit_via_dbus(mgr: &SystemdManagerProxy<'_>, unit: &str) -> Result<()> {
    match mgr.stop_unit(unit, "replace").await {
        Ok(job) => {
            tracing::info!(unit, job = %job.as_str(), "systemd D-Bus stop_unit dispatched");
            Ok(())
        }
        Err(err) => {
            let msg = err.to_string();
            if msg.contains("NoSuchUnit")
                || msg.contains("UnitNotFound")
                || msg.contains("not loaded")
            {
                Ok(())
            } else {
                Err(ServiceError::Orchestration(format!(
                    "systemd D-Bus stop_unit {unit} failed: {msg}"
                )))
            }
        }
    }
}

async fn fetch_unit_status(
    conn: &zbus::Connection,
    mgr: &SystemdManagerProxy<'_>,
    unit: &str,
) -> SystemdUnitStatus {
    let Ok(path) = mgr.get_unit(unit).await else {
        return SystemdUnitStatus {
            unit: unit.to_string(),
            load_state: "not-found".to_string(),
            active_state: "inactive".to_string(),
            sub_state: "dead".to_string(),
        };
    };
    let Ok(unit_proxy) = SystemdUnitProxy::builder(conn).path(path).build().await else {
        return SystemdUnitStatus {
            unit: unit.to_string(),
            load_state: "unknown".to_string(),
            active_state: "unknown".to_string(),
            sub_state: "unknown".to_string(),
        };
    };
    let load_state = unit_proxy
        .load_state()
        .await
        .unwrap_or_else(|_| "unknown".to_string());
    let active_state = unit_proxy
        .active_state()
        .await
        .unwrap_or_else(|_| "unknown".to_string());
    let sub_state = unit_proxy
        .sub_state()
        .await
        .unwrap_or_else(|_| "unknown".to_string());

    SystemdUnitStatus {
        unit: unit.to_string(),
        load_state,
        active_state,
        sub_state,
    }
}
