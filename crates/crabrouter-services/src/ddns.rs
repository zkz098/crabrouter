use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use crabrouter_model::{
    DdnsConfig, DdnsProvider, DdnsRuntimeStatus, RouterConfig, WanRuntimeState,
};
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::{Result, ServiceError};

#[derive(Clone)]
pub struct DdnsManager {
    status: Arc<RwLock<DdnsRuntimeStatus>>,
    dry_run: bool,
}

impl DdnsManager {
    pub fn new(cfg: &RouterConfig, dry_run: bool) -> Self {
        Self {
            status: Arc::new(RwLock::new(DdnsRuntimeStatus {
                enabled: cfg.ddns.enabled,
                provider: cfg.ddns.provider,
                domain: cfg.ddns.domain.clone(),
                last_ip: None,
                last_synced_at: None,
                last_error: None,
            })),
            dry_run,
        }
    }

    pub async fn snapshot(&self) -> DdnsRuntimeStatus {
        self.status.read().await.clone()
    }

    /// Synchronizes the dynamic DNS record for the resolved WAN IP (or skips if unchanged unless `force`).
    pub async fn sync_with_wans(
        &self,
        cfg: &RouterConfig,
        wan_states: &[WanRuntimeState],
        force: bool,
    ) -> DdnsRuntimeStatus {
        if !cfg.ddns.enabled {
            let disabled = DdnsRuntimeStatus {
                enabled: false,
                provider: cfg.ddns.provider,
                domain: cfg.ddns.domain.clone(),
                last_ip: None,
                last_synced_at: None,
                last_error: None,
            };
            *self.status.write().await = disabled.clone();
            return disabled;
        }

        let prev = self.status.read().await.clone();
        let current_ip = resolve_ddns_ip(&cfg.ddns, wan_states, self.dry_run).await;
        let Some(ip) = current_ip else {
            let err_status = DdnsRuntimeStatus {
                enabled: true,
                provider: cfg.ddns.provider,
                domain: cfg.ddns.domain.clone(),
                last_ip: prev.last_ip,
                last_synced_at: prev.last_synced_at,
                last_error: Some("no active WAN IPv4 address available for DDNS".to_string()),
            };
            *self.status.write().await = err_status.clone();
            return err_status;
        };

        if !force && prev.last_ip.as_deref() == Some(ip.as_str()) && prev.last_error.is_none() {
            return prev;
        }

        let res = if self.dry_run {
            tracing::info!(
                provider = ?cfg.ddns.provider,
                domain = %cfg.ddns.domain,
                ip = %ip,
                "dry-run: simulated DDNS record update"
            );
            Ok(())
        } else {
            dispatch_ddns_update(&cfg.ddns, &ip).await
        };

        let next = match res {
            Ok(()) => DdnsRuntimeStatus {
                enabled: true,
                provider: cfg.ddns.provider,
                domain: cfg.ddns.domain.clone(),
                last_ip: Some(ip),
                last_synced_at: Some(Utc::now().to_rfc3339()),
                last_error: None,
            },
            Err(err) => DdnsRuntimeStatus {
                enabled: true,
                provider: cfg.ddns.provider,
                domain: cfg.ddns.domain.clone(),
                last_ip: prev.last_ip,
                last_synced_at: prev.last_synced_at,
                last_error: Some(err.to_string()),
            },
        };
        *self.status.write().await = next.clone();
        next
    }
}

/// Spawns the background DDNS sync ticker.
pub fn spawn_ddns_engine<F, W>(
    cfg_fn: F,
    wan_fn: W,
    initial_cfg: &RouterConfig,
    dry_run: bool,
) -> DdnsManager
where
    F: Fn() -> RouterConfig + Send + Sync + 'static,
    W: Fn() -> Vec<WanRuntimeState> + Send + Sync + 'static,
{
    let mgr = DdnsManager::new(initial_cfg, dry_run);
    let bg = mgr.clone();
    tokio::spawn(async move {
        loop {
            let cfg = cfg_fn();
            let interval = u64::from(cfg.ddns.interval_secs.max(30));
            if cfg.ddns.enabled {
                let wans = wan_fn();
                let _ = bg.sync_with_wans(&cfg, &wans, false).await;
            }
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
    mgr
}

async fn resolve_ddns_ip(
    ddns: &DdnsConfig,
    wan_states: &[WanRuntimeState],
    dry_run: bool,
) -> Option<String> {
    let targeted = ddns
        .wan_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let from_state = wan_states
        .iter()
        .find(|w| targeted.is_none_or(|n| n == w.wan_name) && w.ip.is_some())
        .and_then(|w| w.ip.clone());
    if from_state.is_some() {
        return from_state;
    }
    if dry_run {
        return Some("203.0.113.10".to_string());
    }
    let out = Command::new("curl")
        .args(["-fsS", "--max-time", "5", "https://api.ipify.org"])
        .output()
        .await
        .ok()?;
    if out.status.success() {
        let ip = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !ip.is_empty() {
            return Some(ip);
        }
    }
    None
}

/// Formats the `(url, method, body_opt)` request specification for a DDNS provider.
pub fn build_ddns_request(
    ddns: &DdnsConfig,
    ip: &str,
) -> Result<(String, &'static str, Option<String>)> {
    match ddns.provider {
        DdnsProvider::Cloudflare => {
            let zone = ddns.zone_id.as_deref().unwrap_or("").trim();
            let rec = ddns.record_id.as_deref().unwrap_or("").trim();
            if zone.is_empty() || rec.is_empty() {
                return Err(ServiceError::Render(
                    "Cloudflare DDNS requires zone_id and record_id".to_string(),
                ));
            }
            let url =
                format!("https://api.cloudflare.com/client/v4/zones/{zone}/dns_records/{rec}");
            let body = serde_json::json!({
                "type": "A",
                "name": ddns.domain,
                "content": ip,
                "ttl": 120,
                "proxied": false
            })
            .to_string();
            Ok((url, "PUT", Some(body)))
        }
        DdnsProvider::DynDns2 => {
            let base = ddns
                .webhook_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("https://members.dyndns.org/nic/update");
            let url = format!("{base}?hostname={}&myip={ip}", ddns.domain);
            Ok((url, "GET", None))
        }
        DdnsProvider::Webhook => {
            let url = ddns
                .webhook_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ServiceError::Render("Webhook DDNS requires webhook_url".to_string())
                })?
                .to_string();
            let body = serde_json::json!({
                "domain": ddns.domain,
                "ip": ip,
                "timestamp": Utc::now().to_rfc3339()
            })
            .to_string();
            Ok((url, "POST", Some(body)))
        }
    }
}

async fn dispatch_ddns_update(ddns: &DdnsConfig, ip: &str) -> Result<()> {
    let (url, method, body_opt) = build_ddns_request(ddns, ip)?;
    let mut cmd = Command::new("curl");
    cmd.args(["-fsS", "--max-time", "10", "-X", method]);

    if ddns.provider == DdnsProvider::Cloudflare
        && let Some(tok) = ddns.token.as_deref().filter(|s| !s.is_empty())
    {
        cmd.args(["-H", &format!("Authorization: Bearer {tok}")]);
    } else if ddns.provider == DdnsProvider::DynDns2
        && let (Some(u), Some(p)) = (ddns.username.as_deref(), ddns.token.as_deref())
    {
        cmd.args(["-u", &format!("{u}:{p}")]);
    }

    if let Some(body) = body_opt {
        cmd.args(["-H", "Content-Type: application/json", "-d", &body]);
    }
    cmd.arg(&url);

    let out = cmd.output().await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(ServiceError::Orchestration(format!(
            "DDNS update failed: {stderr}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_cloudflare_and_webhook_requests() {
        let cf = DdnsConfig {
            enabled: true,
            provider: DdnsProvider::Cloudflare,
            domain: "router.example.com".to_string(),
            zone_id: Some("z1".to_string()),
            record_id: Some("r1".to_string()),
            token: Some("tok".to_string()),
            ..DdnsConfig::default()
        };
        let (url, method, body) = build_ddns_request(&cf, "198.51.100.42").expect("cf req");
        assert_eq!(method, "PUT");
        assert!(url.contains("/zones/z1/dns_records/r1"));
        assert!(body.unwrap_or_default().contains("198.51.100.42"));
    }

    #[tokio::test]
    async fn dry_run_ddns_updates_last_ip_and_skips_unchanged() {
        let mut cfg = RouterConfig::default();
        cfg.ddns.enabled = true;
        cfg.ddns.domain = "home.example.com".to_string();

        let mgr = DdnsManager::new(&cfg, true);
        let st1 = mgr.sync_with_wans(&cfg, &[], false).await;
        assert_eq!(st1.last_ip.as_deref(), Some("203.0.113.10"));
        assert!(st1.last_error.is_none());
    }
}
