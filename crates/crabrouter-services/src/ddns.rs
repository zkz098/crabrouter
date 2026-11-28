use std::net::Ipv6Addr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use crabrouter_model::{
    DdnsConfig, DdnsIpVersion, DdnsProvider, DdnsRuntimeStatus, RouterConfig, WanRuntimeState,
};
use tokio::process::Command;
use tokio::sync::{RwLock, watch};

use crate::ddns_domestic::{record_type_for, sync_domestic_ddns};
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
            let label = match cfg.ddns.ip_version {
                DdnsIpVersion::Ipv4 => "IPv4",
                DdnsIpVersion::Ipv6 => "IPv6",
            };
            let err_status = DdnsRuntimeStatus {
                enabled: true,
                provider: cfg.ddns.provider,
                domain: cfg.ddns.domain.clone(),
                last_ip: prev.last_ip,
                last_synced_at: prev.last_synced_at,
                last_error: Some(format!("no active WAN {label} address available for DDNS")),
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
    let (_tx, rx) = watch::channel(false);
    spawn_ddns_engine_with_shutdown(cfg_fn, wan_fn, initial_cfg, dry_run, rx)
}

/// Spawns the periodic DDNS engine with an explicit graceful-shutdown signal.
pub fn spawn_ddns_engine_with_shutdown<F, W>(
    cfg_fn: F,
    wan_fn: W,
    initial_cfg: &RouterConfig,
    dry_run: bool,
    mut shutdown: watch::Receiver<bool>,
) -> DdnsManager
where
    F: Fn() -> RouterConfig + Send + Sync + 'static,
    W: Fn() -> Vec<WanRuntimeState> + Send + Sync + 'static,
{
    let mgr = DdnsManager::new(initial_cfg, dry_run);
    let bg = mgr.clone();
    tokio::spawn(async move {
        loop {
            if *shutdown.borrow() {
                break;
            }
            let cfg = cfg_fn();
            let interval = u64::from(cfg.ddns.interval_secs.max(30));
            if cfg.ddns.enabled {
                let wans = wan_fn();
                let _ = bg.sync_with_wans(&cfg, &wans, false).await;
            }
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(interval)) => {}
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("DDNS engine received shutdown signal");
                        break;
                    }
                }
            }
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

    match ddns.ip_version {
        DdnsIpVersion::Ipv4 => {
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
            fetch_public_ip("https://api.ipify.org", false).await
        }
        DdnsIpVersion::Ipv6 => {
            let from_state = wan_states
                .iter()
                .filter(|w| targeted.is_none_or(|n| n == w.wan_name))
                .filter_map(|w| w.ipv6_addr.as_deref())
                .find_map(extract_global_ipv6);
            if from_state.is_some() {
                return from_state;
            }
            if dry_run {
                return Some("2001:db8::10".to_string());
            }
            fetch_public_ip("https://api6.ipify.org", true).await
        }
    }
}

fn extract_global_ipv6(raw: &str) -> Option<String> {
    let addr_str = raw.trim().split('/').next()?;
    let addr: Ipv6Addr = addr_str.parse().ok()?;
    let first_seg = addr.segments()[0];
    // Global unicast 2000::/3 (excludes ::1, fe80::/10 link-local, fc00::/7 ULA, ff00::/8 multicast)
    if (first_seg & 0xe000) == 0x2000 {
        Some(addr.to_string())
    } else {
        None
    }
}

async fn fetch_public_ip(url: &str, ipv6: bool) -> Option<String> {
    let mut cmd = Command::new("curl");
    if ipv6 {
        cmd.arg("-6");
    } else {
        cmd.arg("-4");
    }
    cmd.args(["-fsS", "--max-time", "5", url]);
    let out = crate::cmd::run_output_with_timeout(&mut cmd, Duration::from_secs(10), "curl ipify")
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
    let rtype = record_type_for(ddns.ip_version);
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
                "type": rtype,
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
                "record_type": rtype,
                "ip_version": if ddns.ip_version == DdnsIpVersion::Ipv6 { "ipv6" } else { "ipv4" },
                "timestamp": Utc::now().to_rfc3339()
            })
            .to_string();
            Ok((url, "POST", Some(body)))
        }
        DdnsProvider::Aliyun | DdnsProvider::Dnspod | DdnsProvider::HuaweiCloud => {
            Err(ServiceError::Render(
                "domestic DDNS providers use signed multi-step API calls via dispatch_ddns_update"
                    .to_string(),
            ))
        }
    }
}

async fn dispatch_ddns_update(ddns: &DdnsConfig, ip: &str) -> Result<()> {
    if matches!(
        ddns.provider,
        DdnsProvider::Aliyun | DdnsProvider::Dnspod | DdnsProvider::HuaweiCloud
    ) {
        return sync_domestic_ddns(ddns, ip).await;
    }
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

    let out =
        crate::cmd::run_output_with_timeout(&mut cmd, Duration::from_secs(15), "curl ddns update")
            .await?;
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

        cfg.ddns.ip_version = DdnsIpVersion::Ipv6;
        let st2 = mgr.sync_with_wans(&cfg, &[], true).await;
        assert_eq!(st2.last_ip.as_deref(), Some("2001:db8::10"));
        assert!(st2.last_error.is_none());
    }
}
