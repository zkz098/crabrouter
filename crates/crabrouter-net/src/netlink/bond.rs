use std::collections::{HashMap, HashSet};

use crabrouter_model::{BondConfig, BondLacpRate, BondMode, BondXmitHashPolicy, RouterConfig};
use rtnetlink::packet_route::link::{
    BondLacpRate as RtBondLacpRate, BondMode as RtBondMode,
    BondXmitHashPolicy as RtBondXmitHashPolicy, BridgeStpState,
};
use rtnetlink::{Handle, LinkBond, LinkBridge, LinkUnspec};

use super::{NetworkDiffPlan, ObservedLink};
use crate::{NetError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BondCreateSpec {
    pub name: String,
    pub mode: BondMode,
    pub xmit_hash_policy: BondXmitHashPolicy,
    pub lacp_rate: BondLacpRate,
    pub miimon_ms: u32,
    pub mtu: Option<u32>,
    pub primary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BondSlaveSpec {
    pub slave: String,
    pub bond: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeStpSpec {
    pub bridge: String,
    pub stp_enabled: bool,
    pub forward_delay_cs: u32,
}

pub(super) fn plan_bonds(
    cfg: &RouterConfig,
    links: &[ObservedLink],
    link_by_name: &HashMap<&str, &ObservedLink>,
    plan: &mut NetworkDiffPlan,
) {
    let mut managed_bond_indices = HashMap::new();
    let mut desired_slave_to_bond: HashMap<&str, &str> = HashMap::new();
    let desired_bond_names: HashSet<&str> = cfg.bonds.iter().map(|b| b.name.as_str()).collect();

    for bond in &cfg.bonds {
        if let Some(existing) = link_by_name.get(bond.name.as_str()) {
            managed_bond_indices.insert(existing.index, bond.name.as_str());
        } else {
            plan.bonds_to_create.push(BondCreateSpec {
                name: bond.name.clone(),
                mode: bond.mode,
                xmit_hash_policy: bond.xmit_hash_policy,
                lacp_rate: bond.lacp_rate,
                miimon_ms: bond.miimon_ms,
                mtu: bond.mtu,
                primary: bond.primary.clone(),
            });
        }
        plan.links_to_up_mtu.push((bond.name.clone(), bond.mtu));

        for slave in &bond.slaves {
            desired_slave_to_bond.insert(slave.as_str(), bond.name.as_str());
            plan.links_to_up_mtu.push((slave.clone(), bond.mtu));
            let already_attached = link_by_name
                .get(slave.as_str())
                .and_then(|l| l.controller)
                .is_some_and(|ctrl_idx| {
                    managed_bond_indices.get(&ctrl_idx) == Some(&bond.name.as_str())
                });
            if !already_attached {
                plan.bond_slaves_to_attach.push(BondSlaveSpec {
                    slave: slave.clone(),
                    bond: bond.name.clone(),
                });
            }
        }
    }

    for link in links {
        if let Some(ctrl_idx) = link.controller
            && managed_bond_indices.contains_key(&ctrl_idx)
            && !desired_slave_to_bond.contains_key(link.name.as_str())
        {
            plan.bond_slaves_to_detach.push(link.name.clone());
        }
        if link.kind.as_deref() == Some("bond") && !desired_bond_names.contains(link.name.as_str())
        {
            plan.bonds_to_delete.push(link.index);
        }
    }
}

pub(super) fn plan_bridge_stp(cfg: &RouterConfig, plan: &mut NetworkDiffPlan) {
    for lan in &cfg.lans {
        plan.bridges_stp_to_set.push(BridgeStpSpec {
            bridge: lan.bridge.clone(),
            stp_enabled: lan.stp_enabled,
            forward_delay_cs: lan.forward_delay_cs,
        });
    }
}

pub(super) async fn apply_bond_detach_and_delete(
    handle: &Handle,
    plan: &NetworkDiffPlan,
) -> Result<()> {
    for slave in &plan.bond_slaves_to_detach {
        handle
            .link()
            .set(LinkUnspec::new_with_name(slave).controller(0).build())
            .execute()
            .await
            .map_err(|e| NetError::Apply(format!("detach bond slave '{slave}' failed: {e}")))?;
    }

    for bond_idx in &plan.bonds_to_delete {
        handle
            .link()
            .del(*bond_idx)
            .execute()
            .await
            .map_err(|e| NetError::Apply(format!("delete bond link {bond_idx} failed: {e}")))?;
    }
    Ok(())
}

pub(super) async fn apply_bond_create_and_attach(
    handle: &Handle,
    plan: &NetworkDiffPlan,
    name_to_index: &mut HashMap<String, u32>,
) -> Result<()> {
    for spec in &plan.bonds_to_create {
        let mut builder = LinkBond::new(&spec.name)
            .mode(map_bond_mode(spec.mode))
            .miimon(spec.miimon_ms)
            .xmit_hash_policy(map_xmit_hash(spec.xmit_hash_policy))
            .up();
        if spec.mode == BondMode::Ieee8023ad {
            builder = builder.ad_lacp_rate(map_lacp_rate(spec.lacp_rate));
        }
        if let Some(mtu) = spec.mtu {
            builder = builder.mtu(mtu);
        }
        handle
            .link()
            .add(builder.build())
            .execute()
            .await
            .map_err(|e| NetError::Apply(format!("create bond '{}' failed: {e}", spec.name)))?;
    }

    if !plan.bonds_to_create.is_empty() {
        *name_to_index = super::apply::refresh_name_to_index(handle).await?;
    }

    for spec in &plan.bond_slaves_to_attach {
        let Some(&bond_idx) = name_to_index.get(&spec.bond) else {
            return Err(NetError::Apply(format!(
                "bond '{}' missing after creation",
                spec.bond
            )));
        };
        if !name_to_index.contains_key(&spec.slave) {
            tracing::warn!(slave = %spec.slave, bond = %spec.bond, "bond slave interface not found on host");
            continue;
        }
        // Kernel requires slave interface to be administratively DOWN before enslaving to a bond.
        let _ = handle
            .link()
            .set(LinkUnspec::new_with_name(&spec.slave).down().build())
            .execute()
            .await;
        handle
            .link()
            .set(
                LinkUnspec::new_with_name(&spec.slave)
                    .controller(bond_idx)
                    .up()
                    .build(),
            )
            .execute()
            .await
            .map_err(|e| {
                NetError::Apply(format!(
                    "attach slave '{}' to bond '{}' failed: {e}",
                    spec.slave, spec.bond
                ))
            })?;
    }

    for spec in &plan.bonds_to_create {
        if let Some(primary) = spec.primary.as_deref()
            && let (Some(&bond_idx), Some(&primary_idx)) =
                (name_to_index.get(&spec.name), name_to_index.get(primary))
        {
            let msg = LinkBond::new(&spec.name)
                .index(bond_idx)
                .primary(primary_idx)
                .build();
            let _ = handle.link().set(msg).execute().await;
        }
    }
    Ok(())
}

pub(super) async fn apply_bridge_stp(
    handle: &Handle,
    plan: &NetworkDiffPlan,
    name_to_index: &HashMap<String, u32>,
) -> Result<()> {
    for spec in &plan.bridges_stp_to_set {
        let Some(&bridge_idx) = name_to_index.get(&spec.bridge) else {
            continue;
        };
        let stp_state = if spec.stp_enabled {
            BridgeStpState::KernelStp
        } else {
            BridgeStpState::Disabled
        };
        let msg = LinkBridge::new(&spec.bridge)
            .index(bridge_idx)
            .stp_state(stp_state)
            .forward_delay(spec.forward_delay_cs)
            .build();
        handle.link().set(msg).execute().await.map_err(|e| {
            NetError::Apply(format!(
                "configure bridge '{}' STP failed: {e}",
                spec.bridge
            ))
        })?;
    }
    Ok(())
}

const fn map_bond_mode(mode: BondMode) -> RtBondMode {
    match mode {
        BondMode::BalanceRr => RtBondMode::BalanceRr,
        BondMode::ActiveBackup => RtBondMode::ActiveBackup,
        BondMode::BalanceXor => RtBondMode::BalanceXor,
        BondMode::Broadcast => RtBondMode::Broadcast,
        BondMode::Ieee8023ad => RtBondMode::Ieee8023Ad,
        BondMode::BalanceTlb => RtBondMode::BalanceTlb,
        BondMode::BalanceAlb => RtBondMode::BalanceAlb,
    }
}

const fn map_xmit_hash(policy: BondXmitHashPolicy) -> RtBondXmitHashPolicy {
    match policy {
        BondXmitHashPolicy::Layer2 => RtBondXmitHashPolicy::Layer2,
        BondXmitHashPolicy::Layer23 => RtBondXmitHashPolicy::Layer23,
        BondXmitHashPolicy::Layer34 => RtBondXmitHashPolicy::Layer34,
        BondXmitHashPolicy::Encap23 => RtBondXmitHashPolicy::Encap23,
        BondXmitHashPolicy::Encap34 => RtBondXmitHashPolicy::Encap34,
    }
}

const fn map_lacp_rate(rate: BondLacpRate) -> RtBondLacpRate {
    match rate {
        BondLacpRate::Slow => RtBondLacpRate::Slow,
        BondLacpRate::Fast => RtBondLacpRate::Fast,
    }
}

/// Helper for tests / diagnostics to describe a `BondConfig`.
pub fn describe_bond(bond: &BondConfig) -> String {
    format!("{} ({:?}, slaves={:?})", bond.name, bond.mode, bond.slaves)
}

#[cfg(test)]
mod tests {
    use crabrouter_model::{BondConfig, BondLacpRate, BondMode, BondXmitHashPolicy, RouterConfig};

    use super::super::{ObservedLink, compute_network_diff_plan};

    #[test]
    fn plans_bond_creation_slave_attachment_and_bridge_stp() {
        let mut cfg = RouterConfig::default();
        cfg.bonds.push(BondConfig {
            name: "bond0".to_string(),
            slaves: vec!["eth1".to_string(), "eth2".to_string()],
            mode: BondMode::Ieee8023ad,
            xmit_hash_policy: BondXmitHashPolicy::Layer34,
            lacp_rate: BondLacpRate::Fast,
            miimon_ms: 100,
            mtu: Some(9000),
            primary: None,
        });
        cfg.lans[0].ports = vec!["bond0".to_string(), "eth3".to_string()];
        cfg.lans[0].stp_enabled = true;
        cfg.lans[0].forward_delay_cs = 200;

        let links = vec![
            ObservedLink {
                index: 2,
                name: "eth1".to_string(),
                mac: None,
                controller: None,
                kind: None,
                vlan_id: None,
                parent_index: None,
            },
            ObservedLink {
                index: 3,
                name: "eth2".to_string(),
                mac: None,
                controller: None,
                kind: None,
                vlan_id: None,
                parent_index: None,
            },
        ];

        let plan = compute_network_diff_plan(&cfg, &links, &[], &[], &[]);
        assert_eq!(plan.bonds_to_create.len(), 1);
        assert_eq!(plan.bonds_to_create[0].name, "bond0");
        assert_eq!(plan.bond_slaves_to_attach.len(), 2);
        assert_eq!(plan.bridges_stp_to_set.len(), 1);
        assert!(plan.bridges_stp_to_set[0].stp_enabled);
        assert_eq!(plan.bridges_stp_to_set[0].forward_delay_cs, 200);
    }

    #[test]
    fn plans_bond_deletion_when_removed_from_config() {
        let cfg = RouterConfig::default();
        let links = vec![ObservedLink {
            index: 99,
            name: "bond0".to_string(),
            mac: None,
            controller: None,
            kind: Some("bond".to_string()),
            vlan_id: None,
            parent_index: None,
        }];
        let plan = compute_network_diff_plan(&cfg, &links, &[], &[], &[]);
        assert_eq!(plan.bonds_to_delete, vec![99]);
    }
}
