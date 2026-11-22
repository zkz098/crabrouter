use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use crabrouter_model::RouterConfig;
use tokio::process::Command;

use crate::crabdns::render_crabdns_toml;
use crate::dnsmasq::render_dnsmasq_conf;
use crate::{Result, ServiceError};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ServiceSupervisor: Send + Sync {
    fn sync_services<'a>(&'a self, cfg: &'a RouterConfig) -> BoxFuture<'a, Result<()>>;
}

#[derive(Debug, Default)]
pub struct DryRunSupervisor;

impl ServiceSupervisor for DryRunSupervisor {
    fn sync_services<'a>(&'a self, cfg: &'a RouterConfig) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let dnsmasq = render_dnsmasq_conf(cfg)?;
            let crabdns = render_crabdns_toml(cfg)?;
            tracing::info!(
                dnsmasq_bytes = dnsmasq.len(),
                crabdns_bytes = crabdns.len(),
                "dry-run: rendered crabdns & dnsmasq configurations"
            );
            Ok(())
        })
    }
}

#[derive(Debug, Clone)]
pub struct SystemdSupervisor {
    pub runtime_dir: PathBuf,
}

impl Default for SystemdSupervisor {
    fn default() -> Self {
        Self {
            runtime_dir: PathBuf::from("/run/crabrouter"),
        }
    }
}

impl ServiceSupervisor for SystemdSupervisor {
    fn sync_services<'a>(&'a self, cfg: &'a RouterConfig) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            tokio::fs::create_dir_all(&self.runtime_dir).await?;
            let dnsmasq_path = self.runtime_dir.join("dnsmasq.conf");
            let crabdns_path = self.runtime_dir.join("crabdns.toml");

            tokio::fs::write(&dnsmasq_path, render_dnsmasq_conf(cfg)?).await?;
            tokio::fs::write(&crabdns_path, render_crabdns_toml(cfg)?).await?;

            // TODO(Phase 1): Migrate unit reload/restart calls to `zbus` over systemd D-Bus.
            reload_unit_if_present("crabdns.service").await?;
            reload_unit_if_present("dnsmasq.service").await?;
            Ok(())
        })
    }
}

async fn reload_unit_if_present(unit: &str) -> Result<()> {
    let status = Command::new("systemctl")
        .args(["try-reload-or-restart", unit])
        .status()
        .await?;
    if !status.success() {
        return Err(ServiceError::Orchestration(format!(
            "systemctl try-reload-or-restart {unit} exited with {status}"
        )));
    }
    Ok(())
}
