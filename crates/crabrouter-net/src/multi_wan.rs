use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use crabrouter_model::{
    MultiWanConfig, MultiWanMode, MultiWanRuntimeStatus, ProbeProtocol, RouterConfig, WanConfig,
    WanHealthState, WanProbeResult, WanRuntimeState, wan_fwmark, wan_table_id,
};
use futures_util::future::join_all;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpSocket;
use tokio::process::Command;
use tokio::sync::{RwLock, watch};

use crate::conntrack::flush_conntrack_for_wan;
use crate::events::{WanEventBus, WanStateObserver};
use crate::netlink::apply_rtnetlink_diff_with_wans;
use crate::nftables::apply_nft_with_precheck_and_multi_wan;

#[derive(Clone)]
pub struct MultiWanManager {
    observer: Arc<dyn WanStateObserver>,
    wan_bus: WanEventBus,
    status: Arc<RwLock<MultiWanRuntimeStatus>>,
    dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    pub ok: bool,
    pub rtt_ms: Option<f64>,
    pub target: Option<String>,
    pub error: Option<String>,
}

/// Spawns the Multi-WAN health probing and failover/load-balancing engine.
pub fn spawn_multi_wan_engine(
    observer: Arc<dyn WanStateObserver>,
    wan_bus: WanEventBus,
    dry_run: bool,
) -> MultiWanManager {
    let (_tx, rx) = watch::channel(false);
    spawn_multi_wan_engine_with_shutdown(observer, wan_bus, dry_run, rx)
}

/// Spawns the Multi-WAN health probing engine with an explicit graceful-shutdown signal.
pub fn spawn_multi_wan_engine_with_shutdown(
    observer: Arc<dyn WanStateObserver>,
    wan_bus: WanEventBus,
    dry_run: bool,
    shutdown: watch::Receiver<bool>,
) -> MultiWanManager {
    let cfg = observer.current_config();
    let initial = MultiWanRuntimeStatus {
        enabled: cfg.multi_wan.enabled,
        mode: cfg.multi_wan.mode,
        active_wans: Vec::new(),
        wans: Vec::new(),
        last_failover_at: None,
        failover_count: 0,
    };
    let manager = MultiWanManager {
        observer,
        wan_bus,
        status: Arc::new(RwLock::new(initial)),
        dry_run,
    };

    let bg = manager.clone();
    tokio::spawn(async move {
        bg.run_loop(shutdown).await;
    });

    manager
}

impl MultiWanManager {
    pub async fn snapshot(&self) -> MultiWanRuntimeStatus {
        self.status.read().await.clone()
    }

    async fn run_loop(&self, mut shutdown: watch::Receiver<bool>) {
        loop {
            if *shutdown.borrow() {
                break;
            }
            let cfg = self.observer.current_config();
            let interval_secs = u64::from(cfg.multi_wan.probe_interval_secs.max(1));
            if cfg.multi_wan.enabled {
                let _ = self.run_probe_cycle().await;
            } else {
                self.sync_disabled_state(&cfg).await;
            }
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(interval_secs)) => {}
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("multi-wan engine received shutdown signal");
                        break;
                    }
                }
            }
        }
    }

    async fn sync_disabled_state(&self, cfg: &RouterConfig) {
        let mut guard = self.status.write().await;
        if guard.enabled {
            guard.enabled = false;
            guard.mode = cfg.multi_wan.mode;
            guard.active_wans.clear();
            guard.wans.clear();
        }
    }

    /// Executes one full Multi-WAN probe cycle and applies failover/rebalance if needed.
    pub async fn run_probe_cycle(&self) -> MultiWanRuntimeStatus {
        let cfg = self.observer.current_config();
        let wan_states = self.wan_bus.snapshot().await;
        let prev_status = self.status.read().await.clone();

        if !cfg.multi_wan.enabled {
            let disabled = MultiWanRuntimeStatus {
                enabled: false,
                mode: cfg.multi_wan.mode,
                active_wans: Vec::new(),
                wans: Vec::new(),
                last_failover_at: prev_status.last_failover_at,
                failover_count: prev_status.failover_count,
            };
            *self.status.write().await = disabled.clone();
            return disabled;
        }

        let futures = cfg.wans.iter().enumerate().map(|(idx, wan)| {
            let rt = wan_states.iter().find(|s| s.wan_name == wan.name).cloned();
            let prev = prev_status
                .wans
                .iter()
                .find(|p| p.wan_name == wan.name)
                .cloned();
            let multi_cfg = cfg.multi_wan.clone();
            let dry_run = self.dry_run;
            async move {
                let table_id = wan_table_id(idx, wan);
                let fwmark = wan_fwmark(table_id);
                let outcome = if wan.health_check_enabled {
                    probe_wan(wan, rt.as_ref(), &multi_cfg, dry_run).await
                } else {
                    ProbeOutcome {
                        ok: true,
                        rtt_ms: None,
                        target: None,
                        error: None,
                    }
                };
                update_probe_state(prev.as_ref(), wan, table_id, fwmark, &outcome, &multi_cfg)
            }
        });

        let mut results: Vec<WanProbeResult> = join_all(futures).await;
        let health_map: HashMap<String, WanHealthState> = results
            .iter()
            .map(|r| (r.wan_name.clone(), r.health))
            .collect();
        let active_wans = compute_active_wans(&cfg, &health_map);

        for res in &mut results {
            let is_active = active_wans.iter().any(|n| n == &res.wan_name);
            res.active = is_active;
            res.effective_weight = if is_active { res.weight.max(1) } else { 0 };
        }

        let topology_changed =
            prev_status.active_wans != active_wans && !prev_status.wans.is_empty();
        let mut failover_count = prev_status.failover_count;
        let mut last_failover_at = prev_status.last_failover_at.clone();

        if topology_changed {
            failover_count = failover_count.saturating_add(1);
            last_failover_at = Some(Utc::now().to_rfc3339());
            self.handle_failover_transition(
                &cfg,
                &wan_states,
                &prev_status.active_wans,
                &active_wans,
                &results,
            )
            .await;
        }

        let next_status = MultiWanRuntimeStatus {
            enabled: true,
            mode: cfg.multi_wan.mode,
            active_wans,
            wans: results,
            last_failover_at,
            failover_count,
        };
        *self.status.write().await = next_status.clone();
        next_status
    }

    async fn handle_failover_transition(
        &self,
        cfg: &RouterConfig,
        wan_states: &[WanRuntimeState],
        prev_active: &[String],
        new_active: &[String],
        results: &[WanProbeResult],
    ) {
        tracing::warn!(
            prev = ?prev_active,
            next = ?new_active,
            mode = ?cfg.multi_wan.mode,
            "Multi-WAN active uplink set changed"
        );

        if cfg.multi_wan.flush_conntrack_on_failover {
            for wan_name in prev_active {
                if !new_active.iter().any(|n| n == wan_name) {
                    let wan_ip = wan_states
                        .iter()
                        .find(|s| &s.wan_name == wan_name)
                        .and_then(|s| s.ip.as_deref());
                    let fwmark = results
                        .iter()
                        .find(|r| &r.wan_name == wan_name)
                        .map(|r| r.fwmark);
                    flush_conntrack_for_wan(wan_ip, fwmark, self.dry_run).await;
                }
            }
        }

        if !self.dry_run {
            if let Err(err) = apply_rtnetlink_diff_with_wans(cfg, wan_states, new_active).await {
                tracing::error!(error = %err, "Multi-WAN rtnetlink update failed");
            }
            if let Err(err) = apply_nft_with_precheck_and_multi_wan(cfg, Some(new_active)).await {
                tracing::error!(error = %err, "Multi-WAN nftables update failed");
            }
        }
    }
}

/// Computes the ordered list of active WAN names based on mode, metric priority, and health.
pub fn compute_active_wans<S: std::hash::BuildHasher>(
    cfg: &RouterConfig,
    health_map: &HashMap<String, WanHealthState, S>,
) -> Vec<String> {
    if cfg.wans.is_empty() {
        return Vec::new();
    }

    let mut ordered: Vec<(usize, &WanConfig)> = cfg.wans.iter().enumerate().collect();
    ordered.sort_by_key(|(idx, wan)| (wan.metric, *idx));

    let healthy: Vec<String> = ordered
        .iter()
        .filter(|(_, w)| !matches!(health_map.get(&w.name).copied(), Some(WanHealthState::Down)))
        .map(|(_, w)| w.name.clone())
        .collect();

    if healthy.is_empty() {
        // Fallback to lowest-metric WAN to avoid total blackhole if probe targets are unreachable.
        return vec![ordered[0].1.name.clone()];
    }

    match cfg.multi_wan.mode {
        MultiWanMode::Failover => vec![healthy[0].clone()],
        MultiWanMode::LoadBalance => healthy,
    }
}

/// Applies hysteresis thresholds to update a WAN's health state from a single probe outcome.
pub fn update_probe_state(
    prev: Option<&WanProbeResult>,
    wan: &WanConfig,
    table_id: u32,
    fwmark: u32,
    outcome: &ProbeOutcome,
    cfg: &MultiWanConfig,
) -> WanProbeResult {
    let prev_health = prev.map_or(WanHealthState::Unknown, |p| p.health);
    let prev_fails = prev.map_or(0, |p| p.consecutive_failures);
    let prev_succs = prev.map_or(0, |p| p.consecutive_successes);

    let (health, fails, succs, loss_pct) = if outcome.ok {
        let s = prev_succs.saturating_add(1);
        let next_health = if prev_health == WanHealthState::Down && s < cfg.recover_threshold {
            WanHealthState::Down
        } else {
            WanHealthState::Healthy
        };
        (next_health, 0, s, 0)
    } else {
        let f = prev_fails.saturating_add(1);
        let next_health = if f >= cfg.fail_threshold {
            WanHealthState::Down
        } else if prev_health == WanHealthState::Healthy {
            WanHealthState::Degraded
        } else {
            prev_health
        };
        (next_health, f, 0, 100)
    };

    WanProbeResult {
        wan_name: wan.name.clone(),
        device: wan.device.clone(),
        table_id,
        fwmark,
        weight: wan.weight.max(1),
        metric: wan.metric,
        health,
        active: false,
        effective_weight: 0,
        rtt_ms: outcome.rtt_ms,
        packet_loss_pct: loss_pct,
        consecutive_failures: fails,
        consecutive_successes: succs,
        last_target: outcome.target.clone(),
        last_error: outcome.error.clone(),
        last_checked_at: Some(Utc::now().to_rfc3339()),
    }
}

async fn probe_wan(
    wan: &WanConfig,
    rt: Option<&WanRuntimeState>,
    cfg: &MultiWanConfig,
    dry_run: bool,
) -> ProbeOutcome {
    let targets = if wan.probe_targets.is_empty() {
        &cfg.probe_targets
    } else {
        &wan.probe_targets
    };
    let first_target = targets.first().cloned();

    if dry_run {
        return ProbeOutcome {
            ok: true,
            rtt_ms: Some(12.4),
            target: first_target,
            error: None,
        };
    }

    let iface = rt.map_or(wan.device.as_str(), |s| s.device.as_str());
    let src_ip = rt
        .and_then(|s| s.ip.as_deref())
        .and_then(|s| s.parse::<IpAddr>().ok());
    let timeout = Duration::from_millis(u64::from(cfg.probe_timeout_ms.max(200)));

    let mut last_err = None;
    for target in targets {
        let res = match cfg.probe_protocol {
            ProbeProtocol::Icmp => probe_icmp(iface, target, timeout).await,
            ProbeProtocol::Http => probe_http(src_ip, target, timeout).await,
            ProbeProtocol::IcmpAndHttp => {
                if target.starts_with("http://") {
                    probe_http(src_ip, target, timeout).await
                } else {
                    probe_icmp(iface, target, timeout).await
                }
            }
        };
        if res.ok {
            return res;
        }
        last_err = res.error;
    }

    ProbeOutcome {
        ok: false,
        rtt_ms: None,
        target: first_target,
        error: last_err.or_else(|| Some("all probe targets failed".to_string())),
    }
}

async fn probe_icmp(iface: &str, target: &str, timeout: Duration) -> ProbeOutcome {
    let secs = timeout.as_secs().max(1).to_string();
    let host = strip_url_host(target);
    let start = Instant::now();
    let out = Command::new("ping")
        .args(["-I", iface, "-c", "1", "-W", &secs, host])
        .output()
        .await;

    match out {
        Ok(res) if res.status.success() => {
            let stdout = String::from_utf8_lossy(&res.stdout);
            let rtt = parse_ping_rtt_ms(&stdout).unwrap_or(start.elapsed().as_secs_f64() * 1000.0);
            ProbeOutcome {
                ok: true,
                rtt_ms: Some(rtt),
                target: Some(target.to_string()),
                error: None,
            }
        }
        Ok(res) => {
            let stderr = String::from_utf8_lossy(&res.stderr).trim().to_string();
            ProbeOutcome {
                ok: false,
                rtt_ms: None,
                target: Some(target.to_string()),
                error: Some(if stderr.is_empty() {
                    format!("ping exit {}", res.status)
                } else {
                    stderr
                }),
            }
        }
        Err(err) => ProbeOutcome {
            ok: false,
            rtt_ms: None,
            target: Some(target.to_string()),
            error: Some(err.to_string()),
        },
    }
}

async fn probe_http(src_ip: Option<IpAddr>, target: &str, timeout: Duration) -> ProbeOutcome {
    let (host, port, path) = parse_http_target(target);
    let addr_str = format!("{host}:{port}");
    let start = Instant::now();

    let fut = async {
        let remote: SocketAddr = tokio::net::lookup_host(&addr_str)
            .await?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "DNS lookup empty"))?;
        let socket = match remote {
            SocketAddr::V4(_) => TcpSocket::new_v4()?,
            SocketAddr::V6(_) => TcpSocket::new_v6()?,
        };
        if let Some(ip) = src_ip
            && ip.is_ipv4() == remote.is_ipv4()
        {
            socket.bind(SocketAddr::new(ip, 0))?;
        }
        let mut stream = socket.connect(remote).await?;
        let req = format!("HEAD {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        stream.write_all(req.as_bytes()).await?;
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf).await?;
        let head = String::from_utf8_lossy(&buf[..n]);
        if head.starts_with("HTTP/1.") {
            Ok::<(), std::io::Error>(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid HTTP response",
            ))
        }
    };

    match tokio::time::timeout(timeout, fut).await {
        Ok(Ok(())) => ProbeOutcome {
            ok: true,
            rtt_ms: Some(start.elapsed().as_secs_f64() * 1000.0),
            target: Some(target.to_string()),
            error: None,
        },
        Ok(Err(err)) => ProbeOutcome {
            ok: false,
            rtt_ms: None,
            target: Some(target.to_string()),
            error: Some(err.to_string()),
        },
        Err(_) => ProbeOutcome {
            ok: false,
            rtt_ms: None,
            target: Some(target.to_string()),
            error: Some("HTTP probe timed out".to_string()),
        },
    }
}

fn strip_url_host(target: &str) -> &str {
    let trimmed = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
        .unwrap_or(target);
    let host_port = trimmed.split('/').next().unwrap_or(trimmed);
    host_port.split(':').next().unwrap_or(host_port)
}

fn parse_http_target(target: &str) -> (&str, u16, &str) {
    let trimmed = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
        .unwrap_or(target);
    let (host_port, path) = match trimmed.find('/') {
        Some(idx) => (&trimmed[..idx], &trimmed[idx..]),
        None => (trimmed, "/"),
    };
    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().unwrap_or(80)),
        None => (host_port, 80),
    };
    (host, port, path)
}

/// Extracts `time=12.3 ms` from `ping` stdout.
pub fn parse_ping_rtt_ms(stdout: &str) -> Option<f64> {
    for token in stdout.split_whitespace() {
        if let Some(val) = token.strip_prefix("time=") {
            return val.trim_end_matches("ms").parse::<f64>().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabrouter_model::WanProto;

    fn make_wan(name: &str, metric: u32, weight: u32) -> WanConfig {
        WanConfig {
            name: name.to_string(),
            device: format!("eth_{name}"),
            proto: WanProto::Dhcp,
            static_cidr: None,
            gateway: None,
            pppoe_username: None,
            pppoe_password: None,
            mtu: 1500,
            ipv6_pd: false,
            ipv6_pd_prefix_len: 60,
            metric,
            weight,
            table_id: None,
            health_check_enabled: true,
            probe_targets: Vec::new(),
        }
    }

    #[test]
    fn failover_selects_lowest_metric_healthy_wan_and_falls_back() {
        let cfg = RouterConfig {
            wans: vec![make_wan("wan_b", 200, 1), make_wan("wan_a", 100, 2)],
            multi_wan: MultiWanConfig {
                enabled: true,
                mode: MultiWanMode::Failover,
                ..MultiWanConfig::default()
            },
            ..RouterConfig::default()
        };

        let mut health = HashMap::new();
        health.insert("wan_a".to_string(), WanHealthState::Healthy);
        health.insert("wan_b".to_string(), WanHealthState::Healthy);
        assert_eq!(compute_active_wans(&cfg, &health), vec!["wan_a"]);

        health.insert("wan_a".to_string(), WanHealthState::Down);
        assert_eq!(compute_active_wans(&cfg, &health), vec!["wan_b"]);

        // Both down -> fallback to lowest metric wan_a
        health.insert("wan_b".to_string(), WanHealthState::Down);
        assert_eq!(compute_active_wans(&cfg, &health), vec!["wan_a"]);
    }

    #[test]
    fn load_balance_includes_all_healthy_wans() {
        let cfg = RouterConfig {
            wans: vec![make_wan("wan1", 100, 3), make_wan("wan2", 110, 1)],
            multi_wan: MultiWanConfig {
                enabled: true,
                mode: MultiWanMode::LoadBalance,
                ..MultiWanConfig::default()
            },
            ..RouterConfig::default()
        };

        let mut health = HashMap::new();
        health.insert("wan1".to_string(), WanHealthState::Healthy);
        health.insert("wan2".to_string(), WanHealthState::Healthy);
        assert_eq!(compute_active_wans(&cfg, &health), vec!["wan1", "wan2"]);

        health.insert("wan1".to_string(), WanHealthState::Down);
        assert_eq!(compute_active_wans(&cfg, &health), vec!["wan2"]);
    }

    #[test]
    fn hysteresis_transitions_healthy_degraded_down_and_recover() {
        let wan = make_wan("wan1", 100, 1);
        let cfg = MultiWanConfig {
            fail_threshold: 3,
            recover_threshold: 2,
            ..MultiWanConfig::default()
        };
        let fail = ProbeOutcome {
            ok: false,
            rtt_ms: None,
            target: Some("1.1.1.1".to_string()),
            error: Some("timeout".to_string()),
        };
        let ok = ProbeOutcome {
            ok: true,
            rtt_ms: Some(10.2),
            target: Some("1.1.1.1".to_string()),
            error: None,
        };

        let s1 = update_probe_state(None, &wan, 101, 0x1065, &ok, &cfg);
        assert_eq!(s1.health, WanHealthState::Healthy);

        let s2 = update_probe_state(Some(&s1), &wan, 101, 0x1065, &fail, &cfg);
        assert_eq!(s2.health, WanHealthState::Degraded);
        let s3 = update_probe_state(Some(&s2), &wan, 101, 0x1065, &fail, &cfg);
        assert_eq!(s3.health, WanHealthState::Degraded);
        let s4 = update_probe_state(Some(&s3), &wan, 101, 0x1065, &fail, &cfg);
        assert_eq!(s4.health, WanHealthState::Down);

        // First success stays Down until recover_threshold (2) is reached.
        let s5 = update_probe_state(Some(&s4), &wan, 101, 0x1065, &ok, &cfg);
        assert_eq!(s5.health, WanHealthState::Down);
        let s6 = update_probe_state(Some(&s5), &wan, 101, 0x1065, &ok, &cfg);
        assert_eq!(s6.health, WanHealthState::Healthy);
    }

    #[test]
    fn parses_ping_rtt_output() {
        let out = "64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=14.8 ms";
        assert_eq!(parse_ping_rtt_ms(out), Some(14.8));
    }
}
