use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use crabrouter_model::{NicCapabilitiesSnapshot, NicOffloadCapabilities, OffloadFeatureState};
use tokio::process::Command;

use crate::cmd::run_output_with_timeout;
use crate::{NetError, Result};

const ETHTOOL_TIMEOUT: Duration = Duration::from_secs(5);

/// Validates an interface name before touching `sysfs` paths or spawning `ethtool`.
pub fn validate_iface_name(iface: &str) -> Result<&str> {
    let trimmed = iface.trim();
    if trimmed.is_empty()
        || trimmed.len() > 15
        || trimmed.contains('/')
        || trimmed.contains("..")
        || trimmed.contains(char::is_whitespace)
    {
        return Err(NetError::Apply(format!("invalid interface name '{iface}'")));
    }
    Ok(trimmed)
}

/// Probes read-only `NIC` driver capabilities via `ethtool` and `sysfs`/`procfs`.
pub async fn probe_nic_capabilities(iface: &str) -> Result<NicCapabilitiesSnapshot> {
    let iface = validate_iface_name(iface)?;
    let net_dir = Path::new("/sys/class/net").join(iface);

    let sysfs_driver = read_sysfs_driver(&net_dir);
    let (rx_queues, tx_queues) = count_sysfs_queues(&net_dir);
    let cpu_cores = std::thread::available_parallelism()
        .map_or(1, |n| u32::try_from(n.get()).unwrap_or(u32::MAX));
    let rx_steering_mask = read_trimmed(&net_dir.join("queues/rx-0/rps_cpus"));
    let tx_steering_mask = read_trimmed(&net_dir.join("queues/tx-0/xps_cpus"));
    let flow_entries = read_trimmed(Path::new("/proc/sys/net/core/rps_sock_flow_entries"))
        .and_then(|s| s.parse::<u32>().ok());

    let mut snap = NicCapabilitiesSnapshot {
        interface: iface.to_string(),
        driver: sysfs_driver,
        driver_version: None,
        bus_info: None,
        ethtool_available: false,
        offloads: NicOffloadCapabilities::default(),
        pause_autoneg: None,
        pause_rx: None,
        pause_tx: None,
        rx_ring_current: None,
        rx_ring_max: None,
        tx_ring_current: None,
        tx_ring_max: None,
        rx_queues,
        tx_queues,
        cpu_cores,
        current_rps_cpus: rx_steering_mask,
        current_xps_cpus: tx_steering_mask,
        current_rfs_entries: flow_entries,
    };

    if let Some(out) = run_ethtool_query("-i", iface).await? {
        snap.ethtool_available = true;
        let (drv, ver, bus) = parse_ethtool_driver(&out);
        if drv.is_some() {
            snap.driver = drv;
        }
        snap.driver_version = ver;
        snap.bus_info = bus;
    } else {
        return Ok(snap);
    }

    if let Some(out) = run_ethtool_query("-k", iface).await? {
        snap.offloads = parse_ethtool_offloads(&out);
    }
    if let Some(out) = run_ethtool_query("-a", iface).await? {
        let (auto, rx, tx) = parse_ethtool_pause(&out);
        snap.pause_autoneg = auto;
        snap.pause_rx = rx;
        snap.pause_tx = tx;
    }
    if let Some(out) = run_ethtool_query("-g", iface).await? {
        let (rx_cur, rx_max, tx_cur, tx_max) = parse_ethtool_rings(&out);
        snap.rx_ring_current = rx_cur;
        snap.rx_ring_max = rx_max;
        snap.tx_ring_current = tx_cur;
        snap.tx_ring_max = tx_max;
    }

    Ok(snap)
}

/// Runs `ethtool <flag> <iface>`. Returns `Ok(None)` if `ethtool` binary is missing,
/// or `Ok(Some(stdout))` when available (ignoring non-zero subcommand exits cleanly).
async fn run_ethtool_query(flag: &str, iface: &str) -> Result<Option<String>> {
    let mut cmd = Command::new("ethtool");
    cmd.args([flag, iface]);
    match run_output_with_timeout(&mut cmd, ETHTOOL_TIMEOUT, "ethtool").await {
        Ok(out) if out.status.success() => {
            Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::debug!(flag, iface, stderr = %stderr.trim(), "ethtool query returned non-zero");
            Ok(Some(String::new()))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(NetError::Io(err)),
    }
}

fn read_sysfs_driver(net_dir: &Path) -> Option<String> {
    let link = fs::read_link(net_dir.join("device/driver")).ok()?;
    link.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub(super) fn count_sysfs_queues(net_dir: &Path) -> (u32, u32) {
    let Ok(entries) = fs::read_dir(net_dir.join("queues")) else {
        return (0, 0);
    };
    let mut rx = 0u32;
    let mut tx = 0u32;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if s.starts_with("rx-") {
            rx = rx.saturating_add(1);
        } else if s.starts_with("tx-") {
            tx = tx.saturating_add(1);
        }
    }
    (rx, tx)
}

pub(super) fn parse_ethtool_driver(raw: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut driver = None;
    let mut version = None;
    let mut bus_info = None;
    for line in raw.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();
        if val.is_empty() {
            continue;
        }
        match key {
            "driver" => driver = Some(val.to_string()),
            "version" => version = Some(val.to_string()),
            "bus-info" => bus_info = Some(val.to_string()),
            _ => {}
        }
    }
    (driver, version, bus_info)
}

pub(super) fn parse_ethtool_offloads(raw: &str) -> NicOffloadCapabilities {
    let mut caps = NicOffloadCapabilities::default();
    for line in raw.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let state = parse_offload_value(v.trim());
        match key {
            "generic-receive-offload" => caps.gro = state,
            "generic-segmentation-offload" => caps.gso = state,
            "tcp-segmentation-offload" => caps.tso = state,
            "large-receive-offload" => caps.lro = state,
            "rx-checksumming" => caps.rx_checksum = state,
            "tx-checksumming" => caps.tx_checksum = state,
            _ => {}
        }
    }
    caps
}

fn parse_offload_value(val: &str) -> Option<OffloadFeatureState> {
    let fixed = val.contains("[fixed]");
    let token = val.split_whitespace().next()?;
    let enabled = match token {
        "on" => true,
        "off" => false,
        _ => return None,
    };
    Some(OffloadFeatureState { enabled, fixed })
}

pub(super) fn parse_ethtool_pause(raw: &str) -> (Option<bool>, Option<bool>, Option<bool>) {
    let mut autoneg = None;
    let mut rx = None;
    let mut tx = None;
    for line in raw.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let parsed = parse_on_off(v.trim());
        match k.trim() {
            "Autonegotiate" => autoneg = parsed,
            "RX" => rx = parsed,
            "TX" => tx = parsed,
            _ => {}
        }
    }
    (autoneg, rx, tx)
}

fn parse_on_off(val: &str) -> Option<bool> {
    match val.split_whitespace().next() {
        Some("on") => Some(true),
        Some("off") => Some(false),
        _ => None,
    }
}

pub(super) fn parse_ethtool_rings(
    raw: &str,
) -> (Option<u32>, Option<u32>, Option<u32>, Option<u32>) {
    enum Section {
        None,
        Max,
        Current,
    }

    let mut sec = Section::None;
    let (mut rx_cur, mut rx_max, mut tx_cur, mut tx_max) = (None, None, None, None);

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Pre-set maximums:") {
            sec = Section::Max;
            continue;
        }
        if trimmed.starts_with("Current hardware settings:") {
            sec = Section::Current;
            continue;
        }
        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        let Ok(num) = v.trim().parse::<u32>() else {
            continue;
        };
        match (&sec, k.trim()) {
            (Section::Max, "RX") => rx_max = Some(num),
            (Section::Max, "TX") => tx_max = Some(num),
            (Section::Current, "RX") => rx_cur = Some(num),
            (Section::Current, "TX") => tx_cur = Some(num),
            _ => {}
        }
    }

    (rx_cur, rx_max, tx_cur, tx_max)
}
