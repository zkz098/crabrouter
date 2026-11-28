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
