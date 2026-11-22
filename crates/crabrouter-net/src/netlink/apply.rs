use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crabrouter_model::RouterConfig;
use futures_util::stream::TryStreamExt;
use ipnet::IpNet;
use rtnetlink::packet_route::AddressFamily;
use rtnetlink::packet_route::address::AddressAttribute;
use rtnetlink::packet_route::link::{InfoData, InfoKind, InfoVlan, LinkAttribute, LinkInfo};
use rtnetlink::packet_route::route::{RouteAddress, RouteAttribute, RouteProtocol};
use rtnetlink::packet_route::rule::RuleAttribute;
use rtnetlink::{
    AddressMessageBuilder, Handle, IpVersion, LinkBridge, LinkUnspec, LinkVlan,
    RouteMessageBuilder, new_connection,
};

use super::{
    DesiredRoute, DesiredRule, NetworkDiffPlan, ObservedAddr, ObservedLink, ObservedRoute,
    ObservedRule, compute_network_diff_plan,
};
use crate::{NetError, Result};

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

fn parse_link_infos(infos: Vec<LinkInfo>) -> (Option<String>, Option<u16>) {
    let mut kind = None;
    let mut vlan_id = None;
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
                    if let InfoVlan::Id(id) = va {
                        vlan_id = Some(id);
                    }
                }
            }
            _ => {}
        }
    }
    (kind, vlan_id)
}

fn parse_observed_link(index: u32, attributes: Vec<LinkAttribute>) -> Option<ObservedLink> {
    let mut name = None;
    let mut controller = None;
    let mut parent_index = None;
    let mut kind = None;
    let mut vlan_id = None;

    for attr in attributes {
        match attr {
            LinkAttribute::IfName(n) => name = Some(n),
            LinkAttribute::Controller(idx) if idx > 0 => controller = Some(idx),
            LinkAttribute::Link(idx) if idx > 0 => parent_index = Some(idx),
            LinkAttribute::LinkInfo(infos) => {
                let (k, vid) = parse_link_infos(infos);
                if k.is_some() {
                    kind = k;
                }
                if vid.is_some() {
                    vlan_id = vid;
                }
            }
            _ => {}
        }
    }

    Some(ObservedLink {
        index,
        name: name?,
        controller,
        kind,
        vlan_id,
        parent_index,
    })
}

async fn dump_links(handle: &Handle) -> Result<Vec<ObservedLink>> {
    let mut stream = handle.link().get().execute();
    let mut out = Vec::new();
    while let Some(msg) = stream
        .try_next()
        .await
        .map_err(|e| NetError::Apply(format!("dump links failed: {e}")))?
    {
        if let Some(link) = parse_observed_link(msg.header.index, msg.attributes) {
            out.push(link);
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
