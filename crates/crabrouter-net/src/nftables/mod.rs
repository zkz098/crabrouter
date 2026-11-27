mod extensions;
#[cfg(test)]
mod tests;

use extensions::{
    write_crowdsec_drop_rules, write_crowdsec_sets, write_iptv_forward_rules,
    write_iptv_input_rules, write_mdns_input_rules,
};

use std::collections::HashSet;
use std::fmt::Write;
use std::net::IpAddr;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use crabrouter_model::{
    FirewallAction, FirewallRule, FirewallZone, Ipv6LanMode, MultiWanMode, RouterConfig,
    TransportProto, WanProto, wan_fwmark, wan_table_id,
};
use ipnet::IpNet;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::{NetError, Result};

/// Compiles declarative `RouterConfig` into an atomic `nftables` ruleset suitable for `nft -f -`.
///
/// This deterministic rendering includes all configured flowtable devices and is used for previews,
/// CLI output, and unit tests. For live kernel application, [`apply_nft_with_precheck`] filters
/// flowtable devices against active host interfaces and falls back gracefully if needed.
pub fn render_nftables_ruleset(cfg: &RouterConfig) -> String {
    render_nftables_ruleset_with_options::<std::collections::hash_map::RandomState>(
        cfg,
        None,
        cfg.firewall.flowtable_offload,
    )
}

/// Renders an `nftables` ruleset with optional live-interface filtering for `flowtable`.
pub fn render_nftables_ruleset_with_options<S: std::hash::BuildHasher>(
    cfg: &RouterConfig,
    live_ifaces: Option<&HashSet<String, S>>,
    enable_flowtable: bool,
) -> String {
    render_nftables_ruleset_with_multi_wan(cfg, live_ifaces, enable_flowtable, None)
}

/// Renders an `nftables` ruleset with optional live-interface filtering and explicit active
/// Multi-WAN list (used by the health-probe failover engine).
pub fn render_nftables_ruleset_with_multi_wan<S: std::hash::BuildHasher>(
    cfg: &RouterConfig,
    live_ifaces: Option<&HashSet<String, S>>,
    enable_flowtable: bool,
    active_wans: Option<&[String]>,
) -> String {
    let mut out = String::from(
        "#!/usr/sbin/nft -f\n\
         table inet crabrouter\n\
         delete table inet crabrouter\n\n\
         table inet crabrouter {\n",
    );

    let flowtable_devices = if enable_flowtable && cfg.firewall.flowtable_offload {
        let candidates = collect_flowtable_devices(cfg);
        match live_ifaces {
            Some(live) => candidates
                .into_iter()
                .filter(|d| live.contains(d))
                .collect::<Vec<_>>(),
            None => candidates,
        }
    } else {
        Vec::new()
    };

    if !flowtable_devices.is_empty() {
        let joined = flowtable_devices
            .iter()
            .map(|d| format!("\"{d}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "    flowtable f_fastpath {{\n        hook ingress priority 0; devices = {{ {joined} }};\n    }}\n"
        );
    }

    let has_flowtable = !flowtable_devices.is_empty();
    write_crowdsec_sets(&mut out, cfg);
    write_input_chain(&mut out, cfg);
    write_forward_chain(&mut out, cfg, has_flowtable);
    write_mangle_prerouting_chain(&mut out, cfg, active_wans);
    write_prerouting_chain(&mut out, cfg);
    write_postrouting_chain(&mut out, cfg);

    out.push_str("}\n");
    out
}

/// Runs `nft -c -f -` pre-check without mutating kernel state.
pub async fn check_nft_syntax(ruleset: &str) -> Result<()> {
    run_nft_stdin(&["-c", "-f", "-"], ruleset).await
}

/// Validates with `nft -c -f -` (falling back safely if dynamic flowtable interfaces are absent)
/// and applies the ruleset atomically via `nft -f -`.
pub async fn apply_nft_with_precheck(cfg: &RouterConfig) -> Result<()> {
    apply_nft_with_precheck_and_multi_wan(cfg, None).await
}

static NFT_APPLY_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

/// Validates and applies `nftables` ruleset with explicit active Multi-WAN list.
pub async fn apply_nft_with_precheck_and_multi_wan(
    cfg: &RouterConfig,
    active_wans: Option<&[String]>,
) -> Result<()> {
    let _guard = NFT_APPLY_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let live_ifaces = read_live_iface_names();
    let candidate = render_nftables_ruleset_with_multi_wan(
        cfg,
        Some(&live_ifaces),
        cfg.firewall.flowtable_offload,
        active_wans,
    );

    let final_ruleset = match check_nft_syntax(&candidate).await {
        Ok(()) => candidate,
        Err(first_err) if cfg.firewall.flowtable_offload => {
            let fallback =
                render_nftables_ruleset_with_multi_wan(cfg, Some(&live_ifaces), false, active_wans);
            if check_nft_syntax(&fallback).await.is_ok() {
                tracing::warn!(
                    error = %first_err,
                    "nft flowtable check failed on current host; falling back without flowtable offload"
                );
                fallback
            } else {
                return Err(first_err);
            }
        }
        Err(err) => return Err(err),
    };

    run_nft_stdin(&["-f", "-"], &final_ruleset).await
}

async fn run_nft_stdin(args: &[&str], ruleset: &str) -> Result<()> {
    let mut cmd = Command::new("nft");
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(ruleset.as_bytes()).await?;
    }

    let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .map_err(|_| NetError::Apply(format!("nft {} timed out after 15s", args.join(" "))))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let mode = args.join(" ");
        return Err(NetError::Apply(format!("nft {mode} failed: {stderr}")));
    }
    Ok(())
}

fn read_live_iface_names() -> HashSet<String> {
    let mut set = HashSet::new();
    let Ok(entries) = std::fs::read_dir(Path::new("/sys/class/net")) else {
        return set;
    };
    for entry in entries.flatten() {
        set.insert(entry.file_name().to_string_lossy().to_string());
    }
    set
}

fn collect_flowtable_devices(cfg: &RouterConfig) -> Vec<String> {
    let mut devs = Vec::new();
    for wan in &cfg.wans {
        if !devs.contains(&wan.device) {
            devs.push(wan.device.clone());
        }
    }
    for lan in &cfg.lans {
        if !devs.contains(&lan.bridge) {
            devs.push(lan.bridge.clone());
        }
    }
    devs
}

fn write_input_chain(out: &mut String, cfg: &RouterConfig) {
    out.push_str(
        "    chain input {\n\
         \x20       type filter hook input priority filter; policy drop;\n\
         \x20       iifname \"lo\" accept\n\
         \x20       ct state established,related accept\n\
         \x20       ct state invalid drop\n\
         \x20       meta l4proto { icmp, ipv6-icmp } accept\n\
         \x20       ip6 daddr fe80::/10 udp sport 547 udp dport 546 accept comment \"dhcpv6-client\"\n",
    );
    write_syn_flood_guard(out, cfg, "input");
    write_crowdsec_drop_rules(out, cfg);
    write_mdns_input_rules(out, cfg);
    write_iptv_input_rules(out, cfg);
    if cfg.upnp.enabled && !cfg.lans.is_empty() {
        let lan_ifaces = upnp_lan_ifaces(cfg);
        if !lan_ifaces.is_empty() {
            let lan_set = format_iface_set(&lan_ifaces);
            let _ = writeln!(
                out,
                "        iifname {lan_set} udp dport {{ 1900, 5351 }} accept comment \"upnp-natpmp-pcp\""
            );
        }
    }
    write_container_input_rules(out, cfg);
    for rule in cfg
        .firewall
        .rules
        .iter()
        .filter(|r| r.enabled && r.dst_zone.is_none())
    {
        write_acl_rule(out, cfg, rule);
    }
    for zone in &cfg.firewall.zones {
        write_zone_verdict(out, zone, "iifname", zone.input);
    }
    out.push_str("    }\n\n");
}

fn write_forward_chain(out: &mut String, cfg: &RouterConfig, has_flowtable: bool) {
    out.push_str(
        "    chain forward {\n\
         \x20       type filter hook forward priority filter; policy drop;\n",
    );
    if has_flowtable {
        out.push_str("        ip protocol { tcp, udp } flow add @f_fastpath\n");
    }
    if cfg.firewall.mss_clamping {
        out.push_str("        tcp flags syn tcp option maxseg size set rt mtu\n");
    }
    out.push_str(
        "        ct state established,related accept\n\
         \x20       ct state invalid drop\n",
    );
    write_syn_flood_guard(out, cfg, "forward");
    write_crowdsec_drop_rules(out, cfg);
    write_iptv_forward_rules(out, cfg);
    if cfg.lans.iter().any(|l| l.ipv6_mode == Ipv6LanMode::Relay) {
        out.push_str("        meta l4proto ipv6-icmp accept comment \"ipv6-relay-ndp\"\n");
    }
    write_container_forward_rules(out, cfg);
    for rule in cfg
        .firewall
        .rules
        .iter()
        .filter(|r| r.enabled && r.dst_zone.is_some())
    {
        write_acl_rule(out, cfg, rule);
    }
    for zone in &cfg.firewall.zones {
        write_zone_verdict(out, zone, "iifname", zone.forward);
    }
    out.push_str("        ct status dnat accept\n");
    out.push_str("    }\n\n");
}

fn write_container_input_rules(out: &mut String, cfg: &RouterConfig) {
    if !cfg.containers.enabled {
        return;
    }
    let ifaces = cfg.containers.effective_bridge_interfaces();
    if ifaces.is_empty() {
        return;
    }
    let iface_set = format_iface_set(&ifaces);
    let mut ports = vec![cfg.dns.crabdns_port];
    if !ports.contains(&cfg.containers.dns_bind_port) {
        ports.push(cfg.containers.dns_bind_port);
    }
    let ports_expr = format_port_set(&ports);
    let _ = writeln!(
        out,
        "        iifname {iface_set} meta l4proto {{ tcp, udp }} th dport {ports_expr} accept comment \"container-dns\""
    );
}

fn write_container_forward_rules(out: &mut String, cfg: &RouterConfig) {
    if !cfg.containers.enabled {
        return;
    }
    let ifaces = cfg.containers.effective_bridge_interfaces();
    if ifaces.is_empty() {
        return;
    }
    let iface_set = format_iface_set(&ifaces);
    let _ = writeln!(
        out,
        "        iifname {iface_set} oifname {iface_set} accept comment \"container-intra\""
    );

    if cfg.containers.allow_wan_access {
        let wan_ifaces = collect_wan_ifaces(cfg);
        if !wan_ifaces.is_empty() {
            let wan_set = format_iface_set(&wan_ifaces);
            let _ = writeln!(
                out,
                "        iifname {iface_set} oifname {wan_set} accept comment \"container-to-wan\""
            );
        }
    }

    if cfg.containers.allow_lan_access {
        let lan_ifaces: Vec<String> = cfg.lans.iter().map(|l| l.bridge.clone()).collect();
        if !lan_ifaces.is_empty() {
            let lan_set = format_iface_set(&lan_ifaces);
            let _ = writeln!(
                out,
                "        iifname {iface_set} oifname {lan_set} accept comment \"container-to-lan\""
            );
        }
    }
}

fn collect_wan_ifaces(cfg: &RouterConfig) -> Vec<String> {
    if let Some(wan_zone) = cfg.firewall.zones.iter().find(|z| z.name == "wan")
        && !wan_zone.interfaces.is_empty()
    {
        return wan_zone.interfaces.clone();
    }
    let mut out = Vec::new();
    let mut has_pppoe = false;
    for wan in &cfg.wans {
        if !out.contains(&wan.device) {
            out.push(wan.device.clone());
        }
        if wan.proto == WanProto::Pppoe {
            has_pppoe = true;
        }
    }
    if has_pppoe && !out.iter().any(|i| i == "ppp0") {
        out.push("ppp0".to_string());
    }
    out
}

fn write_syn_flood_guard(out: &mut String, cfg: &RouterConfig, chain_name: &str) {
    if !cfg.firewall.syn_flood_protect {
        return;
    }
    let rate = cfg.firewall.syn_flood_rate.max(1);
    let burst = rate.saturating_mul(2).max(10);
    let _ = writeln!(
        out,
        "        tcp flags & (fin | syn | rst | ack) == syn meter syn_flood_{chain_name} {{ ip saddr limit rate over {rate}/second burst {burst} packets }} drop"
    );
    let _ = writeln!(
        out,
        "        tcp flags & (fin | syn | rst | ack) == syn meter syn_flood6_{chain_name} {{ ip6 saddr limit rate over {rate}/second burst {burst} packets }} drop"
    );
}

fn write_acl_rule(out: &mut String, cfg: &RouterConfig, rule: &FirewallRule) {
    let Some(src_zone) = cfg.firewall.zones.iter().find(|z| z.name == rule.src_zone) else {
        return;
    };
    if src_zone.interfaces.is_empty() {
        return;
    }

    let mut clauses = vec![format!(
        "iifname {}",
        format_iface_set(&src_zone.interfaces)
    )];

    if let Some(dst_name) = rule.dst_zone.as_deref() {
        let Some(dst_zone) = cfg.firewall.zones.iter().find(|z| z.name == dst_name) else {
            return;
        };
        if dst_zone.interfaces.is_empty() {
            return;
        }
        clauses.push(format!(
            "oifname {}",
            format_iface_set(&dst_zone.interfaces)
        ));
    }

    if let Some(src_expr) = format_ip_match(rule.src_ip.as_deref(), "saddr") {
        clauses.push(src_expr);
    }
    if let Some(dst_expr) = format_ip_match(rule.dst_ip.as_deref(), "daddr") {
        clauses.push(dst_expr);
    }

    let port = rule
        .dst_port
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match (rule.proto, port) {
        (TransportProto::Tcp, Some(p)) => clauses.push(format!("tcp dport {p}")),
        (TransportProto::Tcp, None) => clauses.push("meta l4proto tcp".to_string()),
        (TransportProto::Udp, Some(p)) => clauses.push(format!("udp dport {p}")),
        (TransportProto::Udp, None) => clauses.push("meta l4proto udp".to_string()),
        (TransportProto::TcpUdp, Some(p)) => {
            clauses.push(format!("meta l4proto {{ tcp, udp }} th dport {p}"));
        }
        (TransportProto::TcpUdp, None) => clauses.push("meta l4proto { tcp, udp }".to_string()),
        (TransportProto::Icmp, _) => clauses.push("meta l4proto { icmp, ipv6-icmp }".to_string()),
        (TransportProto::Any, Some(p)) => clauses.push(format!("th dport {p}")),
        (TransportProto::Any, None) => {}
    }

    let verdict = action_keyword(rule.action);
    let _ = writeln!(
        out,
        "        {} {verdict} comment \"acl:{}\"",
        clauses.join(" "),
        rule.name
    );
}

fn format_ip_match(raw: Option<&str>, dir: &str) -> Option<String> {
    let val = raw.map(str::trim).filter(|s| !s.is_empty())?;
    if let Ok(net) = val.parse::<IpNet>() {
        return Some(match net {
            IpNet::V4(_) => format!("ip {dir} {net}"),
            IpNet::V6(_) => format!("ip6 {dir} {net}"),
        });
    }
    if let Ok(addr) = val.parse::<IpAddr>() {
        return Some(match addr {
            IpAddr::V4(_) => format!("ip {dir} {addr}"),
            IpAddr::V6(_) => format!("ip6 {dir} {addr}"),
        });
    }
    None
}

fn write_mangle_prerouting_chain(
    out: &mut String,
    cfg: &RouterConfig,
    active_wans: Option<&[String]>,
) {
    if !cfg.multi_wan.enabled || cfg.wans.is_empty() {
        return;
    }

    let candidates: Vec<(u32, u32)> = cfg
        .wans
        .iter()
        .enumerate()
        .filter(|(_, w)| {
            active_wans.is_none_or(|list| list.is_empty() || list.iter().any(|n| n == &w.name))
        })
        .map(|(idx, w)| (wan_fwmark(wan_table_id(idx, w)), w.weight.max(1)))
        .collect();

    if candidates.is_empty() || cfg.lans.is_empty() {
        return;
    }

    let mut mangle_ifaces: Vec<String> = cfg.lans.iter().map(|l| l.bridge.clone()).collect();
    if cfg.containers.enabled && cfg.containers.multi_wan_enabled {
        for iface in cfg.containers.effective_bridge_interfaces() {
            if !mangle_ifaces.contains(&iface) {
                mangle_ifaces.push(iface);
            }
        }
    }
    let iface_set = format_iface_set(&mangle_ifaces);

    out.push_str(
        "    chain mangle_prerouting {\n\
         \x20       type filter hook prerouting priority mangle; policy accept;\n",
    );
    let _ = writeln!(out, "        iifname != {iface_set} return");
    out.push_str("        fib daddr type local return\n");
    for lan in &cfg.lans {
        if let Ok(net @ IpNet::V4(_)) = lan.cidr.parse::<IpNet>() {
            let _ = writeln!(out, "        ip daddr {net} return");
        }
    }
    if cfg.containers.enabled
        && let Ok(net @ IpNet::V4(_)) = cfg.containers.default_subnet.parse::<IpNet>()
    {
        let _ = writeln!(out, "        ip daddr {net} return");
    }
    out.push_str("        ct mark != 0 meta mark set ct mark return\n");

    if cfg.multi_wan.mode == MultiWanMode::Failover || candidates.len() == 1 {
        let mark = candidates[0].0;
        let _ = writeln!(
            out,
            "        ct state new meta mark set 0x{mark:x} ct mark set meta mark"
        );
    } else {
        let slots = build_weighted_mark_slots(&candidates);
        let total = slots.len();
        let map_entries = slots
            .iter()
            .enumerate()
            .map(|(i, m)| format!("{i} : 0x{m:x}"))
            .collect::<Vec<_>>()
            .join(", ");
        let selector = if cfg.multi_wan.sticky_sessions {
            format!("jhash ip saddr mod {total}")
        } else {
            format!("numgen random mod {total}")
        };
        let _ = writeln!(
            out,
            "        ct state new meta mark set {selector} map {{ {map_entries} }}"
        );
        out.push_str("        ct state new ct mark set meta mark\n");
    }

    out.push_str("    }\n\n");
}

fn build_weighted_mark_slots(candidates: &[(u32, u32)]) -> Vec<u32> {
    let mut g = 0u32;
    for &(_, w) in candidates {
        g = gcd_u32(g, w);
    }
    let g = g.max(1);
    let mut scaled: Vec<(u32, u32)> = candidates
        .iter()
        .map(|&(m, w)| (m, (w / g).max(1)))
        .collect();
    let sum: u32 = scaled.iter().map(|(_, w)| *w).sum();
    if sum > 32 {
        for (_, w) in &mut scaled {
            *w = ((*w * 32) / sum).max(1);
        }
    }
    let mut slots = Vec::new();
    for (mark, count) in scaled {
        for _ in 0..count {
            slots.push(mark);
        }
    }
    slots
}

const fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn write_prerouting_chain(out: &mut String, cfg: &RouterConfig) {
    out.push_str(
        "    chain prerouting {\n\
         \x20       type nat hook prerouting priority dstnat; policy accept;\n",
    );
    let (lan_ifaces, lan_router_ips, _) = collect_lan_v4_info(cfg);
    for rule in &cfg.firewall.port_forwards {
        if !rule.enabled {
            continue;
        }
        let Some(zone) = cfg.firewall.zones.iter().find(|z| z.name == rule.src_zone) else {
            continue;
        };
        if zone.interfaces.is_empty() {
            continue;
        }
        let ifaces = format_iface_set(&zone.interfaces);
        let protos = port_forward_protos(rule.proto);
        for proto in &protos {
            let _ = writeln!(
                out,
                "        iifname {ifaces} {proto} dport {} dnat to {}:{} comment \"{}\"",
                rule.external_port, rule.dest_ip, rule.internal_port, rule.name
            );
        }
        if cfg.firewall.nat_loopback
            && rule.reflection
            && !lan_ifaces.is_empty()
            && !lan_router_ips.is_empty()
        {
            let lan_set = format_iface_set(&lan_ifaces);
            let router_ip_set = format_addr_set(&lan_router_ips);
            for proto in &protos {
                let _ = writeln!(
                    out,
                    "        iifname {lan_set} ip daddr != {router_ip_set} fib daddr type local {proto} dport {} dnat to {}:{} comment \"hairpin:{}\"",
                    rule.external_port, rule.dest_ip, rule.internal_port, rule.name
                );
            }
        }
    }
    out.push_str("    }\n\n");
}

fn write_postrouting_chain(out: &mut String, cfg: &RouterConfig) {
    out.push_str(
        "    chain postrouting {\n\
         \x20       type nat hook postrouting priority srcnat; policy accept;\n",
    );
    for zone in &cfg.firewall.zones {
        if zone.masquerade && !zone.interfaces.is_empty() {
            let ifaces = format_iface_set(&zone.interfaces);
            let _ = writeln!(out, "        meta nfproto ipv4 oifname {ifaces} masquerade");
        }
    }
    write_hairpin_postrouting(out, cfg);
    out.push_str("    }\n");
}

fn write_hairpin_postrouting(out: &mut String, cfg: &RouterConfig) {
    if !cfg.firewall.nat_loopback {
        return;
    }
    let (lan_ifaces, _, lan_nets) = collect_lan_v4_info(cfg);
    if lan_ifaces.is_empty() || lan_nets.is_empty() {
        return;
    }
    let lan_set = format_iface_set(&lan_ifaces);
    let net_set = format_addr_set(&lan_nets);
    for rule in &cfg.firewall.port_forwards {
        if !rule.enabled || !rule.reflection {
            continue;
        }
        for proto in port_forward_protos(rule.proto) {
            let _ = writeln!(
                out,
                "        meta nfproto ipv4 ct status dnat oifname {lan_set} ip saddr {net_set} ip daddr {} {proto} dport {} masquerade comment \"hairpin:{}\"",
                rule.dest_ip, rule.internal_port, rule.name
            );
        }
    }
}

fn port_forward_protos(proto: TransportProto) -> Vec<&'static str> {
    match proto {
        TransportProto::Tcp => vec!["tcp"],
        TransportProto::Udp => vec!["udp"],
        TransportProto::TcpUdp => vec!["tcp", "udp"],
        TransportProto::Icmp | TransportProto::Any => Vec::new(),
    }
}

fn collect_lan_v4_info(cfg: &RouterConfig) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut ifaces = Vec::new();
    let mut router_ips = Vec::new();
    let mut nets = Vec::new();
    for lan in &cfg.lans {
        if !ifaces.contains(&lan.bridge) {
            ifaces.push(lan.bridge.clone());
        }
        if let Ok(IpNet::V4(v4)) = lan.cidr.parse::<IpNet>() {
            router_ips.push(v4.addr().to_string());
            nets.push(v4.trunc().to_string());
        }
    }
    (ifaces, router_ips, nets)
}

fn upnp_lan_ifaces(cfg: &RouterConfig) -> Vec<String> {
    if !cfg.upnp.lan_interfaces.is_empty() {
        return cfg.upnp.lan_interfaces.clone();
    }
    cfg.lans.iter().map(|l| l.bridge.clone()).collect()
}

fn format_addr_set(items: &[String]) -> String {
    if items.len() == 1 {
        items[0].clone()
    } else {
        format!("{{ {} }}", items.join(", "))
    }
}

fn format_port_set(ports: &[u16]) -> String {
    if ports.len() == 1 {
        ports[0].to_string()
    } else {
        let items = ports
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {items} }}")
    }
}

fn write_zone_verdict(out: &mut String, zone: &FirewallZone, dir_kw: &str, action: FirewallAction) {
    if zone.interfaces.is_empty() {
        return;
    }
    let ifaces = format_iface_set(&zone.interfaces);
    let verdict = action_keyword(action);
    let _ = writeln!(
        out,
        "        {dir_kw} {ifaces} {verdict} comment \"zone:{}\"",
        zone.name
    );
}

const fn action_keyword(action: FirewallAction) -> &'static str {
    match action {
        FirewallAction::Accept => "accept",
        FirewallAction::Drop => "drop",
        FirewallAction::Reject => "reject",
    }
}

fn format_iface_set(interfaces: &[String]) -> String {
    if interfaces.len() == 1 {
        format!("\"{}\"", interfaces[0])
    } else {
        let items = interfaces
            .iter()
            .map(|i| format!("\"{i}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {items} }}")
    }
}
