use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use chrono::Utc;
use crabrouter_core::CommitStatus;
use serde::Serialize;
use ts_rs::TS;

use crate::AppState;
use crate::api::{ApiError, require_auth_if_initialized};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct InterfaceRateSample {
    pub name: String,
    pub oper_state: String,
    #[ts(type = "number")]
    pub rx_bytes_per_sec: u64,
    #[ts(type = "number")]
    pub tx_bytes_per_sec: u64,
    #[ts(type = "number")]
    pub rx_bytes: u64,
    #[ts(type = "number")]
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SystemMetricsSnapshot {
    pub timestamp: String,
    pub cpu_usage_pct: f32,
    #[ts(type = "number")]
    pub memory_used_bytes: u64,
    #[ts(type = "number")]
    pub memory_total_bytes: u64,
    pub load_avg_1m: f32,
    pub temperature_celsius: Option<f32>,
    pub interfaces: Vec<InterfaceRateSample>,
    pub commit_status: CommitStatus,
}

#[derive(Clone, Copy)]
struct CpuSample {
    total: u64,
    idle: u64,
}

#[derive(Clone, Copy)]
struct NetSample {
    rx_bytes: u64,
    tx_bytes: u64,
}

pub async fn metrics_ws(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    require_auth_if_initialized(&state, &headers)?;
    Ok(ws.on_upgrade(move |socket| handle_metrics_ws(socket, state)))
}

async fn handle_metrics_ws(mut socket: WebSocket, state: AppState) {
    let mut prev_cpu = read_cpu_sample();
    let mut prev_net = read_net_samples();
    let mut prev_at = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let now = Instant::now();
                let elapsed_secs = now.duration_since(prev_at).as_secs_f64().max(0.001);
                prev_at = now;

                let cur_cpu = read_cpu_sample();
                let cpu_usage_pct = compute_cpu_pct(prev_cpu, cur_cpu);
                prev_cpu = cur_cpu;

                let cur_net = read_net_samples();
                let interfaces = compute_interface_rates(&prev_net, &cur_net, elapsed_secs);
                prev_net = cur_net;

                let (memory_total_bytes, memory_used_bytes) = read_mem_bytes();
                let commit_status = state.engine.status().await;

                let snap = SystemMetricsSnapshot {
                    timestamp: Utc::now().to_rfc3339(),
                    cpu_usage_pct,
                    memory_used_bytes,
                    memory_total_bytes,
                    load_avg_1m: read_load_avg_1m(),
                    temperature_celsius: read_temperature_celsius(),
                    interfaces,
                    commit_status,
                };

                if let Ok(json) = serde_json::to_string(&snap)
                    && socket.send(Message::Text(json.into())).await.is_err()
                {
                    break;
                }
            }
            msg = socket.recv() => {
                if msg.is_none() || matches!(msg, Some(Ok(Message::Close(_)) | Err(_))) {
                    break;
                }
            }
        }
    }
}

fn read_cpu_sample() -> Option<CpuSample> {
    let raw = fs::read_to_string("/proc/stat").ok()?;
    let line = raw.lines().next()?;
    let cols: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse::<u64>().ok())
        .collect();
    if cols.len() < 4 {
        return None;
    }
    let idle = cols[3].saturating_add(cols.get(4).copied().unwrap_or(0));
    let total: u64 = cols.iter().sum();
    Some(CpuSample { total, idle })
}

#[allow(clippy::cast_precision_loss)]
fn compute_cpu_pct(prev: Option<CpuSample>, cur: Option<CpuSample>) -> f32 {
    let (Some(p), Some(c)) = (prev, cur) else {
        return 0.0;
    };
    let d_total = c.total.saturating_sub(p.total);
    let d_idle = c.idle.saturating_sub(p.idle);
    if d_total == 0 {
        return 0.0;
    }
    let busy = d_total.saturating_sub(d_idle) as f64;
    ((busy / d_total as f64) * 100.0).clamp(0.0, 100.0) as f32
}

fn read_mem_bytes() -> (u64, u64) {
    let Ok(raw) = fs::read_to_string("/proc/meminfo") else {
        return (0, 0);
    };
    let mut total_kb = 0_u64;
    let mut avail_kb = 0_u64;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = parse_kb(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail_kb = parse_kb(rest);
        }
    }
    let total = total_kb.saturating_mul(1024);
    let used = total_kb.saturating_sub(avail_kb).saturating_mul(1024);
    (total, used)
}

fn parse_kb(rest: &str) -> u64 {
    rest.split_whitespace()
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

fn read_load_avg_1m() -> f32 {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f32>().ok())
        .unwrap_or(0.0)
}

#[allow(clippy::cast_precision_loss)]
fn read_temperature_celsius() -> Option<f32> {
    let thermal = Path::new("/sys/class/thermal");
    let entries = fs::read_dir(thermal).ok()?;
    let mut max_temp: Option<f32> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("thermal_zone") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(entry.path().join("temp"))
            && let Ok(milli) = raw.trim().parse::<i32>()
            && milli > 0
        {
            let c = milli as f32 / 1000.0;
            max_temp = Some(max_temp.map_or(c, |prev| prev.max(c)));
        }
    }
    max_temp
}

fn read_net_samples() -> HashMap<String, NetSample> {
    let mut map = HashMap::new();
    let Ok(raw) = fs::read_to_string("/proc/net/dev") else {
        return map;
    };
    for line in raw.lines().skip(2) {
        let Some((iface_part, rest)) = line.split_once(':') else {
            continue;
        };
        let name = iface_part.trim().to_string();
        if name == "lo" {
            continue;
        }
        let cols: Vec<&str> = rest.split_whitespace().collect();
        if cols.len() < 9 {
            continue;
        }
        let rx_bytes = cols[0].parse().unwrap_or(0);
        let tx_bytes = cols[8].parse().unwrap_or(0);
        map.insert(name, NetSample { rx_bytes, tx_bytes });
    }
    map
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn compute_interface_rates(
    prev: &HashMap<String, NetSample>,
    cur: &HashMap<String, NetSample>,
    elapsed_secs: f64,
) -> Vec<InterfaceRateSample> {
    let mut out: Vec<InterfaceRateSample> = cur
        .iter()
        .map(|(name, sample)| {
            let p = prev.get(name).copied().unwrap_or(*sample);
            let rx_delta = sample.rx_bytes.saturating_sub(p.rx_bytes);
            let tx_delta = sample.tx_bytes.saturating_sub(p.tx_bytes);
            let oper_state = fs::read_to_string(format!("/sys/class/net/{name}/operstate"))
                .map_or_else(|_| "unknown".to_string(), |s| s.trim().to_string());
            InterfaceRateSample {
                name: name.clone(),
                oper_state,
                rx_bytes_per_sec: (rx_delta as f64 / elapsed_secs) as u64,
                tx_bytes_per_sec: (tx_delta as f64 / elapsed_secs) as u64,
                rx_bytes: sample.rx_bytes,
                tx_bytes: sample.tx_bytes,
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
