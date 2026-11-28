use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use crabrouter_model::{ContainerRuntimeKind, RouterConfig};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zbus::OwnedObjectPath;

use crate::chrony::render_chrony_conf;
use crate::containers::config::{
    render_containers_conf, render_registries_conf, sync_container_host_configs,
};
use crate::crabdns::{render_crabdns_toml, render_crabdns_toml_with_isp_dns};
use crate::crowdsec::{render_crowdsec_acquis_yaml, render_crowdsec_bouncer_conf};
use crate::dnsmasq::render_dnsmasq_conf;
use crate::iptv::{render_igmpproxy_conf, render_udpxy_env};
use crate::ipv6_relay::{has_ipv6_relay_enabled, render_ndppd_conf};
use crate::mdns::render_avahi_daemon_conf;
use crate::upnp::render_miniupnpd_conf;
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
    fn sync_services_with_isp_dns<'a>(
        &'a self,
        cfg: &'a RouterConfig,
        _isp_dns: &'a [String],
    ) -> BoxFuture<'a, Result<()>> {
        self.sync_services(cfg)
    }
    fn sync_crabdns_with_isp_dns<'a>(
        &'a self,
        _cfg: &'a RouterConfig,
        _isp_dns: &'a [String],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }
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
            let chrony = render_chrony_conf(cfg);
            let miniupnpd = render_miniupnpd_conf(cfg);
            let ndppd = render_ndppd_conf(cfg);
            let avahi = render_avahi_daemon_conf(cfg);
            let igmpproxy = render_igmpproxy_conf(cfg);
            let udpxy = render_udpxy_env(cfg);
            let crowdsec_bouncer = render_crowdsec_bouncer_conf(cfg);
            let crowdsec_acquis = render_crowdsec_acquis_yaml(cfg);
            let wg = if cfg.wireguard.enabled {
                render_wg_quick_conf(
                    cfg,
                    cfg.wireguard.private_key.as_deref().unwrap_or("<auto>"),
                )?
            } else {
                String::new()
            };
            let containers_conf = render_containers_conf(cfg);
            let registries_conf = render_registries_conf(cfg);
            tracing::info!(
                dnsmasq_bytes = dnsmasq.len(),
                crabdns_bytes = crabdns.len(),
                chrony_bytes = chrony.len(),
                miniupnpd_bytes = miniupnpd.len(),
                ndppd_bytes = ndppd.len(),
                avahi_bytes = avahi.len(),
                igmpproxy_bytes = igmpproxy.len(),
                udpxy_bytes = udpxy.len(),
                crowdsec_bouncer_bytes = crowdsec_bouncer.len(),
                crowdsec_acquis_bytes = crowdsec_acquis.len(),
                wg_bytes = wg.len(),
                containers_conf_bytes = containers_conf.len(),
                registries_conf_bytes = registries_conf.len(),
                "dry-run: rendered crabdns, dnsmasq, miniupnpd, ndppd, wireguard & container configurations"
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
                    unit: "crabrouter-dhcpcd.service".to_string(),
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
                dry_run_toggle_unit("miniupnpd.service", cfg.upnp.enabled),
                dry_run_toggle_unit("ndppd.service", has_ipv6_relay_enabled(cfg)),
                dry_run_toggle_unit("avahi-daemon.service", cfg.mdns.enabled),
                dry_run_toggle_unit("igmpproxy.service", cfg.iptv.enabled),
                dry_run_toggle_unit("udpxy.service", cfg.iptv.enabled && cfg.iptv.udpxy_enabled),
                dry_run_toggle_unit("crowdsec.service", cfg.crowdsec.enabled),
                dry_run_toggle_unit(
                    "crowdsec-firewall-bouncer.service",
                    cfg.crowdsec.enabled && cfg.crowdsec.bouncer_enabled,
                ),
                dry_run_toggle_unit(container_runtime_unit(cfg), cfg.containers.enabled),
            ])
        })
    }
}

#[derive(Debug, Clone)]
pub struct SystemdSupervisor {
    pub runtime_dir: PathBuf,
    pub wireguard_dir: PathBuf,
    pub containers_dir: PathBuf,
    pub systemd_dir: PathBuf,
}

impl Default for SystemdSupervisor {
    fn default() -> Self {
        Self {
            runtime_dir: PathBuf::from("/run/crabrouter"),
            wireguard_dir: PathBuf::from("/etc/wireguard"),
            containers_dir: PathBuf::from("/etc/containers"),
            systemd_dir: PathBuf::from("/etc/systemd/system"),
        }
    }
}

impl ServiceSupervisor for SystemdSupervisor {
    fn sync_services<'a>(&'a self, cfg: &'a RouterConfig) -> BoxFuture<'a, Result<()>> {
        self.sync_services_with_isp_dns(cfg, &[])
    }

    fn sync_services_with_isp_dns<'a>(
        &'a self,
        cfg: &'a RouterConfig,
        isp_dns: &'a [String],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            tokio::fs::create_dir_all(&self.runtime_dir).await?;
            let dnsmasq_path = self.runtime_dir.join("dnsmasq.conf");
            let crabdns_path = self.runtime_dir.join("crabdns.toml");
            let chrony_path = self.runtime_dir.join("chrony.conf");
            let miniupnpd_path = self.runtime_dir.join("miniupnpd.conf");
            let ndppd_path = self.runtime_dir.join("ndppd.conf");
            let avahi_path = self.runtime_dir.join("avahi-daemon.conf");
            let igmpproxy_path = self.runtime_dir.join("igmpproxy.conf");
            let udpxy_path = self.runtime_dir.join("udpxy.env");
            let crowdsec_bouncer_path = self.runtime_dir.join("crowdsec-firewall-bouncer.yaml");
            let crowdsec_acquis_path = self.runtime_dir.join("crowdsec-acquis.yaml");

            write_atomic(&dnsmasq_path, &render_dnsmasq_conf(cfg)?).await?;
            write_atomic(
                &crabdns_path,
                &render_crabdns_toml_with_isp_dns(cfg, isp_dns)?,
            )
            .await?;
            write_atomic(&chrony_path, &render_chrony_conf(cfg)).await?;
            write_atomic(&miniupnpd_path, &render_miniupnpd_conf(cfg)).await?;
            write_atomic(&ndppd_path, &render_ndppd_conf(cfg)).await?;
            write_atomic(&avahi_path, &render_avahi_daemon_conf(cfg)).await?;
            write_atomic(&igmpproxy_path, &render_igmpproxy_conf(cfg)).await?;
            write_atomic(&udpxy_path, &render_udpxy_env(cfg)).await?;
            write_atomic(&crowdsec_bouncer_path, &render_crowdsec_bouncer_conf(cfg)).await?;
            write_atomic(&crowdsec_acquis_path, &render_crowdsec_acquis_yaml(cfg)).await?;

            if cfg.wireguard.enabled {
                sync_wireguard_files(cfg, &self.wireguard_dir)?;
            }
            if cfg.containers.enabled {
                sync_container_host_configs(cfg, &self.containers_dir, &self.systemd_dir).await?;
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
            reload_unit_via_dbus(&mgr, "crabrouter-dhcpcd.service", false).await?;

            let wg_unit = format!("wg-quick@{}.service", cfg.wireguard.interface);
            sync_optional_unit(&mgr, &wg_unit, cfg.wireguard.enabled, true).await?;
            sync_optional_unit(&mgr, "miniupnpd.service", cfg.upnp.enabled, false).await?;
            sync_optional_unit(&mgr, "ndppd.service", has_ipv6_relay_enabled(cfg), false).await?;
            sync_optional_unit(&mgr, "avahi-daemon.service", cfg.mdns.enabled, false).await?;
            sync_optional_unit(&mgr, "igmpproxy.service", cfg.iptv.enabled, false).await?;
            sync_optional_unit(
                &mgr,
                "udpxy.service",
                cfg.iptv.enabled && cfg.iptv.udpxy_enabled,
                false,
            )
            .await?;
            sync_optional_unit(&mgr, "crowdsec.service", cfg.crowdsec.enabled, false).await?;
            sync_optional_unit(
                &mgr,
                "crowdsec-firewall-bouncer.service",
                cfg.crowdsec.enabled && cfg.crowdsec.bouncer_enabled,
                false,
            )
            .await?;
            sync_optional_unit(
                &mgr,
                container_runtime_unit(cfg),
                cfg.containers.enabled,
                false,
            )
            .await?;

            Ok(())
        })
    }

    fn sync_crabdns_with_isp_dns<'a>(
        &'a self,
        cfg: &'a RouterConfig,
        isp_dns: &'a [String],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            tokio::fs::create_dir_all(&self.runtime_dir).await?;
            let crabdns_path = self.runtime_dir.join("crabdns.toml");
            write_atomic(
                &crabdns_path,
                &render_crabdns_toml_with_isp_dns(cfg, isp_dns)?,
            )
            .await?;
            let conn = zbus::Connection::system().await.map_err(|e| {
                ServiceError::Orchestration(format!("systemd D-Bus connect failed: {e}"))
            })?;
            let mgr = SystemdManagerProxy::new(&conn).await.map_err(|e| {
                ServiceError::Orchestration(format!("systemd Manager proxy failed: {e}"))
            })?;
            reload_unit_via_dbus(&mgr, "crabdns.service", true).await
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
                "crabrouter-dhcpcd.service".to_string(),
                format!("wg-quick@{}.service", cfg.wireguard.interface),
                "miniupnpd.service".to_string(),
                "ndppd.service".to_string(),
                "avahi-daemon.service".to_string(),
                "igmpproxy.service".to_string(),
                "udpxy.service".to_string(),
                "crowdsec.service".to_string(),
                "crowdsec-firewall-bouncer.service".to_string(),
                container_runtime_unit(cfg).to_string(),
            ];
            let mut out = Vec::with_capacity(units.len());
            for unit in units {
                out.push(fetch_unit_status(&conn, &mgr, &unit).await);
            }
            Ok(out)
        })
    }
}

fn container_runtime_unit(cfg: &RouterConfig) -> &'static str {
    match cfg.containers.runtime {
        ContainerRuntimeKind::Podman => "podman.socket",
        ContainerRuntimeKind::Docker => "docker.service",
    }
}

fn dry_run_toggle_unit(unit: &str, enabled: bool) -> SystemdUnitStatus {
    SystemdUnitStatus {
        unit: unit.to_string(),
        load_state: "dry-run".to_string(),
        active_state: if enabled { "active" } else { "inactive" }.to_string(),
        sub_state: if enabled { "running" } else { "dead" }.to_string(),
    }
}

async fn sync_optional_unit(
    mgr: &SystemdManagerProxy<'_>,
    unit: &str,
    enabled: bool,
    required: bool,
) -> Result<()> {
    if enabled {
        reload_unit_via_dbus(mgr, unit, required).await
    } else {
        stop_unit_via_dbus(mgr, unit).await
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
