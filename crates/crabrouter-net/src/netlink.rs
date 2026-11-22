use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::pin::Pin;

use crabrouter_model::{RouterConfig, WanProto};
use futures_util::stream::TryStreamExt;
use ipnet::IpNet;
use rtnetlink::packet_route::AddressFamily;
use rtnetlink::packet_route::address::AddressAttribute;
use rtnetlink::packet_route::link::{InfoData, InfoKind, LinkAttribute, LinkInfo};
use rtnetlink::packet_route::route::{RouteAddress, RouteAttribute, RouteProtocol};
use rtnetlink::packet_route::rule::RuleAttribute;
use rtnetlink::{
    AddressMessageBuilder, Handle, IpVersion, LinkBridge, LinkUnspec, LinkVlan,
    RouteMessageBuilder, new_connection,
};

use crate::dhcpcd::sync_dhcpcd_files;
use crate::nftables::{apply_nft_with_precheck, render_nftables_ruleset};
use crate::pins::sync_interface_pin_files;
use crate::pppoe::{PppdManager, render_pppd_peer};
use crate::{NetError, Result};

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
            dhcpcd_hook_path: PathBuf::from("/lib/dhcpcd/dhcpcd-hooks/90-crabrouter"),
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
                "dry-run: computed network diff plan and rendered nftables configuration"
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

            self.pppd.sync(cfg).await?;

            apply_nft_with_precheck(cfg).await?;
            Ok(())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedLink {
    pub index: u32,
    pub name: String,
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
    pub table_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedRule {
    pub family: AddressFamily,
    pub priority: u32,
    pub src: Option<IpNet>,
    pub dst: Option<IpNet>,
    pub iif: Option<String>,
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
    pub bridges_to_create: Vec<String>,
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
    let mut plan = NetworkDiffPlan::default();
    let link_by_name: HashMap<&str, &ObservedLink> =
        links.iter().map(|l| (l.name.as_str(), l)).collect();
    let link_by_index: HashMap<u32, &ObservedLink> = links.iter().map(|l| (l.index, l)).collect();

    plan_bridges_and_ports(cfg, links, &link_by_name, &mut plan);
    plan_vlans(cfg, links, &link_by_name, &link_by_index, &mut plan);
    plan_addresses(cfg, &link_by_name, &link_by_index, addrs, &mut plan);
    plan_routes(cfg, &link_by_index, routes, &mut plan);
    plan_policy_rules(cfg, rules, &mut plan);

    for wan in &cfg.wans {
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

fn plan_addresses(
    cfg: &RouterConfig,
    link_by_name: &HashMap<&str, &ObservedLink>,
    link_by_index: &HashMap<u32, &ObservedLink>,
    addrs: &[ObservedAddr],
    plan: &mut NetworkDiffPlan,
) {
    let mut desired_by_iface: HashMap<String, HashSet<IpNet>> = HashMap::new();

    for lan in &cfg.lans {
        if let Ok(net) = lan.cidr.parse::<IpNet>() {
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
        if let Some(existing_nets) = existing {
            for &net in existing_nets {
                if !desired_nets.contains(&net) && link_by_name.contains_key(iface.as_str()) {
                    plan.addrs_to_del.push(InterfaceAddrSpec {
                        iface: iface.clone(),
                        net,
                    });
                }
            }
        }
    }
}

fn plan_routes(
    cfg: &RouterConfig,
    link_by_index: &HashMap<u32, &ObservedLink>,
    routes: &[ObservedRoute],
    plan: &mut NetworkDiffPlan,
) {
    let mut desired: Vec<DesiredRoute> = Vec::new();
    let mut managed_tables: HashSet<u32> = HashSet::new();

    for wan in &cfg.wans {
        if wan.proto == WanProto::Static
            && let Some(gw_str) = wan.gateway.as_deref()
            && let Ok(gw) = gw_str.parse::<IpAddr>()
        {
            let dest = match gw {
                IpAddr::V4(_) => "0.0.0.0/0".parse().expect("valid v4 default"),
                IpAddr::V6(_) => "::/0".parse().expect("valid v6 default"),
            };
            managed_tables.insert(254);
            desired.push(DesiredRoute {
                table_id: 254,
                destination: dest,
                gateway: Some(gw),
                device: Some(wan.device.clone()),
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

fn plan_policy_rules(cfg: &RouterConfig, rules: &[ObservedRule], plan: &mut NetworkDiffPlan) {
    let mut desired: Vec<DesiredRule> = Vec::new();
    for rule in &cfg.policy_rules {
        desired.push(DesiredRule {
            priority: rule.priority,
            src: rule.src.as_deref().and_then(|s| s.parse().ok()),
            dst: rule.dst.as_deref().and_then(|s| s.parse().ok()),
            iif: rule.iif.clone(),
            table_id: rule.table_id,
        });
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
}

fn is_link_local_or_loopback(net: &IpNet) -> bool {
    match net.addr() {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// Applies the incremental Netlink diff plan to the live Linux kernel via `rtnetlink`.
pub async fn apply_rtnetlink_diff(cfg: &RouterConfig) -> Result<()> {
    let (connection, handle, _) =
        new_connection().map_err(|e| NetError::Apply(format!("rtnetlink socket error: {e}")))?;
    let conn_task = tokio::spawn(connection);

    let result = apply_rtnetlink_with_handle(&handle, cfg).await;
    conn_task.abort();
    result
}

async fn apply_rtnetlink_with_handle(handle: &Handle, cfg: &RouterConfig) -> Result<()> {
    let links = dump_links(handle).await?;
    let addrs = dump_addresses(handle).await?;
    let routes = dump_routes(handle).await?;
    let rules = dump_rules(handle).await?;

    let plan = compute_network_diff_plan(cfg, &links, &addrs, &routes, &rules);
    let name_to_index = apply_link_diff(handle, &plan).await?;
    apply_addr_diff(handle, &plan, &name_to_index).await?;
    apply_route_and_rule_diff(handle, &plan, &name_to_index).await?;
    Ok(())
}

async fn apply_link_diff(handle: &Handle, plan: &NetworkDiffPlan) -> Result<HashMap<String, u32>> {
    for vlan_idx in &plan.vlans_to_delete {
        handle
            .link()
            .del(*vlan_idx)
            .execute()
            .await
            .map_err(|e| NetError::Apply(format!("delete vlan link {vlan_idx} failed: {e}")))?;
    }

    for port in &plan.bridge_ports_to_detach {
        handle
            .link()
            .set(LinkUnspec::new_with_name(port).controller(0).build())
            .execute()
            .await
            .map_err(|e| NetError::Apply(format!("detach bridge port '{port}' failed: {e}")))?;
    }

    for bridge in &plan.bridges_to_create {
        handle
            .link()
            .add(LinkBridge::new(bridge).up().build())
            .execute()
            .await
            .map_err(|e| NetError::Apply(format!("create bridge '{bridge}' failed: {e}")))?;
    }

    let mut name_to_index = refresh_name_to_index(handle).await?;

    for spec in &plan.bridge_ports_to_attach {
        let Some(&bridge_idx) = name_to_index.get(&spec.bridge) else {
            return Err(NetError::Apply(format!(
                "bridge '{}' missing after creation",
                spec.bridge
            )));
        };
        if name_to_index.contains_key(&spec.port) {
            handle
                .link()
                .set(
                    LinkUnspec::new_with_name(&spec.port)
                        .controller(bridge_idx)
                        .up()
                        .build(),
                )
                .execute()
                .await
                .map_err(|e| {
                    NetError::Apply(format!(
                        "attach port '{}' to bridge '{}' failed: {e}",
                        spec.port, spec.bridge
                    ))
                })?;
        } else {
            tracing::warn!(port = %spec.port, bridge = %spec.bridge, "bridge port interface not found on host");
        }
    }

    for vlan in &plan.vlans_to_create {
        let Some(&parent_idx) = name_to_index.get(&vlan.parent) else {
            return Err(NetError::Apply(format!(
                "VLAN parent interface '{}' does not exist",
                vlan.parent
            )));
        };
        handle
            .link()
            .add(LinkVlan::new(&vlan.name, parent_idx, vlan.id).up().build())
            .execute()
            .await
            .map_err(|e| NetError::Apply(format!("create VLAN '{}' failed: {e}", vlan.name)))?;
    }

    if !plan.vlans_to_create.is_empty() {
        name_to_index = refresh_name_to_index(handle).await?;
    }

    for (iface, mtu_opt) in &plan.links_to_up_mtu {
        if let Some(&idx) = name_to_index.get(iface) {
            let mut req = LinkUnspec::new_with_index(idx).up();
            if let Some(mtu) = mtu_opt {
                req = req.mtu(*mtu);
            }
            if let Err(err) = handle.link().set(req.build()).execute().await {
                tracing::warn!(iface = %iface, error = %err, "failed to bring link up/set MTU");
            }
        }
    }

    Ok(name_to_index)
}

async fn apply_addr_diff(
    handle: &Handle,
    plan: &NetworkDiffPlan,
    name_to_index: &HashMap<String, u32>,
) -> Result<()> {
    for spec in &plan.addrs_to_del {
        if let Some(&idx) = name_to_index.get(&spec.iface) {
            let msg = match spec.net.addr() {
                IpAddr::V4(v4) => AddressMessageBuilder::<Ipv4Addr>::new()
                    .index(idx)
                    .address(v4, spec.net.prefix_len())
                    .build(),
                IpAddr::V6(v6) => AddressMessageBuilder::<Ipv6Addr>::new()
                    .index(idx)
                    .address(v6, spec.net.prefix_len())
                    .build(),
            };
            handle.address().del(msg).execute().await.map_err(|e| {
                NetError::Apply(format!(
                    "delete address {} on '{}' failed: {e}",
                    spec.net, spec.iface
                ))
            })?;
        }
    }
    for spec in &plan.addrs_to_add {
        if let Some(&idx) = name_to_index.get(&spec.iface) {
            handle
                .address()
                .add(idx, spec.net.addr(), spec.net.prefix_len())
                .execute()
                .await
                .map_err(|e| {
                    NetError::Apply(format!(
                        "add address {} on '{}' failed: {e}",
                        spec.net, spec.iface
                    ))
                })?;
        }
    }
    Ok(())
}

async fn apply_route_and_rule_diff(
    handle: &Handle,
    plan: &NetworkDiffPlan,
    name_to_index: &HashMap<String, u32>,
) -> Result<()> {
    for route in &plan.routes_to_del {
        del_route(handle, route).await?;
    }
    for route in &plan.routes_to_add {
        add_route(handle, route, name_to_index).await?;
    }
    for rule in &plan.rules_to_del {
        del_rule(handle, rule).await?;
    }
    for rule in &plan.rules_to_add {
        add_rule(handle, rule).await?;
    }
    Ok(())
}

async fn refresh_name_to_index(handle: &Handle) -> Result<HashMap<String, u32>> {
    let links = dump_links(handle).await?;
    Ok(links.into_iter().map(|l| (l.name, l.index)).collect())
}

async fn dump_links(handle: &Handle) -> Result<Vec<ObservedLink>> {
    let mut stream = handle.link().get().execute();
    let mut out = Vec::new();
    while let Some(msg) = stream
        .try_next()
        .await
        .map_err(|e| NetError::Apply(format!("dump links failed: {e}")))?
    {
        let mut name = None;
        let mut controller = None;
        let mut parent_index = None;
        let mut kind = None;
        let mut vlan_id = None;

        for attr in msg.attributes {
            match attr {
                LinkAttribute::IfName(n) => name = Some(n),
                LinkAttribute::Controller(idx) if idx > 0 => controller = Some(idx),
                LinkAttribute::Link(idx) if idx > 0 => parent_index = Some(idx),
                LinkAttribute::LinkInfo(infos) => {
                    for info in infos {
                        match info {
                            LinkInfo::Kind(k) => {
                                kind = Some(match k {
                                    InfoKind::Bridge => "bridge".to_string(),
                                    InfoKind::Vlan => "vlan".to_string(),
                                    other => format!("{other:?}").to_ascii_lowercase(),
                                });
                            }
                            LinkInfo::Data(InfoData::Vlan(vlan_attrs)) => {
                                for va in vlan_attrs {
                                    if let rtnetlink::packet_route::link::InfoVlan::Id(id) = va {
                                        vlan_id = Some(id);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(name) = name {
            out.push(ObservedLink {
                index: msg.header.index,
                name,
                controller,
                kind,
                vlan_id,
                parent_index,
            });
        }
    }
    Ok(out)
}

async fn dump_addresses(handle: &Handle) -> Result<Vec<ObservedAddr>> {
    let mut stream = handle.address().get().execute();
    let mut out = Vec::new();
    while let Some(msg) = stream
        .try_next()
        .await
        .map_err(|e| NetError::Apply(format!("dump addresses failed: {e}")))?
    {
        let prefix_len = msg.header.prefix_len;
        let mut ip = None;
        for attr in msg.attributes {
            match attr {
                AddressAttribute::Local(addr) => ip = Some(addr),
                AddressAttribute::Address(addr) if ip.is_none() => ip = Some(addr),
                _ => {}
            }
        }
        if let Some(addr) = ip
            && let Ok(net) = IpNet::new(addr, prefix_len)
        {
            out.push(ObservedAddr {
                link_index: msg.header.index,
                net,
            });
        }
    }
    Ok(out)
}

async fn dump_routes(handle: &Handle) -> Result<Vec<ObservedRoute>> {
    let mut out = Vec::new();
    for version in [IpVersion::V4, IpVersion::V6] {
        let query = match version {
            IpVersion::V4 => RouteMessageBuilder::<Ipv4Addr>::new().build(),
            IpVersion::V6 => RouteMessageBuilder::<Ipv6Addr>::new().build(),
        };
        let mut stream = handle.route().get(query).execute();
        while let Some(msg) = stream
            .try_next()
            .await
            .map_err(|e| NetError::Apply(format!("dump routes failed: {e}")))?
        {
            let mut table_id = u32::from(msg.header.table);
            let mut dst_ip: Option<IpAddr> = None;
            let mut gateway: Option<IpAddr> = None;
            let mut oif_index = None;
            let mut metric = None;

            for attr in msg.attributes {
                match attr {
                    RouteAttribute::Table(t) => table_id = t,
                    RouteAttribute::Destination(RouteAddress::Inet(v4)) => {
                        dst_ip = Some(IpAddr::V4(v4));
                    }
                    RouteAttribute::Destination(RouteAddress::Inet6(v6)) => {
                        dst_ip = Some(IpAddr::V6(v6));
                    }
                    RouteAttribute::Gateway(RouteAddress::Inet(v4)) => {
                        gateway = Some(IpAddr::V4(v4));
                    }
                    RouteAttribute::Gateway(RouteAddress::Inet6(v6)) => {
                        gateway = Some(IpAddr::V6(v6));
                    }
                    RouteAttribute::Oif(idx) => oif_index = Some(idx),
                    RouteAttribute::Priority(m) => metric = Some(m),
                    _ => {}
                }
            }

            let default_ip = match msg.header.address_family {
                AddressFamily::Inet => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                AddressFamily::Inet6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                _ => continue,
            };
            let ip = dst_ip.unwrap_or(default_ip);
            if let Ok(destination) = IpNet::new(ip, msg.header.destination_prefix_length) {
                out.push(ObservedRoute {
                    table_id,
                    destination,
                    gateway,
                    oif_index,
                    metric,
                    protocol: u8::from(msg.header.protocol),
                });
            }
        }
    }
    Ok(out)
}

async fn dump_rules(handle: &Handle) -> Result<Vec<ObservedRule>> {
    let mut out = Vec::new();
    for version in [IpVersion::V4, IpVersion::V6] {
        let mut stream = handle.rule().get(version).execute();
        while let Some(msg) = stream
            .try_next()
            .await
            .map_err(|e| NetError::Apply(format!("dump rules failed: {e}")))?
        {
            let mut table_id = u32::from(msg.header.table);
            let mut priority = 0;
            let mut src_ip = None;
            let mut dst_ip = None;
            let mut iif = None;

            for attr in msg.attributes {
                match attr {
                    RuleAttribute::Table(t) => table_id = t,
                    RuleAttribute::Priority(p) => priority = p,
                    RuleAttribute::Source(ip) => src_ip = Some(ip),
                    RuleAttribute::Destination(ip) => dst_ip = Some(ip),
                    RuleAttribute::Iifname(name) => iif = Some(name),
                    _ => {}
                }
            }

            let src = src_ip.and_then(|ip| IpNet::new(ip, msg.header.src_len).ok());
            let dst = dst_ip.and_then(|ip| IpNet::new(ip, msg.header.dst_len).ok());
            out.push(ObservedRule {
                family: msg.header.family,
                priority,
                src,
                dst,
                iif,
                table_id,
            });
        }
    }
    Ok(out)
}

async fn add_route(
    handle: &Handle,
    route: &DesiredRoute,
    name_to_index: &HashMap<String, u32>,
) -> Result<()> {
    let oif = route
        .device
        .as_deref()
        .and_then(|d| name_to_index.get(d).copied());

    let msg = match route.destination {
        IpNet::V4(v4) => {
            let mut b = RouteMessageBuilder::<Ipv4Addr>::new()
                .destination_prefix(v4.addr(), v4.prefix_len())
                .table_id(route.table_id)
                .protocol(RouteProtocol::Static);
            if let Some(IpAddr::V4(gw)) = route.gateway {
                b = b.gateway(gw);
            }
            if let Some(idx) = oif {
                b = b.output_interface(idx);
            }
            if let Some(m) = route.metric {
                b = b.priority(m);
            }
            b.build()
        }
        IpNet::V6(v6) => {
            let mut b = RouteMessageBuilder::<Ipv6Addr>::new()
                .destination_prefix(v6.addr(), v6.prefix_len())
                .table_id(route.table_id)
                .protocol(RouteProtocol::Static);
            if let Some(IpAddr::V6(gw)) = route.gateway {
                b = b.gateway(gw);
            }
            if let Some(idx) = oif {
                b = b.output_interface(idx);
            }
            if let Some(m) = route.metric {
                b = b.priority(m);
            }
            b.build()
        }
    };

    handle
        .route()
        .add(msg)
        .replace()
        .execute()
        .await
        .map_err(|e| NetError::Apply(format!("add route {} failed: {e}", route.destination)))
}

async fn del_route(handle: &Handle, route: &ObservedRoute) -> Result<()> {
    let msg = match route.destination {
        IpNet::V4(v4) => {
            let mut b = RouteMessageBuilder::<Ipv4Addr>::new()
                .destination_prefix(v4.addr(), v4.prefix_len())
                .table_id(route.table_id);
            if let Some(IpAddr::V4(gw)) = route.gateway {
                b = b.gateway(gw);
            }
            if let Some(idx) = route.oif_index {
                b = b.output_interface(idx);
            }
            if let Some(m) = route.metric {
                b = b.priority(m);
            }
            b.build()
        }
        IpNet::V6(v6) => {
            let mut b = RouteMessageBuilder::<Ipv6Addr>::new()
                .destination_prefix(v6.addr(), v6.prefix_len())
                .table_id(route.table_id);
            if let Some(IpAddr::V6(gw)) = route.gateway {
                b = b.gateway(gw);
            }
            if let Some(idx) = route.oif_index {
                b = b.output_interface(idx);
            }
            if let Some(m) = route.metric {
                b = b.priority(m);
            }
            b.build()
        }
    };
    handle
        .route()
        .del(msg)
        .execute()
        .await
        .map_err(|e| NetError::Apply(format!("del route {} failed: {e}", route.destination)))
}

async fn add_rule(handle: &Handle, rule: &DesiredRule) -> Result<()> {
    let is_v6 = matches!(rule.src, Some(IpNet::V6(_))) || matches!(rule.dst, Some(IpNet::V6(_)));
    if is_v6 {
        let mut req = handle
            .rule()
            .add()
            .v6()
            .priority(rule.priority)
            .table_id(rule.table_id);
        if let Some(IpNet::V6(s)) = rule.src {
            req = req.source_prefix(s.addr(), s.prefix_len());
        }
        if let Some(IpNet::V6(d)) = rule.dst {
            req = req.destination_prefix(d.addr(), d.prefix_len());
        }
        if let Some(ref iif) = rule.iif {
            req = req.input_interface(iif);
        }
        req.execute()
            .await
            .map_err(|e| NetError::Apply(format!("add v6 rule prio {} failed: {e}", rule.priority)))
    } else {
        let mut req = handle
            .rule()
            .add()
            .v4()
            .priority(rule.priority)
            .table_id(rule.table_id);
        if let Some(IpNet::V4(s)) = rule.src {
            req = req.source_prefix(s.addr(), s.prefix_len());
        }
        if let Some(IpNet::V4(d)) = rule.dst {
            req = req.destination_prefix(d.addr(), d.prefix_len());
        }
        if let Some(ref iif) = rule.iif {
            req = req.input_interface(iif);
        }
        req.execute()
            .await
            .map_err(|e| NetError::Apply(format!("add v4 rule prio {} failed: {e}", rule.priority)))
    }
}

async fn del_rule(handle: &Handle, rule: &ObservedRule) -> Result<()> {
    let mut msg = rtnetlink::packet_route::rule::RuleMessage::default();
    msg.header.family = rule.family;
    msg.header.table = u8::try_from(rule.table_id).unwrap_or(0);
    msg.attributes.push(RuleAttribute::Priority(rule.priority));
    msg.attributes.push(RuleAttribute::Table(rule.table_id));
    if let Some(s) = rule.src {
        msg.header.src_len = s.prefix_len();
        msg.attributes.push(RuleAttribute::Source(s.addr()));
    }
    if let Some(d) = rule.dst {
        msg.header.dst_len = d.prefix_len();
        msg.attributes.push(RuleAttribute::Destination(d.addr()));
    }
    if let Some(ref iif) = rule.iif {
        msg.attributes.push(RuleAttribute::Iifname(iif.clone()));
    }
    handle
        .rule()
        .del(msg)
        .execute()
        .await
        .map_err(|e| NetError::Apply(format!("del rule prio {} failed: {e}", rule.priority)))
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
                controller: Some(10),
                kind: None,
                vlan_id: None,
                parent_index: None,
            },
            ObservedLink {
                index: 10,
                name: "br-lan".to_string(),
                controller: None,
                kind: Some("bridge".to_string()),
                vlan_id: None,
                parent_index: None,
            },
            ObservedLink {
                index: 11,
                name: "eth9".to_string(),
                controller: Some(10),
                kind: None,
                vlan_id: None,
                parent_index: None,
            },
            ObservedLink {
                index: 12,
                name: "eth1.99".to_string(),
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
