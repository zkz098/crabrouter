use std::fmt::Write;

use crabrouter_model::{InterfaceRole, Ipv6NatMode, RouterConfig};
use ipnet::IpNet;

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

pub(super) fn write_ipv6_nat_prerouting(out: &mut String, cfg: &RouterConfig) {
    if cfg.firewall.ipv6_nat_mode != Ipv6NatMode::Nptv6 {
        return;
    }
    let Some(int_prefix) = cfg.firewall.nptv6_internal_prefix.as_deref() else {
        return;
    };
    let Some(ext_prefix) = cfg.firewall.nptv6_external_prefix.as_deref() else {
        return;
    };
    for zone in &cfg.firewall.zones {
        if zone.masquerade && !zone.interfaces.is_empty() {
            let ifaces = format_iface_set(&zone.interfaces);
            let _ = writeln!(
                out,
                "        iifname {ifaces} ip6 daddr {} dnat ip6 prefix to {} comment \"nptv6-in\"",
                ext_prefix.trim(),
                int_prefix.trim()
            );
        }
    }
}

pub(super) fn write_ipv6_nat_postrouting(out: &mut String, cfg: &RouterConfig) {
    match cfg.firewall.ipv6_nat_mode {
        Ipv6NatMode::Disabled => {}
        Ipv6NatMode::Masquerade => {
            for zone in &cfg.firewall.zones {
                if zone.masquerade && !zone.interfaces.is_empty() {
                    let ifaces = format_iface_set(&zone.interfaces);
                    let _ = writeln!(
                        out,
                        "        meta nfproto ipv6 oifname {ifaces} masquerade comment \"nat66\""
                    );
                }
            }
        }
        Ipv6NatMode::Nptv6 => {
            let Some(int_prefix) = cfg.firewall.nptv6_internal_prefix.as_deref() else {
                return;
            };
            let Some(ext_prefix) = cfg.firewall.nptv6_external_prefix.as_deref() else {
                return;
            };
            for zone in &cfg.firewall.zones {
                if zone.masquerade && !zone.interfaces.is_empty() {
                    let ifaces = format_iface_set(&zone.interfaces);
                    let _ = writeln!(
                        out,
                        "        oifname {ifaces} ip6 saddr {} snat ip6 prefix to {} comment \"nptv6-out\"",
                        int_prefix.trim(),
                        ext_prefix.trim()
                    );
                }
            }
        }
    }
}

pub(super) fn write_modem_bridge_postrouting(out: &mut String, cfg: &RouterConfig) {
    for wan in &cfg.wans {
        let Some(modem_cidr) = wan
            .modem_cidr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if let Ok(IpNet::V4(net)) = modem_cidr.parse::<IpNet>() {
            let _ = writeln!(
                out,
                "        meta nfproto ipv4 oifname \"{}\" ip daddr {} masquerade comment \"modem-bridge:{}\"",
                wan.device,
                net.trunc(),
                wan.name
            );
        }
    }
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

pub(super) fn collect_lan_interfaces(cfg: &RouterConfig) -> Vec<String> {
    let mut ifaces = Vec::new();
    for lan in &cfg.lans {
        if !ifaces.contains(&lan.bridge) {
            ifaces.push(lan.bridge.clone());
        }
    }
    for vlan in &cfg.vlans {
        if vlan.role == crabrouter_model::InterfaceRole::Lan {
            let name = format!("{}.{}", vlan.parent, vlan.id);
            if !ifaces.contains(&name) {
                ifaces.push(name);
            }
        }
    }
    ifaces
}

pub(super) fn write_wireguard_input_rules(out: &mut String, cfg: &RouterConfig) {
    if !cfg.wireguard.enabled {
        return;
    }
    let iface = cfg.wireguard.interface.trim();
    if iface.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "        udp dport {} accept comment \"wireguard-in\"",
        cfg.wireguard.listen_port
    );
    let _ = writeln!(
        out,
        "        iifname \"{iface}\" accept comment \"wireguard-iface-in\""
    );
}

pub(super) fn write_wireguard_forward_rules(out: &mut String, cfg: &RouterConfig) {
    if !cfg.wireguard.enabled {
        return;
    }
    let iface = cfg.wireguard.interface.trim();
    if iface.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "        iifname \"{iface}\" accept comment \"wireguard-fwd-in\""
    );
    let _ = writeln!(
        out,
        "        oifname \"{iface}\" accept comment \"wireguard-fwd-out\""
    );
}

pub(super) fn write_wireguard_postrouting_rules(
    out: &mut String,
    cfg: &RouterConfig,
    wan_ifaces: &[String],
) {
    if !cfg.wireguard.enabled || wan_ifaces.is_empty() {
        return;
    }
    let iface = cfg.wireguard.interface.trim();
    if iface.is_empty() {
        return;
    }
    let wan_set = format_iface_set(wan_ifaces);
    let _ = writeln!(
        out,
        "        meta nfproto ipv4 iifname \"{iface}\" oifname {wan_set} masquerade comment \"wireguard-nat\""
    );
}

pub(super) fn write_mangle_bypass_nets(out: &mut String, cfg: &RouterConfig) {
    for lan in &cfg.lans {
        if let Ok(net @ IpNet::V4(_)) = lan.cidr.parse::<IpNet>() {
            let _ = writeln!(out, "        ip daddr {net} return");
        }
    }
    for vlan in &cfg.vlans {
        if vlan.role == crabrouter_model::InterfaceRole::Lan
            && let Some(cidr) = vlan.cidr.as_deref()
            && let Ok(net @ IpNet::V4(_)) = cidr.parse::<IpNet>()
        {
            let _ = writeln!(out, "        ip daddr {net} return");
        }
    }
    if cfg.containers.enabled
        && let Ok(net @ IpNet::V4(_)) = cfg.containers.default_subnet.parse::<IpNet>()
    {
        let _ = writeln!(out, "        ip daddr {net} return");
    }
}
