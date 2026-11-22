use std::fs;
use std::net::IpAddr;
use std::path::Path;

use crabrouter_model::{ConntrackEntry, ConntrackKillRequest};
use tokio::process::Command;

use crate::{NetError, Result};

/// Reads the live Linux kernel connection tracking table via Netlink (`conntrack -L -o extended`),
/// falling back to `/proc/net/nf_conntrack` or `/proc/net/ip_conntrack`.
///
/// In `dry_run` mode, if the host does not expose `nf_conntrack` to unprivileged processes, returns
/// representative synthetic entries so the `WebUI` can be previewed and filtered locally.
pub async fn read_conntrack_table(dry_run: bool) -> Result<Vec<ConntrackEntry>> {
    if let Ok(out) = Command::new("conntrack")
        .args(["-L", "-o", "extended"])
        .output()
        .await
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        let parsed = parse_conntrack_table(&text);
        if !parsed.is_empty() || !dry_run {
            return Ok(parsed);
        }
    }

    for proc_path in ["/proc/net/nf_conntrack", "/proc/net/ip_conntrack"] {
        let p = Path::new(proc_path);
        if p.exists()
            && let Ok(raw) = fs::read_to_string(p)
        {
            let parsed = parse_conntrack_table(&raw);
            if !parsed.is_empty() || !dry_run {
                return Ok(parsed);
            }
        }
    }

    if dry_run {
        return Ok(sample_dry_run_entries());
    }

    Ok(Vec::new())
}

/// Terminates matching tracked connection(s) in the kernel conntrack table via Netlink (`conntrack -D`).
pub async fn kill_conntrack_flow(req: &ConntrackKillRequest, dry_run: bool) -> Result<u32> {
    let src_ip: IpAddr = req
        .src
        .trim()
        .parse()
        .map_err(|e| NetError::Apply(format!("invalid source IP '{}': {e}", req.src)))?;
    let dst_ip: IpAddr = req
        .dst
        .trim()
        .parse()
        .map_err(|e| NetError::Apply(format!("invalid destination IP '{}': {e}", req.dst)))?;

    let proto = req.proto.trim().to_ascii_lowercase();
    if !matches!(proto.as_str(), "tcp" | "udp" | "icmp" | "ipv6-icmp") {
        return Err(NetError::Apply(format!(
            "unsupported conntrack protocol '{proto}'"
        )));
    }

    if dry_run {
        tracing::info!(
            proto = %proto,
            src = %src_ip,
            dst = %dst_ip,
            sport = ?req.sport,
            dport = ?req.dport,
            "dry-run: simulated conntrack -D flow deletion"
        );
        return Ok(1);
    }

    let family = match req.family.as_deref() {
        Some("ipv6") => "ipv6",
        Some("ipv4") => "ipv4",
        _ => match src_ip {
            IpAddr::V4(_) => "ipv4",
            IpAddr::V6(_) => "ipv6",
        },
    };

    let src_s = src_ip.to_string();
    let dst_s = dst_ip.to_string();
    let mut args: Vec<String> = vec![
        "-D".into(),
        "-f".into(),
        family.into(),
        "-p".into(),
        proto.clone(),
        "-s".into(),
        src_s,
        "-d".into(),
        dst_s,
    ];
    if let Some(sp) = req.sport
        && matches!(proto.as_str(), "tcp" | "udp")
    {
        args.push("--orig-port-src".into());
        args.push(sp.to_string());
    }
    if let Some(dp) = req.dport
        && matches!(proto.as_str(), "tcp" | "udp")
    {
        args.push("--orig-port-dst".into());
        args.push(dp.to_string());
    }

    let ref_args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = Command::new("conntrack").args(&ref_args).output().await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(NetError::Apply(format!("conntrack -D failed: {stderr}")));
    }

    Ok(1)
}

/// Parses extended netfilter conntrack lines from `conntrack -L -o extended` or `/proc/net/nf_conntrack`.
pub fn parse_conntrack_table(raw: &str) -> Vec<ConntrackEntry> {
    let mut entries: Vec<ConntrackEntry> = raw.lines().filter_map(parse_conntrack_line).collect();
    entries.sort_by(|a, b| {
        let b_total = b.orig_bytes.saturating_add(b.reply_bytes);
        let a_total = a.orig_bytes.saturating_add(a.reply_bytes);
        b_total.cmp(&a_total)
    });
    entries
}

fn parse_conntrack_line(line: &str) -> Option<ConntrackEntry> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 6 {
        return None;
    }

    // Standard extended format:
    // `ipv4 2 tcp 6 431999 ESTABLISHED src=192.168.10.105 dst=1.1.1.1 sport=54321 dport=443 packets=12 bytes=1500 ...`
    // Or non-extended format:
    // `tcp 6 431999 ESTABLISHED src=...`
    let (family, proto_idx) = if matches!(tokens[0], "ipv4" | "ipv6") {
        (tokens[0].to_string(), 2usize)
    } else {
        ("ipv4".to_string(), 0usize)
    };

    let proto = (*tokens.get(proto_idx)?).to_ascii_lowercase();
    let timeout_secs = tokens
        .get(proto_idx + 2)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut state = String::new();
    let mut src_orig: Option<String> = None;
    let mut dst_orig: Option<String> = None;
    let mut sport_orig: Option<u16> = None;
    let mut dport_orig: Option<u16> = None;
    let mut packets_vals: Vec<u64> = Vec::with_capacity(2);
    let mut bytes_vals: Vec<u64> = Vec::with_capacity(2);
    let mut mark: u32 = 0;
    let mut assured = false;
    let mut offloaded = false;

    for &tok in tokens.iter().skip(proto_idx + 3) {
        if tok == "[ASSURED]" {
            assured = true;
            continue;
        }
        if tok == "[OFFLOAD]" || tok == "[HW_OFFLOAD]" {
            offloaded = true;
            continue;
        }
        if !tok.contains('=') && !tok.starts_with('[') && state.is_empty() {
            state = tok.to_string();
            continue;
        }
        let Some((k, v)) = tok.split_once('=') else {
            continue;
        };
        match k {
            "src" if src_orig.is_none() => {
                src_orig = Some(v.to_string());
            }
            "dst" if dst_orig.is_none() => {
                dst_orig = Some(v.to_string());
            }
            "sport" if sport_orig.is_none() => {
                sport_orig = v.parse::<u16>().ok();
            }
            "dport" if dport_orig.is_none() => {
                dport_orig = v.parse::<u16>().ok();
            }
            "packets" => {
                if let Ok(n) = v.parse::<u64>() {
                    packets_vals.push(n);
                }
            }
            "bytes" => {
                if let Ok(n) = v.parse::<u64>() {
                    bytes_vals.push(n);
                }
            }
            "mark" => {
                mark = v.parse::<u32>().unwrap_or(0);
            }
            _ => {}
        }
    }

    let src = src_orig?;
    let dst = dst_orig?;
    let inferred_family = if src.contains(':') {
        "ipv6".to_string()
    } else {
        family
    };
    if state.is_empty() {
        state = if assured {
            "ASSURED".to_string()
        } else {
            "ACTIVE".to_string()
        };
    }

    Some(ConntrackEntry {
        family: inferred_family,
        proto,
        state,
        src,
        dst,
        sport: sport_orig,
        dport: dport_orig,
        timeout_secs,
        orig_packets: packets_vals.first().copied().unwrap_or(0),
        orig_bytes: bytes_vals.first().copied().unwrap_or(0),
        reply_packets: packets_vals.get(1).copied().unwrap_or(0),
        reply_bytes: bytes_vals.get(1).copied().unwrap_or(0),
        mark,
        assured,
        offloaded,
    })
}

fn sample_dry_run_entries() -> Vec<ConntrackEntry> {
    vec![
        ConntrackEntry {
            family: "ipv4".to_string(),
            proto: "tcp".to_string(),
            state: "ESTABLISHED".to_string(),
            src: "192.168.10.102".to_string(),
            dst: "1.1.1.1".to_string(),
            sport: Some(53412),
            dport: Some(853),
            timeout_secs: 431_890,
            orig_packets: 1_420,
            orig_bytes: 218_400,
            reply_packets: 1_890,
            reply_bytes: 1_942_080,
            mark: 0,
            assured: true,
            offloaded: true,
        },
        ConntrackEntry {
            family: "ipv6".to_string(),
            proto: "udp".to_string(),
            state: "ASSURED".to_string(),
            src: "240e:3b0:1234:5670::128".to_string(),
            dst: "2606:4700:4700::1111".to_string(),
            sport: Some(58120),
            dport: Some(443),
            timeout_secs: 175,
            orig_packets: 640,
            orig_bytes: 94_120,
            reply_packets: 810,
            reply_bytes: 782_400,
            mark: 0,
            assured: true,
            offloaded: true,
        },
        ConntrackEntry {
            family: "ipv4".to_string(),
            proto: "tcp".to_string(),
            state: "TIME_WAIT".to_string(),
            src: "192.168.10.115".to_string(),
            dst: "140.82.114.4".to_string(),
            sport: Some(49102),
            dport: Some(22),
            timeout_secs: 98,
            orig_packets: 84,
            orig_bytes: 12_400,
            reply_packets: 79,
            reply_bytes: 18_920,
            mark: 0,
            assured: true,
            offloaded: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipv4_and_ipv6_nf_conntrack_lines_and_sorts_by_bytes() {
        let raw = "\
ipv4     2 tcp      6 431995 ESTABLISHED src=192.168.10.50 dst=8.8.8.8 sport=44112 dport=443 packets=15 bytes=2400 src=8.8.8.8 dst=100.64.1.2 sport=443 dport=44112 packets=20 bytes=18000 [ASSURED] mark=0 zone=0 use=2 [OFFLOAD]
ipv6     10 udp      17 170 src=240e:3b0:1234:5670::25 dst=2001:4860:4860::8888 sport=51990 dport=53 packets=50 bytes=9000 src=2001:4860:4860::8888 dst=240e:3b0:1234:5670::25 sport=53 dport=51990 packets=50 bytes=42000 [ASSURED] mark=0 use=2
";
        let list = parse_conntrack_table(raw);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].family, "ipv6");
        assert_eq!(list[0].proto, "udp");
        assert_eq!(list[0].src, "240e:3b0:1234:5670::25");
        assert_eq!(list[0].dport, Some(53));
        assert_eq!(list[0].orig_bytes, 9000);
        assert_eq!(list[0].reply_bytes, 42000);
        assert!(list[0].assured);

        assert_eq!(list[1].family, "ipv4");
        assert_eq!(list[1].state, "ESTABLISHED");
        assert!(list[1].offloaded);
    }
}
