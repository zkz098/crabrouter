use std::collections::HashMap;
use std::ffi::CString;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aya::Ebpf;
use aya::maps::lpm_trie::{Key as LpmKey, LpmTrie};
use aya::maps::{Array, PerCpuArray, PerCpuHashMap};
use aya::programs::{SchedClassifier, TcAttachType, Xdp, tc, xdp::XdpMode};
use crabrouter_model::{
    DomainPolicyRule, EbpfRuntimeStatus, EbpfTrafficStat, RouterConfig, wan_fwmark, wan_table_id,
};
use ipnet::{Ipv4Net, Ipv6Net};
use tokio::sync::Mutex;

use crate::{NetError, Result};

const EBPF_BYTECODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/crabrouter_tcx.bpf.o"));

#[derive(Debug, Default)]
struct EbpfInner {
    bpf: Option<Ebpf>,
    attached_ifaces: Vec<String>,
    xdp_attached_ifaces: Vec<String>,
    dry_run_lpm: HashMap<(Ipv4Addr, u32), u64>,
    dry_run_lpm_v6: HashMap<(Ipv6Addr, u32), u64>,
    last_error: Option<String>,
}

/// Manages `aya` eBPF `tc`/`tcx` classifiers, `DOMAIN_POLICY_LPM` trie, and `FLOW_STATS` counters.
#[derive(Clone)]
pub struct EbpfEngine {
    inner: Arc<Mutex<EbpfInner>>,
    dry_run: bool,
}

impl EbpfEngine {
    pub fn new(dry_run: bool) -> Self {
        Self {
            inner: Arc::new(Mutex::new(EbpfInner::default())),
            dry_run,
        }
    }

    /// Synchronizes eBPF program attachment against `RouterConfig.ebpf`.
    pub async fn sync_config(&self, cfg: &RouterConfig) -> Result<()> {
        let mut guard = self.inner.lock().await;
        if !cfg.ebpf.enabled {
            guard.bpf = None;
            guard.attached_ifaces.clear();
            guard.xdp_attached_ifaces.clear();
            guard.last_error = None;
            return Ok(());
        }

        let target_ifaces = resolve_attach_interfaces(cfg);
        let xdp_ifaces = if cfg.ebpf.xdp_guard_enabled {
            resolve_xdp_interfaces(cfg)
        } else {
            Vec::new()
        };

        if self.dry_run {
            guard.attached_ifaces = target_ifaces;
            guard.xdp_attached_ifaces = xdp_ifaces;
            guard.last_error = None;
            tracing::info!(
                ifaces = ?guard.attached_ifaces,
                xdp_ifaces = ?guard.xdp_attached_ifaces,
                rules = cfg.ebpf.domain_rules.len(),
                "dry-run: synchronized eBPF tc/tcx & XDP guard engine"
            );
            return Ok(());
        }

        if EBPF_BYTECODE.is_empty() {
            let msg = "embedded eBPF bytecode is empty (clang missing at build time)".to_string();
            guard.last_error = Some(msg.clone());
            return Err(NetError::Apply(msg));
        }

        if guard.bpf.is_none() {
            let loaded = Ebpf::load(EBPF_BYTECODE)
                .map_err(|e| NetError::Apply(format!("aya Ebpf::load failed: {e}")))?;
            guard.bpf = Some(loaded);
        }

        let Some(bpf) = guard.bpf.as_mut() else {
            return Ok(());
        };

        let mut attached = Vec::new();
        let mut last_err = None;
        for iface in &target_ifaces {
            if let Err(err) = attach_tc_pair(bpf, iface) {
                let msg = format!("eBPF tc attach on '{iface}' failed: {err}");
                last_err = Some(msg);
                tracing::warn!(%iface, error = %err, "failed to attach eBPF tc/tcx program");
            } else {
                attached.push(iface.clone());
            }
        }
        let xdp_attached = if cfg.ebpf.xdp_guard_enabled {
            match sync_xdp_guard(bpf, cfg, &xdp_ifaces) {
                Ok(list) => list,
                Err(err) => {
                    last_err = Some(err.to_string());
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        if let Err(err) = sync_fast_forward_cfg(bpf, cfg.ebpf.fast_forward_enabled) {
            last_err = Some(err.to_string());
        }
        guard.attached_ifaces = attached;
        guard.xdp_attached_ifaces = xdp_attached;
        guard.last_error = last_err;
        Ok(())
    }

    /// Matches a resolved domain against `cfg.ebpf.domain_rules` and inserts IPv4 `/32`
    /// and IPv6 `/128` policy entries into `DOMAIN_POLICY_LPM` / `DOMAIN_POLICY_LPM_V6`.
    pub async fn ingest_dns_resolution(
        &self,
        cfg: &RouterConfig,
        domain: &str,
        ips: &[String],
        ttl_secs: u64,
    ) -> Result<u32> {
        if !cfg.ebpf.enabled {
            return Ok(0);
        }
        let Some(rule) = match_domain_rule(&cfg.ebpf.domain_rules, domain) else {
            return Ok(0);
        };
        let fwmark = resolve_wan_fwmark(cfg, &rule.wan_name).unwrap_or(0);
        let mut inserted = 0u32;

        for raw in ips {
            let Ok(ip) = raw.trim().parse::<IpAddr>() else {
                continue;
            };
            match ip {
                IpAddr::V4(v4) => {
                    self.upsert_domain_ip(v4, 32, ttl_secs, fwmark, rule.tag_id)
                        .await?;
                }
                IpAddr::V6(v6) => {
                    self.upsert_domain_ipv6(v6, 128, ttl_secs, fwmark, rule.tag_id)
                        .await?;
                }
            }
            inserted = inserted.saturating_add(1);
        }
        Ok(inserted)
    }

    /// Inserts or updates a prefix in `DOMAIN_POLICY_LPM`.
    pub async fn upsert_domain_ip(
        &self,
        addr: Ipv4Addr,
        prefix_len: u32,
        ttl_secs: u64,
        fwmark: u32,
        tag_id: u32,
    ) -> Result<()> {
        let mut guard = self.inner.lock().await;
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expires_at_unix = if ttl_secs == 0 {
            0
        } else {
            now_unix.saturating_add(ttl_secs)
        };

        if self.dry_run {
            guard
                .dry_run_lpm
                .insert((addr, prefix_len), expires_at_unix);
            return Ok(());
        }

        let Some(bpf) = guard.bpf.as_mut() else {
            return Err(NetError::Apply("eBPF program is not loaded".to_string()));
        };
        let mut trie: LpmTrie<_, u32, [u8; 16]> = LpmTrie::try_from(
            bpf.map_mut("DOMAIN_POLICY_LPM")
                .ok_or_else(|| NetError::Apply("missing DOMAIN_POLICY_LPM map".to_string()))?,
        )
        .map_err(|e| NetError::Apply(format!("DOMAIN_POLICY_LPM open failed: {e}")))?;

        let key = LpmKey::new(prefix_len, u32::from(addr).to_be());
        let val = encode_policy_value(fwmark, tag_id, monotonic_expire_ns(ttl_secs));
        trie.insert(&key, &val, 0)
            .map_err(|e| NetError::Apply(format!("DOMAIN_POLICY_LPM insert failed: {e}")))?;
        Ok(())
    }

    /// Inserts or updates an IPv6 prefix in `DOMAIN_POLICY_LPM_V6`.
    pub async fn upsert_domain_ipv6(
        &self,
        addr: Ipv6Addr,
        prefix_len: u32,
        ttl_secs: u64,
        fwmark: u32,
        tag_id: u32,
    ) -> Result<()> {
        let mut guard = self.inner.lock().await;
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expires_at_unix = if ttl_secs == 0 {
            0
        } else {
            now_unix.saturating_add(ttl_secs)
        };

        if self.dry_run {
            guard
                .dry_run_lpm_v6
                .insert((addr, prefix_len), expires_at_unix);
            return Ok(());
        }

        let Some(bpf) = guard.bpf.as_mut() else {
            return Err(NetError::Apply("eBPF program is not loaded".to_string()));
        };
        let mut trie: LpmTrie<_, [u8; 16], [u8; 16]> = LpmTrie::try_from(
            bpf.map_mut("DOMAIN_POLICY_LPM_V6")
                .ok_or_else(|| NetError::Apply("missing DOMAIN_POLICY_LPM_V6 map".to_string()))?,
        )
        .map_err(|e| NetError::Apply(format!("DOMAIN_POLICY_LPM_V6 open failed: {e}")))?;

        let key = LpmKey::new(prefix_len, addr.octets());
        let val = encode_policy_value(fwmark, tag_id, monotonic_expire_ns(ttl_secs));
        trie.insert(&key, &val, 0)
            .map_err(|e| NetError::Apply(format!("DOMAIN_POLICY_LPM_V6 insert failed: {e}")))?;
        Ok(())
    }

    /// Collects current eBPF attachment status, LPM entry count, and per-CPU traffic stats.
    pub async fn snapshot(&self, cfg: &RouterConfig) -> EbpfRuntimeStatus {
        let mut guard = self.inner.lock().await;
        if !cfg.ebpf.enabled {
            return EbpfRuntimeStatus {
                enabled: false,
                attached_interfaces: Vec::new(),
                active_lpm_entries: 0,
                active_lpm_v6_entries: 0,
                stats: Vec::new(),
                xdp_attached_interfaces: Vec::new(),
                xdp_blocklist_drops: 0,
                xdp_syn_drops: 0,
                fast_forward_packets: 0,
                last_error: None,
            };
        }

        if self.dry_run {
            let now_unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            guard
                .dry_run_lpm
                .retain(|_, exp| *exp == 0 || *exp >= now_unix);
            guard
                .dry_run_lpm_v6
                .retain(|_, exp| *exp == 0 || *exp >= now_unix);
            return EbpfRuntimeStatus {
                enabled: true,
                attached_interfaces: if guard.attached_ifaces.is_empty() {
                    resolve_attach_interfaces(cfg)
                } else {
                    guard.attached_ifaces.clone()
                },
                active_lpm_entries: u32::try_from(guard.dry_run_lpm.len()).unwrap_or(u32::MAX),
                active_lpm_v6_entries: u32::try_from(guard.dry_run_lpm_v6.len())
                    .unwrap_or(u32::MAX),
                stats: Vec::new(),
                xdp_attached_interfaces: guard.xdp_attached_ifaces.clone(),
                xdp_blocklist_drops: 0,
                xdp_syn_drops: 0,
                fast_forward_packets: 0,
                last_error: guard.last_error.clone(),
            };
        }

        let attached_interfaces = guard.attached_ifaces.clone();
        let xdp_attached_interfaces = guard.xdp_attached_ifaces.clone();
        let last_error = guard.last_error.clone();
        let Some(bpf) = guard.bpf.as_ref() else {
            return EbpfRuntimeStatus {
                enabled: true,
                attached_interfaces,
                active_lpm_entries: 0,
                active_lpm_v6_entries: 0,
                stats: Vec::new(),
                xdp_attached_interfaces,
                xdp_blocklist_drops: 0,
                xdp_syn_drops: 0,
                fast_forward_packets: 0,
                last_error,
            };
        };

        let active_lpm_entries = count_lpm_entries(bpf);
        let active_lpm_v6_entries = count_lpm_v6_entries(bpf);
        let stats = read_flow_stats(bpf, cfg);
        let (xdp_blocklist_drops, xdp_syn_drops) = read_xdp_drop_stats(bpf);
        let fast_forward_packets = read_fast_forward_stats(bpf);

        EbpfRuntimeStatus {
            enabled: true,
            attached_interfaces,
            active_lpm_entries,
            active_lpm_v6_entries,
            stats,
            xdp_attached_interfaces,
            xdp_blocklist_drops,
            xdp_syn_drops,
            fast_forward_packets,
            last_error,
        }
    }
}

fn attach_tc_pair(bpf: &mut Ebpf, iface: &str) -> Result<()> {
    let iface_c = CString::new(iface)
        .map_err(|e| NetError::Apply(format!("invalid interface name '{iface}': {e}")))?;
    let _ = tc::qdisc_add_clsact(iface_c.as_c_str());

    for (prog_name, dir) in [
        ("crabrouter_tcx_ingress", TcAttachType::Ingress),
        ("crabrouter_tcx_egress", TcAttachType::Egress),
    ] {
        let prog: &mut SchedClassifier = bpf
            .program_mut(prog_name)
            .ok_or_else(|| NetError::Apply(format!("missing eBPF program '{prog_name}'")))?
            .try_into()
            .map_err(|e| NetError::Apply(format!("program '{prog_name}' type mismatch: {e}")))?;
        let _ = prog.load();
        prog.attach(iface_c.as_c_str(), dir)
            .map_err(|e| NetError::Apply(format!("attach '{prog_name}' on '{iface}': {e}")))?;
    }
    Ok(())
}

fn count_lpm_entries(bpf: &Ebpf) -> u32 {
    let Some(map) = bpf.map("DOMAIN_POLICY_LPM") else {
        return 0;
    };
    let Ok(trie) = LpmTrie::<_, u32, [u8; 16]>::try_from(map) else {
        return 0;
    };
    u32::try_from(trie.keys().count()).unwrap_or(u32::MAX)
}

fn count_lpm_v6_entries(bpf: &Ebpf) -> u32 {
    let Some(map) = bpf.map("DOMAIN_POLICY_LPM_V6") else {
        return 0;
    };
    let Ok(trie) = LpmTrie::<_, [u8; 16], [u8; 16]>::try_from(map) else {
        return 0;
    };
    u32::try_from(trie.keys().count()).unwrap_or(u32::MAX)
}

fn read_flow_stats(bpf: &Ebpf, cfg: &RouterConfig) -> Vec<EbpfTrafficStat> {
    let mut out = Vec::new();
    collect_flow_stats_v4(bpf, cfg, &mut out);
    collect_flow_stats_v6(bpf, cfg, &mut out);
    out.sort_by(|a, b| {
        let b_tot = b.rx_bytes.saturating_add(b.tx_bytes);
        let a_tot = a.rx_bytes.saturating_add(a.tx_bytes);
        b_tot.cmp(&a_tot)
    });
    out
}

fn collect_flow_stats_v4(bpf: &Ebpf, cfg: &RouterConfig, out: &mut Vec<EbpfTrafficStat>) {
    let Some(map) = bpf.map("FLOW_STATS") else {
        return;
    };
    let Ok(stats_map) = PerCpuHashMap::<_, [u8; 8], [u8; 32]>::try_from(map) else {
        return;
    };
    for item in &stats_map {
        let Ok((key_bytes, per_cpu_vals)) = item else {
            continue;
        };
        let ip_be = u32::from_ne_bytes([key_bytes[0], key_bytes[1], key_bytes[2], key_bytes[3]]);
        let client_ip = Ipv4Addr::from(u32::from_be(ip_be)).to_string();
        let tag_id = u32::from_le_bytes([key_bytes[4], key_bytes[5], key_bytes[6], key_bytes[7]]);
        push_flow_stat_entry(cfg, client_ip, tag_id, &per_cpu_vals, out);
    }
}

fn collect_flow_stats_v6(bpf: &Ebpf, cfg: &RouterConfig, out: &mut Vec<EbpfTrafficStat>) {
    let Some(map) = bpf.map("FLOW_STATS_V6") else {
        return;
    };
    let Ok(stats_map) = PerCpuHashMap::<_, [u8; 20], [u8; 32]>::try_from(map) else {
        return;
    };
    for item in &stats_map {
        let Ok((key_bytes, per_cpu_vals)) = item else {
            continue;
        };
        let mut octets = [0u8; 16];
        octets.copy_from_slice(&key_bytes[0..16]);
        let client_ip = Ipv6Addr::from(octets).to_string();
        let tag_id =
            u32::from_le_bytes([key_bytes[16], key_bytes[17], key_bytes[18], key_bytes[19]]);
        push_flow_stat_entry(cfg, client_ip, tag_id, &per_cpu_vals, out);
    }
}

fn push_flow_stat_entry(
    cfg: &RouterConfig,
    client_ip: String,
    tag_id: u32,
    per_cpu_vals: &aya::maps::PerCpuValues<[u8; 32]>,
    out: &mut Vec<EbpfTrafficStat>,
) {
    let mut rx_packets = 0u64;
    let mut rx_bytes = 0u64;
    let mut tx_packets = 0u64;
    let mut tx_bytes = 0u64;

    for val in per_cpu_vals.iter() {
        let (rp, rb, tp, tb) = decode_traffic_counters(val);
        rx_packets = rx_packets.saturating_add(rp);
        rx_bytes = rx_bytes.saturating_add(rb);
        tx_packets = tx_packets.saturating_add(tp);
        tx_bytes = tx_bytes.saturating_add(tb);
    }

    let rule = cfg.ebpf.domain_rules.iter().find(|r| r.tag_id == tag_id);
    out.push(EbpfTrafficStat {
        client_ip,
        tag_id,
        domain_label: rule.map(|r| r.domain.clone()),
        wan_name: rule.map(|r| r.wan_name.clone()),
        tx_packets,
        tx_bytes,
        rx_packets,
        rx_bytes,
    });
}

pub fn resolve_attach_interfaces(cfg: &RouterConfig) -> Vec<String> {
    if !cfg.ebpf.attach_interfaces.is_empty() {
        return cfg.ebpf.attach_interfaces.clone();
    }
    cfg.lans.iter().map(|l| l.bridge.clone()).collect()
}

pub fn resolve_xdp_interfaces(cfg: &RouterConfig) -> Vec<String> {
    if !cfg.ebpf.xdp_interfaces.is_empty() {
        return cfg.ebpf.xdp_interfaces.clone();
    }
    cfg.wans.iter().map(|w| w.device.clone()).collect()
}

fn sync_xdp_guard(bpf: &mut Ebpf, cfg: &RouterConfig, ifaces: &[String]) -> Result<Vec<String>> {
    if let Some(map) = bpf.map_mut("XDP_SYN_LIMIT_CFG") {
        let mut arr = Array::<_, u32>::try_from(map)
            .map_err(|e| NetError::Apply(format!("XDP_SYN_LIMIT_CFG open failed: {e}")))?;
        arr.set(0, &cfg.ebpf.xdp_syn_rate_limit, 0)
            .map_err(|e| NetError::Apply(format!("XDP_SYN_LIMIT_CFG set failed: {e}")))?;
    }

    if let Some(map) = bpf.map_mut("XDP_BLOCKLIST_LPM") {
        let mut trie = LpmTrie::<_, u32, u64>::try_from(map)
            .map_err(|e| NetError::Apply(format!("XDP_BLOCKLIST_LPM open failed: {e}")))?;
        let existing_keys: Vec<LpmKey<u32>> = trie.keys().flatten().collect();
        for k in existing_keys {
            let _ = trie.remove(&k);
        }
        for raw in &cfg.ebpf.xdp_blocklist {
            let trimmed = raw.trim();
            if let Ok(net) = trimmed.parse::<Ipv4Net>() {
                let key = LpmKey::new(u32::from(net.prefix_len()), u32::from(net.addr()).to_be());
                let _ = trie.insert(&key, &0u64, 0);
            } else if let Ok(ip) = trimmed.parse::<Ipv4Addr>() {
                let key = LpmKey::new(32, u32::from(ip).to_be());
                let _ = trie.insert(&key, &0u64, 0);
            }
        }
    }

    if let Some(map) = bpf.map_mut("XDP_BLOCKLIST_LPM_V6") {
        let mut trie = LpmTrie::<_, [u8; 16], u64>::try_from(map)
            .map_err(|e| NetError::Apply(format!("XDP_BLOCKLIST_LPM_V6 open failed: {e}")))?;
        let existing_keys: Vec<LpmKey<[u8; 16]>> = trie.keys().flatten().collect();
        for k in existing_keys {
            let _ = trie.remove(&k);
        }
        for raw in &cfg.ebpf.xdp_blocklist {
            let trimmed = raw.trim();
            if let Ok(net) = trimmed.parse::<Ipv6Net>() {
                let key = LpmKey::new(u32::from(net.prefix_len()), net.addr().octets());
                let _ = trie.insert(&key, &0u64, 0);
            } else if let Ok(ip) = trimmed.parse::<Ipv6Addr>() {
                let key = LpmKey::new(128, ip.octets());
                let _ = trie.insert(&key, &0u64, 0);
            }
        }
    }

    let xdp: &mut Xdp = bpf
        .program_mut("crabrouter_xdp_wan_guard")
        .ok_or_else(|| {
            NetError::Apply("missing XDP program 'crabrouter_xdp_wan_guard'".to_string())
        })?
        .try_into()
        .map_err(|e| NetError::Apply(format!("XDP program type mismatch: {e}")))?;
    let _ = xdp.load();

    let mut attached = Vec::new();
    for iface in ifaces {
        let Ok(iface_c) = CString::new(iface.as_str()) else {
            continue;
        };
        let res = xdp
            .attach(iface_c.as_c_str(), XdpMode::Default)
            .or_else(|_| xdp.attach(iface_c.as_c_str(), XdpMode::Skb));
        match res {
            Ok(_) => attached.push(iface.clone()),
            Err(err) => {
                tracing::warn!(%iface, error = %err, "failed to attach XDP WAN guard");
            }
        }
    }
    Ok(attached)
}

fn sync_fast_forward_cfg(bpf: &mut Ebpf, enabled: bool) -> Result<()> {
    if let Some(map) = bpf.map_mut("FAST_FWD_CFG") {
        let mut arr = Array::<_, u32>::try_from(map)
            .map_err(|e| NetError::Apply(format!("FAST_FWD_CFG open failed: {e}")))?;
        arr.set(0, &u32::from(enabled), 0)
            .map_err(|e| NetError::Apply(format!("FAST_FWD_CFG set failed: {e}")))?;
    }
    Ok(())
}

fn read_fast_forward_stats(bpf: &Ebpf) -> u64 {
    let Some(map) = bpf.map("FAST_FWD_STATS") else {
        return 0;
    };
    let Ok(arr) = PerCpuArray::<_, u64>::try_from(map) else {
        return 0;
    };
    arr.get(&0, 0).map_or(0, |vals| {
        vals.iter().copied().fold(0u64, u64::saturating_add)
    })
}

fn read_xdp_drop_stats(bpf: &Ebpf) -> (u64, u64) {
    let Some(map) = bpf.map("XDP_DROP_STATS") else {
        return (0, 0);
    };
    let Ok(arr) = PerCpuArray::<_, u64>::try_from(map) else {
        return (0, 0);
    };
    let sum_idx = |idx: u32| -> u64 {
        arr.get(&idx, 0).map_or(0, |vals| {
            vals.iter().copied().fold(0u64, u64::saturating_add)
        })
    };
    (sum_idx(0), sum_idx(1))
}

pub fn match_domain_rule<'a>(
    rules: &'a [DomainPolicyRule],
    domain: &str,
) -> Option<&'a DomainPolicyRule> {
    let normalized = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    rules.iter().find(|r| {
        let pat = r.domain.trim().trim_end_matches('.').to_ascii_lowercase();
        if let Some(suffix) = pat.strip_prefix("*.") {
            normalized == suffix || normalized.ends_with(&format!(".{suffix}"))
        } else {
            normalized == pat
        }
    })
}

fn resolve_wan_fwmark(cfg: &RouterConfig, wan_name: &str) -> Option<u32> {
    cfg.wans
        .iter()
        .enumerate()
        .find(|(_, w)| w.name == wan_name)
        .map(|(idx, w)| wan_fwmark(wan_table_id(idx, w)))
}

fn encode_policy_value(fwmark: u32, tag_id: u32, expires_at_ns: u64) -> [u8; 16] {
    let mut buf = [0u8; 16];
    buf[0..4].copy_from_slice(&fwmark.to_le_bytes());
    buf[4..8].copy_from_slice(&tag_id.to_le_bytes());
    buf[8..16].copy_from_slice(&expires_at_ns.to_le_bytes());
    buf
}

fn decode_traffic_counters(buf: &[u8; 32]) -> (u64, u64, u64, u64) {
    let rp = u64::from_le_bytes([
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
    ]);
    let rb = u64::from_le_bytes([
        buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
    ]);
    let tp = u64::from_le_bytes([
        buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
    ]);
    let tb = u64::from_le_bytes([
        buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
    ]);
    (rp, rb, tp, tb)
}

fn monotonic_expire_ns(ttl_secs: u64) -> u64 {
    if ttl_secs == 0 {
        return 0;
    }
    let mut ts = std::time::Duration::ZERO;
    if let Ok(d) = std::fs::read_to_string("/proc/uptime")
        && let Some(first) = d.split_whitespace().next()
        && let Ok(secs) = first.parse::<f64>()
    {
        ts = Duration::from_secs_f64(secs.max(0.0));
    }
    u64::try_from(ts.as_nanos())
        .unwrap_or(u64::MAX)
        .saturating_add(ttl_secs.saturating_mul(1_000_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_and_wildcard_domain_rules() {
        let rules = vec![
            DomainPolicyRule {
                domain: "*.openai.com".to_string(),
                wan_name: "wan2".to_string(),
                tag_id: 1,
            },
            DomainPolicyRule {
                domain: "example.org".to_string(),
                wan_name: "wan".to_string(),
                tag_id: 2,
            },
        ];

        assert_eq!(
            match_domain_rule(&rules, "api.openai.com.").map(|r| r.tag_id),
            Some(1)
        );
        assert_eq!(
            match_domain_rule(&rules, "openai.com").map(|r| r.tag_id),
            Some(1)
        );
        assert_eq!(
            match_domain_rule(&rules, "EXAMPLE.ORG").map(|r| r.tag_id),
            Some(2)
        );
        assert!(match_domain_rule(&rules, "notopenai.com").is_none());
    }

    #[tokio::test]
    async fn dry_run_ingests_dns_resolution_into_lpm_mirror() {
        let mut cfg = RouterConfig::default();
        cfg.ebpf.enabled = true;
        cfg.ebpf.domain_rules.push(DomainPolicyRule {
            domain: "*.example.com".to_string(),
            wan_name: "wan".to_string(),
            tag_id: 7,
        });

        let engine = EbpfEngine::new(true);
        engine.sync_config(&cfg).await.expect("sync ok");
        let inserted = engine
            .ingest_dns_resolution(
                &cfg,
                "cdn.example.com",
                &[
                    "93.184.216.34".to_string(),
                    "2606:2800:220:1::248".to_string(),
                ],
                300,
            )
            .await
            .expect("ingest ok");
        assert_eq!(inserted, 2);

        let snap = engine.snapshot(&cfg).await;
        assert!(snap.enabled);
        assert_eq!(snap.active_lpm_entries, 1);
        assert_eq!(snap.active_lpm_v6_entries, 1);
        assert_eq!(snap.attached_interfaces, vec!["br-lan"]);

        cfg.ebpf.xdp_guard_enabled = true;
        cfg.ebpf.fast_forward_enabled = true;
        cfg.ebpf.xdp_blocklist = vec!["198.51.100.0/24".to_string(), "2001:db8::/32".to_string()];
        engine.sync_config(&cfg).await.expect("xdp sync ok");
        let xdp_snap = engine.snapshot(&cfg).await;
        assert_eq!(xdp_snap.xdp_attached_interfaces, vec!["eth0"]);
    }
}
