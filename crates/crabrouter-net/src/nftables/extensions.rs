use std::fmt::Write;

use crabrouter_model::{InterfaceRole, RouterConfig};

use super::format_iface_set;

pub(super) fn write_crowdsec_sets(out: &mut String, cfg: &RouterConfig) {
    if !cfg.crowdsec.enabled || !cfg.crowdsec.bouncer_enabled {
        return;
    }
    out.push_str(
        "    set crowdsec_block_v4 {\n\
         \x20       type ipv4_addr\n\
         \x20       flags interval, timeout\n\
         \x20   }\n\n\
         \x20   set crowdsec_block_v6 {\n\
         \x20       type ipv6_addr\n\
         \x20       flags interval, timeout\n\
         \x20   }\n\n",
    );
}

pub(super) fn write_crowdsec_drop_rules(out: &mut String, cfg: &RouterConfig) {
    if !cfg.crowdsec.enabled || !cfg.crowdsec.bouncer_enabled {
        return;
    }
    out.push_str(
        "        ip saddr @crowdsec_block_v4 drop comment \"crowdsec-ban-v4\"\n\
         \x20       ip6 saddr @crowdsec_block_v6 drop comment \"crowdsec-ban-v6\"\n",
    );
}

pub(super) fn write_mdns_input_rules(out: &mut String, cfg: &RouterConfig) {
    if !cfg.mdns.enabled {
        return;
    }
    let ifaces = resolve_mdns_ifaces(cfg);
    if ifaces.is_empty() {
        return;
    }
    let iface_set = format_iface_set(&ifaces);
    let _ = writeln!(
        out,
        "        iifname {iface_set} udp dport 5353 ip daddr 224.0.0.251 accept comment \"mdns-v4\""
    );
    let _ = writeln!(
        out,
        "        iifname {iface_set} udp dport 5353 ip6 daddr ff02::fb accept comment \"mdns-v6\""
    );
}

pub(super) fn write_iptv_input_rules(out: &mut String, cfg: &RouterConfig) {
    if !cfg.iptv.enabled {
        return;
    }
    let upstream = cfg.iptv.upstream_interface.trim();
    let downstream = resolve_iptv_downstream_ifaces(cfg);
    let mut igmp_ifaces = Vec::new();
    if !upstream.is_empty() {
        igmp_ifaces.push(upstream.to_string());
    }
    for iface in &downstream {
        if !igmp_ifaces.contains(iface) {
            igmp_ifaces.push(iface.clone());
        }
    }
    if !igmp_ifaces.is_empty() {
        let igmp_set = format_iface_set(&igmp_ifaces);
        let _ = writeln!(
            out,
            "        iifname {igmp_set} ip protocol igmp accept comment \"iptv-igmp\""
        );
    }
    if cfg.iptv.udpxy_enabled && !downstream.is_empty() {
        let down_set = format_iface_set(&downstream);
        let _ = writeln!(
            out,
            "        iifname {down_set} tcp dport {} accept comment \"iptv-udpxy\"",
            cfg.iptv.udpxy_port
        );
    }
}

pub(super) fn write_iptv_forward_rules(out: &mut String, cfg: &RouterConfig) {
    if !cfg.iptv.enabled {
        return;
    }
    let upstream = cfg.iptv.upstream_interface.trim();
    let downstream = resolve_iptv_downstream_ifaces(cfg);
    if upstream.is_empty() || downstream.is_empty() {
        return;
    }
    let down_set = format_iface_set(&downstream);
    let _ = writeln!(
        out,
        "        iifname \"{upstream}\" oifname {down_set} ip daddr 224.0.0.0/4 ip protocol udp accept comment \"iptv-multicast\""
    );
}

fn resolve_mdns_ifaces(cfg: &RouterConfig) -> Vec<String> {
    if !cfg.mdns.interfaces.is_empty() {
        return cfg
            .mdns
            .interfaces
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    let mut out: Vec<String> = cfg.lans.iter().map(|l| l.bridge.clone()).collect();
    for vlan in &cfg.vlans {
        if vlan.role == InterfaceRole::Lan {
            let name = format!("{}.{}", vlan.parent, vlan.id);
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

fn resolve_iptv_downstream_ifaces(cfg: &RouterConfig) -> Vec<String> {
    if !cfg.iptv.downstream_interfaces.is_empty() {
        return cfg
            .iptv
            .downstream_interfaces
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    cfg.lans.iter().map(|l| l.bridge.clone()).collect()
}
