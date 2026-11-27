use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crabrouter_model::HostInterfaceSnapshot;

use crate::Result;

#[derive(Default)]
struct NetDevCounters {
    rx_bytes: u64,
    rx_packets: u64,
    tx_bytes: u64,
    tx_packets: u64,
}

/// Returns `true` for ephemeral container veth interfaces (`veth*`) that should not pollute
/// physical/bridge interface snapshots.
pub fn is_ephemeral_container_iface(name: &str) -> bool {
    name.starts_with("veth")
}

/// Reads current interface metadata and traffic counters from `/sys/class/net` and `/proc/net/dev`.
pub fn read_host_interfaces() -> Result<Vec<HostInterfaceSnapshot>> {
    let counters = read_proc_net_dev().unwrap_or_default();
    let sys_net = Path::new("/sys/class/net");
    if !sys_net.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    for entry in fs::read_dir(sys_net)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "lo" || is_ephemeral_container_iface(&name) {
            continue;
        }
        let base = entry.path();
        let mac = read_trimmed(&base.join("address"));
        let oper_state = read_trimmed(&base.join("operstate")).unwrap_or_else(|| "unknown".into());
        let mtu = read_trimmed(&base.join("mtu"))
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1500);
        let speed_mbps = read_trimmed(&base.join("speed"))
            .and_then(|v| v.parse::<i32>().ok())
            .filter(|&v| v > 0)
            .map(i32::cast_unsigned);
        let c = counters.get(&name);

        result.push(HostInterfaceSnapshot {
            name,
            mac,
            oper_state,
            mtu,
            speed_mbps,
            rx_bytes: c.map_or(0, |v| v.rx_bytes),
            tx_bytes: c.map_or(0, |v| v.tx_bytes),
            rx_packets: c.map_or(0, |v| v.rx_packets),
            tx_packets: c.map_or(0, |v| v.tx_packets),
        });
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_proc_net_dev() -> Result<HashMap<String, NetDevCounters>> {
    let raw = fs::read_to_string("/proc/net/dev")?;
    let mut map = HashMap::new();
    for line in raw.lines().skip(2) {
        let Some((iface_part, rest)) = line.split_once(':') else {
            continue;
        };
        let cols: Vec<&str> = rest.split_whitespace().collect();
        if cols.len() < 10 {
            continue;
        }
        let rx_bytes = cols[0].parse().unwrap_or(0);
        let rx_packets = cols[1].parse().unwrap_or(0);
        let tx_bytes = cols[8].parse().unwrap_or(0);
        let tx_packets = cols[9].parse().unwrap_or(0);
        map.insert(
            iface_part.trim().to_string(),
            NetDevCounters {
                rx_bytes,
                rx_packets,
                tx_bytes,
                tx_packets,
            },
        );
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::is_ephemeral_container_iface;

    #[test]
    fn filters_veth_but_keeps_bridges_and_nics() {
        assert!(is_ephemeral_container_iface("veth1a2b3c"));
        assert!(is_ephemeral_container_iface("veth0"));
        assert!(!is_ephemeral_container_iface("eth0"));
        assert!(!is_ephemeral_container_iface("br-lan"));
        assert!(!is_ephemeral_container_iface("podman0"));
        assert!(!is_ephemeral_container_iface("docker0"));
    }
}
