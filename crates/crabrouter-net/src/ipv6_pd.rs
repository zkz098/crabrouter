use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv6Addr};

use crabrouter_model::{InterfaceRole, Ipv6LanMode, Ipv6PdSlice, RouterConfig};
use futures_util::stream::TryStreamExt;
use ipnet::Ipv6Net;
use rtnetlink::packet_route::link::LinkAttribute;
use rtnetlink::{AddressMessageBuilder, Handle, new_connection};

use crate::{NetError, Result};

/// A downstream LAN bridge or 802.1Q VLAN interface eligible to receive a `/64` slice
/// from an upstream DHCPv6-PD (`/56` or `/60`) delegated prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownstreamIpv6Target {
    pub interface: String,
    pub label: String,
    pub subnet_id: u8,
}

/// Resolves all RA-enabled LAN bridges and LAN-role VLAN sub-interfaces into a deterministic
/// ordered list of `/64` targets with non-colliding `subnet_id` (`sla_id` in `0..=255`).
pub fn resolve_downstream_ipv6_targets(cfg: &RouterConfig) -> Vec<DownstreamIpv6Target> {
    let mut used_ids: HashSet<u8> = HashSet::new();
    for lan in &cfg.lans {
        if lan.ipv6_ra_enabled
            && lan.ipv6_mode == Ipv6LanMode::Server
            && let Some(id) = lan.ipv6_subnet_id
        {
            used_ids.insert(id);
        }
    }
    for vlan in &cfg.vlans {
        if vlan.role == InterfaceRole::Lan
            && vlan.ipv6_ra_enabled
            && let Some(id) = vlan.ipv6_subnet_id
        {
            used_ids.insert(id);
        }
    }

    let mut next_free: u8 = 0;
    let mut alloc_free_id = |used: &mut HashSet<u8>| -> Option<u8> {
        loop {
            if !used.contains(&next_free) {
                let chosen = next_free;
                used.insert(chosen);
                next_free = next_free.saturating_add(1);
                return Some(chosen);
            }
            if next_free == u8::MAX {
                return None;
            }
            next_free += 1;
        }
    };

    let mut targets = Vec::new();
    for lan in &cfg.lans {
        if !lan.ipv6_ra_enabled || lan.ipv6_mode != Ipv6LanMode::Server {
            continue;
        }
        let Some(subnet_id) = lan.ipv6_subnet_id.or_else(|| alloc_free_id(&mut used_ids)) else {
            continue;
        };
        targets.push(DownstreamIpv6Target {
            interface: lan.bridge.clone(),
            label: lan.name.clone(),
            subnet_id,
        });
    }

    for vlan in &cfg.vlans {
        if vlan.role != InterfaceRole::Lan || !vlan.ipv6_ra_enabled {
            continue;
        }
        let Some(subnet_id) = vlan.ipv6_subnet_id.or_else(|| alloc_free_id(&mut used_ids)) else {
            continue;
        };
        let ifname = format!("{}.{}", vlan.parent, vlan.id);
        targets.push(DownstreamIpv6Target {
            interface: ifname.clone(),
            label: ifname,
            subnet_id,
        });
    }

    targets
}

/// Normalizes and slices an upstream delegated IPv6 prefix (such as a `/56` or `/60` from
/// `dhcpcd` or `pppd` + `dhcpcd`) into `/64` subnets and `::1/64` router addresses for each
/// configured LAN bridge and LAN VLAN.
pub fn slice_delegated_ipv6_prefix(
    delegated_prefix: &str,
    cfg: &RouterConfig,
) -> Result<Vec<Ipv6PdSlice>> {
    let trimmed = delegated_prefix.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let normalized = if trimmed.contains('/') {
        trimmed.to_string()
    } else {
        let hint = cfg
            .wans
            .iter()
            .find(|w| w.ipv6_pd)
            .map_or(60, |w| w.ipv6_pd_prefix_len);
        format!("{trimmed}/{hint}")
    };

    let pd_net: Ipv6Net = normalized.parse().map_err(|e| {
        NetError::Apply(format!(
            "invalid delegated IPv6-PD prefix '{normalized}': {e}"
        ))
    })?;
    let prefix_len = pd_net.prefix_len();
    if !(48..=64).contains(&prefix_len) {
        return Err(NetError::Apply(format!(
            "delegated IPv6-PD prefix '{pd_net}' length /{prefix_len} must be within /48..=/64"
        )));
    }

    let max_subnets: u128 = 1u128 << (64 - prefix_len);
    let base_bits = u128::from(pd_net.network());
    let targets = resolve_downstream_ipv6_targets(cfg);

    let mut slices = Vec::new();
    for target in targets {
        let idx = u128::from(target.subnet_id);
        if idx >= max_subnets {
            tracing::warn!(
                interface = %target.interface,
                subnet_id = target.subnet_id,
                prefix = %pd_net,
                max_subnets,
                "skipping IPv6-PD /64 slice because subnet_id exceeds delegated prefix capacity"
            );
            continue;
        }
        let subnet_bits = base_bits | (idx << 64);
        let subnet_addr = Ipv6Addr::from(subnet_bits);
        let router_ip = Ipv6Addr::from(subnet_bits | 1);
        slices.push(Ipv6PdSlice {
            interface: target.interface,
            subnet_id: target.subnet_id,
            prefix_cidr: format!("{subnet_addr}/64"),
            router_addr: format!("{router_ip}/64"),
        });
    }

    Ok(slices)
}

/// Applies `/64` delegated router addresses (`<prefix>::1/64`) to downstream LAN/VLAN interfaces
/// via `rtnetlink` and removes stale addresses from a rotated or expired prefix.
pub async fn apply_ipv6_pd_slices_netlink(
    old_slices: &[Ipv6PdSlice],
    new_slices: &[Ipv6PdSlice],
) -> Result<()> {
    let (connection, handle, _) =
        new_connection().map_err(|e| NetError::Apply(format!("rtnetlink socket error: {e}")))?;
    let conn_task = tokio::spawn(connection);
    let res = apply_slices_with_handle(&handle, old_slices, new_slices).await;
    conn_task.abort();
    res
}

async fn apply_slices_with_handle(
    handle: &Handle,
    old_slices: &[Ipv6PdSlice],
    new_slices: &[Ipv6PdSlice],
) -> Result<()> {
    let name_to_idx = dump_link_indices(handle).await?;
    let new_set: HashSet<(&str, &str)> = new_slices
        .iter()
        .map(|s| (s.interface.as_str(), s.router_addr.as_str()))
        .collect();

    for old in old_slices {
        if new_set.contains(&(old.interface.as_str(), old.router_addr.as_str())) {
            continue;
        }
        let Some(&idx) = name_to_idx.get(&old.interface) else {
            continue;
        };
        let Ok(net) = old.router_addr.parse::<Ipv6Net>() else {
            continue;
        };
        let msg = AddressMessageBuilder::<Ipv6Addr>::new()
            .index(idx)
            .address(net.addr(), net.prefix_len())
            .build();
        if let Err(err) = handle.address().del(msg).execute().await {
            tracing::debug!(
                iface = %old.interface,
                addr = %old.router_addr,
                error = %err,
                "ignoring error while removing old IPv6-PD address"
            );
        }
    }

    for slice in new_slices {
        let Some(&idx) = name_to_idx.get(&slice.interface) else {
            tracing::debug!(
                iface = %slice.interface,
                addr = %slice.router_addr,
                "downstream interface not present yet for IPv6-PD slice"
            );
            continue;
        };
        let Ok(net) = slice.router_addr.parse::<Ipv6Net>() else {
            continue;
        };
        if let Err(err) = handle
            .address()
            .add(idx, IpAddr::V6(net.addr()), net.prefix_len())
            .execute()
            .await
        {
            let msg = err.to_string();
            if !msg.contains("File exists") && !msg.contains("EEXIST") {
                tracing::warn!(
                    iface = %slice.interface,
                    addr = %slice.router_addr,
                    error = %msg,
                    "failed to assign IPv6-PD /64 router address"
                );
            }
        }
    }

    Ok(())
}

async fn dump_link_indices(handle: &Handle) -> Result<HashMap<String, u32>> {
    let mut stream = handle.link().get().execute();
    let mut map = HashMap::new();
    while let Some(msg) = stream
        .try_next()
        .await
        .map_err(|e| NetError::Apply(format!("dump links failed: {e}")))?
    {
        for attr in msg.attributes {
            if let LinkAttribute::IfName(name) = attr {
                map.insert(name, msg.header.index);
                break;
            }
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabrouter_model::VlanConfig;

    #[test]
    fn slices_delegated_60_and_56_prefixes_across_lan_and_vlans() {
        let mut cfg = RouterConfig::default();
        cfg.vlans.push(VlanConfig {
            parent: "eth1".to_string(),
            id: 20,
            role: InterfaceRole::Lan,
            cidr: Some("192.168.20.1/24".to_string()),
            ipv6_ra_enabled: true,
            ipv6_subnet_id: None,
        });
        cfg.vlans.push(VlanConfig {
            parent: "eth1".to_string(),
            id: 30,
            role: InterfaceRole::Lan,
            cidr: Some("192.168.30.1/24".to_string()),
            ipv6_ra_enabled: true,
            ipv6_subnet_id: Some(5),
        });

        let slices_60 = slice_delegated_ipv6_prefix("240e:3b0:1234:5670::/60", &cfg).unwrap();
        assert_eq!(slices_60.len(), 3);
        assert_eq!(
            slices_60[0],
            Ipv6PdSlice {
                interface: "br-lan".to_string(),
                subnet_id: 0,
                prefix_cidr: "240e:3b0:1234:5670::/64".to_string(),
                router_addr: "240e:3b0:1234:5670::1/64".to_string(),
            }
        );
        assert_eq!(
            slices_60[1],
            Ipv6PdSlice {
                interface: "eth1.20".to_string(),
                subnet_id: 1,
                prefix_cidr: "240e:3b0:1234:5671::/64".to_string(),
                router_addr: "240e:3b0:1234:5671::1/64".to_string(),
            }
        );
        assert_eq!(
            slices_60[2],
            Ipv6PdSlice {
                interface: "eth1.30".to_string(),
                subnet_id: 5,
                prefix_cidr: "240e:3b0:1234:5675::/64".to_string(),
                router_addr: "240e:3b0:1234:5675::1/64".to_string(),
            }
        );

        let slices_56 = slice_delegated_ipv6_prefix("2001:db8:abcd:ef00::/56", &cfg).unwrap();
        assert_eq!(slices_56.len(), 3);
        assert_eq!(slices_56[2].prefix_cidr, "2001:db8:abcd:ef05::/64");
        assert_eq!(slices_56[2].router_addr, "2001:db8:abcd:ef05::1/64");
    }

    #[test]
    fn skips_overflowing_slices_when_upstream_only_delegates_64() {
        let mut cfg = RouterConfig::default();
        cfg.vlans.push(VlanConfig {
            parent: "eth1".to_string(),
            id: 20,
            role: InterfaceRole::Lan,
            cidr: Some("192.168.20.1/24".to_string()),
            ipv6_ra_enabled: true,
            ipv6_subnet_id: None,
        });
        let slices = slice_delegated_ipv6_prefix("2001:db8:1:2::/64", &cfg).unwrap();
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].interface, "br-lan");
        assert_eq!(slices[0].router_addr, "2001:db8:1:2::1/64");
    }
}
