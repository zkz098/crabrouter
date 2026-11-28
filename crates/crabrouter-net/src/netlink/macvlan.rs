use std::collections::{HashMap, HashSet};

use crabrouter_model::RouterConfig;
use rtnetlink::packet_route::link::MacVlanMode;
use rtnetlink::{Handle, LinkMacVlan};

use super::{NetworkDiffPlan, ObservedLink};
use crate::{NetError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacvlanCreateSpec {
    pub name: String,
    pub parent: String,
}

pub(super) fn plan_macvlans(
    cfg: &RouterConfig,
    links: &[ObservedLink],
    link_by_name: &HashMap<&str, &ObservedLink>,
    plan: &mut NetworkDiffPlan,
) {
    let mut desired_names = HashSet::new();
    let mut configured_parents = HashSet::new();

    for wan in &cfg.wans {
        let Some(parent) = wan
            .macvlan_parent
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        desired_names.insert(wan.device.as_str());
        configured_parents.insert(parent);
        plan.links_to_up_mtu.push((parent.to_string(), None));

        if !link_by_name.contains_key(wan.device.as_str()) {
            plan.macvlans_to_create.push(MacvlanCreateSpec {
                name: wan.device.clone(),
                parent: parent.to_string(),
            });
        }
    }

    for link in links {
        if link.kind.as_deref() == Some("macvlan")
            && !desired_names.contains(link.name.as_str())
            && !configured_parents.is_empty()
        {
            plan.macvlans_to_delete.push(link.index);
        }
    }
}

pub(super) async fn apply_macvlan_delete(handle: &Handle, plan: &NetworkDiffPlan) -> Result<()> {
    for idx in &plan.macvlans_to_delete {
        handle
            .link()
            .del(*idx)
            .execute()
            .await
            .map_err(|e| NetError::Apply(format!("delete macvlan link {idx} failed: {e}")))?;
    }
    Ok(())
}

pub(super) async fn apply_macvlan_create(
    handle: &Handle,
    plan: &NetworkDiffPlan,
    name_to_index: &mut HashMap<String, u32>,
) -> Result<()> {
    for spec in &plan.macvlans_to_create {
        let Some(&parent_idx) = name_to_index.get(&spec.parent) else {
            return Err(NetError::Apply(format!(
                "macvlan parent interface '{}' does not exist",
                spec.parent
            )));
        };
        handle
            .link()
            .add(
                LinkMacVlan::new(&spec.name, parent_idx, MacVlanMode::Bridge)
                    .up()
                    .build(),
            )
            .execute()
            .await
            .map_err(|e| NetError::Apply(format!("create macvlan '{}' failed: {e}", spec.name)))?;
    }
    if !plan.macvlans_to_create.is_empty() {
        *name_to_index = super::apply::refresh_name_to_index(handle).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netlink::compute_network_diff_plan;

    #[test]
    fn plans_macvlan_for_single_line_multi_dial() {
        let mut cfg = RouterConfig::default();
        cfg.wans[0].device = "vwan0".to_string();
        cfg.wans[0].macvlan_parent = Some("eth0".to_string());
        let mut wan2 = cfg.wans[0].clone();
        wan2.name = "wan2".to_string();
        wan2.device = "vwan1".to_string();
        wan2.metric = 110;
        cfg.wans.push(wan2);

        let links = vec![ObservedLink {
            index: 2,
            name: "eth0".to_string(),
            kind: None,
            mac: None,
            vlan_id: None,
            controller: None,
            parent_index: None,
        }];
        let plan = compute_network_diff_plan(&cfg, &links, &[], &[], &[]);
        assert_eq!(plan.macvlans_to_create.len(), 2);
        assert_eq!(plan.macvlans_to_create[0].name, "vwan0");
        assert_eq!(plan.macvlans_to_create[0].parent, "eth0");
        assert_eq!(plan.macvlans_to_create[1].name, "vwan1");
    }
}
