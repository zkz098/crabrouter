mod apply;
mod bond;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::IpAddr;
use std::path::PathBuf;
use std::pin::Pin;

use crabrouter_model::{RouterConfig, WanProto, WanRuntimeState, wan_fwmark, wan_table_id};
use ipnet::IpNet;
use rtnetlink::packet_route::AddressFamily;
use rtnetlink::packet_route::route::RouteProtocol;

use crate::Result;
use crate::dhcpcd::sync_dhcpcd_files;
use crate::nftables::{apply_nft_with_precheck, render_nftables_ruleset};
use crate::nic_tuning::{apply_nic_tuning, render_nic_tuning_script};
use crate::pins::sync_interface_pin_files;
use crate::pppoe::{PppdManager, render_pppd_peer};
use crate::qos::{apply_tc_qos, render_tc_qos_script};

pub use apply::{apply_rtnetlink_diff, apply_rtnetlink_diff_with_wans};
pub use bond::{BondCreateSpec, BondSlaveSpec, BridgeStpSpec, describe_bond};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Filesystem paths used by `LinuxNetworkBackend` when writing OS network artifacts.
#[derive(Debug, Clone)]
pub struct NetworkPaths {
    pub systemd_network_dir: PathBuf,
    pub udev_rules_path: PathBuf,
    pub ppp_dir: PathBuf,
    pub dhcpcd_conf_path: PathBuf,
    pub dhcpcd_hook_path: PathBuf,
    pub hook_endpoint_url: String,
}

impl Default for NetworkPaths {
    fn default() -> Self {
        Self {
            systemd_network_dir: PathBuf::from("/etc/systemd/network"),
            udev_rules_path: PathBuf::from("/etc/udev/rules.d/70-crabrouter-net.rules"),
            ppp_dir: PathBuf::from("/etc/ppp"),
            dhcpcd_conf_path: PathBuf::from("/etc/dhcpcd.d/crabrouter.conf"),
            dhcpcd_hook_path: PathBuf::from("/etc/dhcpcd.exit-hook"),
            hook_endpoint_url: "http://127.0.0.1:8080/api/net/wan-event".to_string(),
        }
    }
}

/// Abstraction over Linux kernel networking & nftables application so tests and dry-run mode
/// can run safely without host privileges.
pub trait NetworkBackend: Send + Sync {
    fn apply_network_and_firewall<'a>(&'a self, cfg: &'a RouterConfig)
    -> BoxFuture<'a, Result<()>>;
}

#[derive(Debug, Default)]
pub struct DryRunNetworkBackend;

impl NetworkBackend for DryRunNetworkBackend {
    fn apply_network_and_firewall<'a>(
        &'a self,
        cfg: &'a RouterConfig,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let ruleset = render_nftables_ruleset(cfg);
            let tc_script = render_tc_qos_script(cfg);
            let _nic_script = render_nic_tuning_script(cfg);
            let plan = compute_network_diff_plan(cfg, &[], &[], &[], &[]);
            let ppp_peers: Vec<String> = cfg
                .wans
                .iter()
                .filter(|w| w.proto == WanProto::Pppoe)
                .map(render_pppd_peer)
                .collect();
            tracing::info!(
                wans = cfg.wans.len(),
                lans = cfg.lans.len(),
                vlans = cfg.vlans.len(),
                bridges_to_create = plan.bridges_to_create.len(),
                vlans_to_create = plan.vlans_to_create.len(),
                addrs_to_add = plan.addrs_to_add.len(),
                routes_to_add = plan.routes_to_add.len(),
                rules_to_add = plan.rules_to_add.len(),
                ppp_peers = ppp_peers.len(),
                nft_bytes = ruleset.len(),
                tc_bytes = tc_script.len(),
                "dry-run: computed network diff plan and rendered nftables/tc configuration"
            );
            Ok(())
        })
    }
}

pub struct LinuxNetworkBackend {
    paths: NetworkPaths,
    pppd: PppdManager,
}

impl Default for LinuxNetworkBackend {
    fn default() -> Self {
        let paths = NetworkPaths::default();
        let pppd = PppdManager::new(&paths.ppp_dir, &paths.hook_endpoint_url);
        Self { paths, pppd }
    }
}

impl LinuxNetworkBackend {
    pub fn with_paths(paths: NetworkPaths) -> Self {
        let pppd = PppdManager::new(&paths.ppp_dir, &paths.hook_endpoint_url);
        Self { paths, pppd }
    }
}

impl NetworkBackend for LinuxNetworkBackend {
    fn apply_network_and_firewall<'a>(
        &'a self,
        cfg: &'a RouterConfig,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            sync_interface_pin_files(
                &cfg.interface_pins,
                &self.paths.systemd_network_dir,
                &self.paths.udev_rules_path,
            )?;

            sync_dhcpcd_files(
                cfg,
                &self.paths.dhcpcd_conf_path,
                &self.paths.dhcpcd_hook_path,
                &self.paths.hook_endpoint_url,
            )?;

            apply_rtnetlink_diff(cfg).await?;
            apply::apply_ipv6_relay_sysctls(cfg);
            apply_nic_tuning(cfg).await?;

            self.pppd.sync(cfg).await?;

            apply_nft_with_precheck(cfg).await?;

            apply_tc_qos(cfg).await?;
            Ok(())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedLink {
    pub index: u32,
    pub name: String,
    pub mac: Option<String>,
    pub controller: Option<u32>,
    pub kind: Option<String>,
    pub vlan_id: Option<u16>,
    pub parent_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedAddr {
    pub link_index: u32,
    pub net: IpNet,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DesiredRoute {
    pub table_id: u32,
    pub destination: IpNet,
    pub gateway: Option<IpAddr>,
    pub device: Option<String>,
    pub metric: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedRoute {
    pub table_id: u32,
    pub destination: IpNet,
    pub gateway: Option<IpAddr>,
    pub oif_index: Option<u32>,
    pub metric: Option<u32>,
    pub protocol: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DesiredRule {
    pub priority: u32,
    pub src: Option<IpNet>,
    pub dst: Option<IpNet>,
    pub iif: Option<String>,
    pub fwmark: Option<u32>,
    pub table_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedRule {
    pub family: AddressFamily,
    pub priority: u32,
    pub src: Option<IpNet>,
    pub dst: Option<IpNet>,
    pub iif: Option<String>,
    pub fwmark: Option<u32>,
    pub table_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VlanCreateSpec {
    pub name: String,
    pub parent: String,
    pub id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgePortSpec {
    pub port: String,
    pub bridge: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceAddrSpec {
    pub iface: String,
    pub net: IpNet,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkDiffPlan {
    pub bonds_to_create: Vec<BondCreateSpec>,
    pub bonds_to_delete: Vec<u32>,
    pub bond_slaves_to_attach: Vec<BondSlaveSpec>,
    pub bond_slaves_to_detach: Vec<String>,
    pub bridges_to_create: Vec<String>,
    pub bridges_stp_to_set: Vec<BridgeStpSpec>,
    pub links_mac_to_set: Vec<(String, [u8; 6])>,
    pub links_to_up_mtu: Vec<(String, Option<u32>)>,
    pub bridge_ports_to_attach: Vec<BridgePortSpec>,
    pub bridge_ports_to_detach: Vec<String>,
    pub vlans_to_create: Vec<VlanCreateSpec>,
    pub vlans_to_delete: Vec<u32>,
    pub addrs_to_add: Vec<InterfaceAddrSpec>,
    pub addrs_to_del: Vec<InterfaceAddrSpec>,
    pub routes_to_add: Vec<DesiredRoute>,
    pub routes_to_del: Vec<ObservedRoute>,
    pub rules_to_add: Vec<DesiredRule>,
    pub rules_to_del: Vec<ObservedRule>,
}

/// Computes the exact incremental diff between `RouterConfig` and observed kernel Netlink state.
pub fn compute_network_diff_plan(
    cfg: &RouterConfig,
    links: &[ObservedLink],
    addrs: &[ObservedAddr],
    routes: &[ObservedRoute],
    rules: &[ObservedRule],
) -> NetworkDiffPlan {
    compute_network_diff_plan_with_wans(cfg, links, addrs, routes, rules, &[], &[])
}

/// Computes incremental Netlink diff including dynamic Multi-WAN routing tables and `fwmark` rules.
pub fn compute_network_diff_plan_with_wans(
    cfg: &RouterConfig,
    links: &[ObservedLink],
    addrs: &[ObservedAddr],
    routes: &[ObservedRoute],
    rules: &[ObservedRule],
    wan_states: &[WanRuntimeState],
    active_wans: &[String],
) -> NetworkDiffPlan {
    let mut plan = NetworkDiffPlan::default();
    let link_by_name: HashMap<&str, &ObservedLink> =
        links.iter().map(|l| (l.name.as_str(), l)).collect();
    let link_by_index: HashMap<u32, &ObservedLink> = links.iter().map(|l| (l.index, l)).collect();

    bond::plan_bonds(cfg, links, &link_by_name, &mut plan);
    plan_bridges_and_ports(cfg, links, &link_by_name, &mut plan);
    bond::plan_bridge_stp(cfg, &mut plan);
    plan_vlans(cfg, links, &link_by_name, &link_by_index, &mut plan);
    plan_addresses(cfg, &link_by_name, &link_by_index, addrs, &mut plan);
    plan_routes(
        cfg,
        &link_by_index,
        routes,
        wan_states,
        active_wans,
        &mut plan,
    );
    plan_policy_rules(cfg, rules, wan_states, &mut plan);

    for wan in &cfg.wans {
        if let Some(mac_str) = wan
            .mac_override
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            && let Some(desired_mac) = apply::parse_mac_bytes(mac_str)
        {
            let already = link_by_name
                .get(wan.device.as_str())
                .and_then(|l| l.mac.as_deref())
                .is_some_and(|m| m.eq_ignore_ascii_case(mac_str));
            if !already {
                plan.links_mac_to_set
                    .push((wan.device.clone(), desired_mac));
            }
        }
        plan.links_to_up_mtu
            .push((wan.device.clone(), Some(u32::from(wan.mtu))));
    }
    plan
}

fn plan_bridges_and_ports(
    cfg: &RouterConfig,
    links: &[ObservedLink],
    link_by_name: &HashMap<&str, &ObservedLink>,
    plan: &mut NetworkDiffPlan,
) {
    let mut managed_bridge_indices = HashMap::new();
    let mut desired_port_to_bridge: HashMap<&str, &str> = HashMap::new();

    for lan in &cfg.lans {
        if let Some(existing) = link_by_name.get(lan.bridge.as_str()) {
            managed_bridge_indices.insert(existing.index, lan.bridge.as_str());
        } else {
            plan.bridges_to_create.push(lan.bridge.clone());
        }
        plan.links_to_up_mtu.push((lan.bridge.clone(), None));
        for port in &lan.ports {
            desired_port_to_bridge.insert(port.as_str(), lan.bridge.as_str());
            plan.links_to_up_mtu.push((port.clone(), None));
            let already_attached = link_by_name
                .get(port.as_str())
                .and_then(|l| l.controller)
                .is_some_and(|ctrl_idx| {
                    managed_bridge_indices.get(&ctrl_idx) == Some(&lan.bridge.as_str())
                });
            if !already_attached {
                plan.bridge_ports_to_attach.push(BridgePortSpec {
                    port: port.clone(),
                    bridge: lan.bridge.clone(),
                });
            }
        }
    }

    for link in links {
        if let Some(ctrl_idx) = link.controller
            && managed_bridge_indices.contains_key(&ctrl_idx)
            && !desired_port_to_bridge.contains_key(link.name.as_str())
        {
            plan.bridge_ports_to_detach.push(link.name.clone());
        }
    }
}

fn plan_vlans(
    cfg: &RouterConfig,
    links: &[ObservedLink],
    link_by_name: &HashMap<&str, &ObservedLink>,
    link_by_index: &HashMap<u32, &ObservedLink>,
    plan: &mut NetworkDiffPlan,
) {
    let mut desired_vlan_names = HashSet::new();
    let configured_parents: HashSet<&str> = cfg
        .wans
        .iter()
        .map(|w| w.device.as_str())
        .chain(cfg.lans.iter().flat_map(|l| {
            std::iter::once(l.bridge.as_str()).chain(l.ports.iter().map(String::as_str))
        }))
        .chain(cfg.vlans.iter().map(|v| v.parent.as_str()))
        .collect();

    for vlan in &cfg.vlans {
        let name = format!("{}.{}", vlan.parent, vlan.id);
        desired_vlan_names.insert(name.clone());
        if !link_by_name.contains_key(name.as_str()) {
            plan.vlans_to_create.push(VlanCreateSpec {
                name: name.clone(),
                parent: vlan.parent.clone(),
                id: vlan.id,
            });
        }
        plan.links_to_up_mtu.push((name, None));
    }

    for link in links {
        if link.kind.as_deref() == Some("vlan")
            && !desired_vlan_names.contains(&link.name)
            && let Some(parent_idx) = link.parent_index
            && let Some(parent) = link_by_index.get(&parent_idx)
            && configured_parents.contains(parent.name.as_str())
        {
            plan.vlans_to_delete.push(link.index);
        }
    }
}

fn collect_desired_addresses(cfg: &RouterConfig) -> HashMap<String, HashSet<IpNet>> {
    let mut desired_by_iface: HashMap<String, HashSet<IpNet>> = HashMap::new();

    for lan in &cfg.lans {
        if let Ok(net) = lan.cidr.parse::<IpNet>() {
            desired_by_iface
                .entry(lan.bridge.clone())
                .or_default()
                .insert(net);
        }
        if let Some(ula) = lan
            .ipv6_ula_cidr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            && let Ok(net) = ula.parse::<IpNet>()
        {
            desired_by_iface
                .entry(lan.bridge.clone())
                .or_default()
                .insert(net);
        }
    }
    for wan in &cfg.wans {
        if wan.proto == WanProto::Static
            && let Some(cidr) = wan.static_cidr.as_deref()
            && let Ok(net) = cidr.parse::<IpNet>()
        {
            desired_by_iface
                .entry(wan.device.clone())
                .or_default()
                .insert(net);
        }
    }
    for vlan in &cfg.vlans {
        if let Some(cidr) = vlan.cidr.as_deref()
            && let Ok(net) = cidr.parse::<IpNet>()
        {
            let name = format!("{}.{}", vlan.parent, vlan.id);
            desired_by_iface.entry(name).or_default().insert(net);
        }
    }

    desired_by_iface
}

fn collect_observed_addresses(
    link_by_index: &HashMap<u32, &ObservedLink>,
    addrs: &[ObservedAddr],
) -> HashMap<String, HashSet<IpNet>> {
    let mut observed_by_iface: HashMap<String, HashSet<IpNet>> = HashMap::new();
    for addr in addrs {
        if is_link_local_or_loopback(&addr.net) {
            continue;
        }
        if let Some(link) = link_by_index.get(&addr.link_index) {
            observed_by_iface
                .entry(link.name.clone())
                .or_default()
                .insert(addr.net);
        }
    }
    observed_by_iface
}

fn plan_addresses(
    cfg: &RouterConfig,
    link_by_name: &HashMap<&str, &ObservedLink>,
    link_by_index: &HashMap<u32, &ObservedLink>,
    addrs: &[ObservedAddr],
    plan: &mut NetworkDiffPlan,
) {
    let desired_by_iface = collect_desired_addresses(cfg);
    let observed_by_iface = collect_observed_addresses(link_by_index, addrs);

    for (iface, desired_nets) in &desired_by_iface {
        let existing = observed_by_iface.get(iface);
        for &net in desired_nets {
            if !existing.is_some_and(|set| set.contains(&net)) {
                plan.addrs_to_add.push(InterfaceAddrSpec {
                    iface: iface.clone(),
                    net,
                });
            }
        }
        if let Some(existing_nets) = existing
            && link_by_name.contains_key(iface.as_str())
        {
            for &net in existing_nets {
                // Preserve dynamic global unicast IPv6 addresses (2000::/3) assigned via DHCPv6-PD
                if !is_dynamic_gua_ipv6(&net) && !desired_nets.contains(&net) {
                    plan.addrs_to_del.push(InterfaceAddrSpec {
                        iface: iface.clone(),
                        net,
                    });
                }
            }
        }
    }
}

fn is_dynamic_gua_ipv6(net: &IpNet) -> bool {
    match net.addr() {
        IpAddr::V6(v6) => (v6.segments()[0] & 0xe000) == 0x2000,
        IpAddr::V4(_) => false,
    }
}

fn plan_routes(
    cfg: &RouterConfig,
    link_by_index: &HashMap<u32, &ObservedLink>,
    routes: &[ObservedRoute],
    wan_states: &[WanRuntimeState],
    active_wans: &[String],
    plan: &mut NetworkDiffPlan,
) {
    let mut desired: Vec<DesiredRoute> = Vec::new();
    let mut managed_tables: HashSet<u32> = HashSet::new();

    for (idx, wan) in cfg.wans.iter().enumerate() {
        let rt = wan_states.iter().find(|s| s.wan_name == wan.name);
        let gw_opt = wan
            .gateway
            .as_deref()
            .or_else(|| rt.and_then(|s| s.gateway.as_deref()))
            .and_then(|s| s.parse::<IpAddr>().ok());
        let dev = rt.map_or_else(|| wan.device.clone(), |s| s.device.clone());

        if wan.proto == WanProto::Static
            && let Some(gw) = gw_opt
        {
            let dest = default_route_prefix(gw);
            let is_active = !cfg.multi_wan.enabled
                || active_wans.is_empty()
                || active_wans.iter().any(|n| n == &wan.name);
            if is_active {
                managed_tables.insert(254);
                desired.push(DesiredRoute {
                    table_id: 254,
                    destination: dest,
                    gateway: Some(gw),
                    device: Some(dev.clone()),
                    metric: Some(wan.metric),
                });
            }
        }

        if cfg.multi_wan.enabled
            && let Some(gw) = gw_opt
        {
            let table_id = wan_table_id(idx, wan);
            managed_tables.insert(table_id);
            desired.push(DesiredRoute {
                table_id,
                destination: default_route_prefix(gw),
                gateway: Some(gw),
                device: Some(dev),
                metric: Some(wan.metric),
            });
        }
    }

    for r in &cfg.routes {
        if let Ok(dest) = r.destination.parse::<IpNet>() {
            let gw = r.gateway.as_deref().and_then(|s| s.parse::<IpAddr>().ok());
            managed_tables.insert(r.table_id);
            desired.push(DesiredRoute {
                table_id: r.table_id,
                destination: dest,
                gateway: gw,
                device: r.device.clone(),
                metric: r.metric,
            });
        }
    }

    for want in &desired {
        let exists = routes
            .iter()
            .any(|obs| route_matches(obs, want, link_by_index));
        if !exists {
            plan.routes_to_add.push(want.clone());
        }
    }

    // Only delete static routes (RTPROT_STATIC = 4 or RTPROT_BOOT = 3) in managed tables
    // so kernel-generated connected/RA/DHCP routes are never touched.
    for obs in routes {
        if !managed_tables.contains(&obs.table_id) {
            continue;
        }
        if obs.protocol != u8::from(RouteProtocol::Static) {
            continue;
        }
        let still_wanted = desired
            .iter()
            .any(|want| route_matches(obs, want, link_by_index));
        if !still_wanted {
            plan.routes_to_del.push(obs.clone());
        }
    }
}

fn route_matches(
    obs: &ObservedRoute,
    want: &DesiredRoute,
    link_by_index: &HashMap<u32, &ObservedLink>,
) -> bool {
    if obs.table_id != want.table_id || obs.destination != want.destination {
        return false;
    }
    if obs.gateway != want.gateway {
        return false;
    }
    if let Some(dev) = want.device.as_deref() {
        let obs_dev = obs
            .oif_index
            .and_then(|idx| link_by_index.get(&idx))
            .map(|l| l.name.as_str());
        if obs_dev != Some(dev) {
            return false;
        }
    }
    true
}

fn default_route_prefix(gw: IpAddr) -> IpNet {
    match gw {
        IpAddr::V4(_) => "0.0.0.0/0".parse().expect("valid v4 default"),
        IpAddr::V6(_) => "::/0".parse().expect("valid v6 default"),
    }
}

fn plan_policy_rules(
    cfg: &RouterConfig,
    rules: &[ObservedRule],
    wan_states: &[WanRuntimeState],
    plan: &mut NetworkDiffPlan,
) {
    let mut desired: Vec<DesiredRule> = Vec::new();
    for rule in &cfg.policy_rules {
        desired.push(DesiredRule {
            priority: rule.priority,
            src: rule.src.as_deref().and_then(|s| s.parse().ok()),
            dst: rule.dst.as_deref().and_then(|s| s.parse().ok()),
            iif: rule.iif.clone(),
            fwmark: None,
            table_id: rule.table_id,
        });
    }

    if cfg.multi_wan.enabled {
        for (idx, wan) in cfg.wans.iter().enumerate() {
            let table_id = wan_table_id(idx, wan);
            desired.push(DesiredRule {
                priority: 1000 + idx as u32,
                src: None,
                dst: None,
                iif: None,
                fwmark: Some(wan_fwmark(table_id)),
                table_id,
            });
            if let Some(rt) = wan_states.iter().find(|s| s.wan_name == wan.name)
                && let Some(ip_str) = rt.ip.as_deref()
                && let Ok(ip) = ip_str.parse::<IpAddr>()
            {
                desired.push(DesiredRule {
                    priority: 1100 + idx as u32,
                    src: Some(IpNet::from(ip)),
                    dst: None,
                    iif: None,
                    fwmark: None,
                    table_id,
                });
            }
        }
    }

    for want in &desired {
        let exists = rules.iter().any(|obs| rule_matches(obs, want));
        if !exists {
            plan.rules_to_add.push(want.clone());
        }
    }

    for obs in rules {
        // Never touch kernel default rules (local=0, main=32766, default=32767)
        if obs.priority == 0 || obs.priority >= 32766 {
            continue;
        }
        let wanted = desired.iter().any(|want| rule_matches(obs, want));
        if !wanted {
            plan.rules_to_del.push(obs.clone());
        }
    }
}

fn rule_matches(obs: &ObservedRule, want: &DesiredRule) -> bool {
    obs.priority == want.priority
        && obs.table_id == want.table_id
        && obs.src == want.src
        && obs.dst == want.dst
        && obs.iif == want.iif
        && obs.fwmark == want.fwmark
}

fn is_link_local_or_loopback(net: &IpNet) -> bool {
    match net.addr() {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabrouter_model::{InterfaceRole, PolicyRuleConfig, StaticRouteConfig, VlanConfig};

    #[test]
    fn computes_incremental_diff_for_bridges_vlans_addrs_and_pbr() {
        let mut cfg = RouterConfig::default();
        cfg.vlans.push(VlanConfig {
            parent: "eth1".to_string(),
            id: 20,
            role: InterfaceRole::Lan,
            cidr: Some("192.168.20.1/24".to_string()),
            ipv6_ra_enabled: true,
            ipv6_subnet_id: None,
        });
        cfg.routes.push(StaticRouteConfig {
            table_id: 100,
            destination: "10.20.0.0/16".to_string(),
            gateway: Some("192.168.10.254".to_string()),
            device: Some("br-lan".to_string()),
            metric: Some(50),
        });
        cfg.policy_rules.push(PolicyRuleConfig {
            priority: 1000,
            src: Some("192.168.20.0/24".to_string()),
            dst: None,
            iif: None,
            table_id: 100,
        });

        let links = vec![
            ObservedLink {
                index: 2,
                name: "eth1".to_string(),
                mac: None,
                controller: Some(10),
                kind: None,
                vlan_id: None,
                parent_index: None,
            },
            ObservedLink {
                index: 10,
                name: "br-lan".to_string(),
                mac: None,
                controller: None,
                kind: Some("bridge".to_string()),
                vlan_id: None,
                parent_index: None,
            },
            ObservedLink {
                index: 11,
                name: "eth9".to_string(),
                mac: None,
                controller: Some(10),
                kind: None,
                vlan_id: None,
                parent_index: None,
            },
            ObservedLink {
                index: 12,
                name: "eth1.99".to_string(),
                mac: None,
                controller: None,
                kind: Some("vlan".to_string()),
                vlan_id: Some(99),
                parent_index: Some(2),
            },
        ];

        let plan = compute_network_diff_plan(&cfg, &links, &[], &[], &[]);
        assert_eq!(plan.bridges_to_create, Vec::<String>::new());
        assert_eq!(plan.bridge_ports_to_detach, vec!["eth9"]);
        assert_eq!(plan.vlans_to_create.len(), 1);
        assert_eq!(plan.vlans_to_create[0].name, "eth1.20");
        assert_eq!(plan.vlans_to_delete, vec![12]);
        assert_eq!(plan.addrs_to_add.len(), 2);
        assert_eq!(plan.routes_to_add.len(), 1);
        assert_eq!(plan.rules_to_add.len(), 1);
    }
}
