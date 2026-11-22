use crabrouter_model::{
    FirewallAction, FirewallRule, InterfacePin, InterfaceRole, RouterConfig, TransportProto,
    VlanConfig,
};

#[test]
fn default_config_is_valid() {
    let cfg = RouterConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn identical_dns_ports_are_rejected() {
    let mut cfg = RouterConfig::default();
    cfg.dns.crabdns_port = 5354;
    assert!(cfg.validate().is_err());
}

#[test]
fn validates_vlan_and_firewall_rule() {
    let mut cfg = RouterConfig::default();
    cfg.vlans.push(VlanConfig {
        parent: "eth1".to_string(),
        id: 20,
        role: InterfaceRole::Lan,
        cidr: Some("192.168.20.1/24".to_string()),
        ipv6_ra_enabled: true,
        ipv6_subnet_id: Some(1),
    });
    cfg.firewall.rules.push(FirewallRule {
        name: "allow-ssh-lan".to_string(),
        enabled: true,
        src_zone: "lan".to_string(),
        dst_zone: None,
        proto: TransportProto::Tcp,
        src_ip: Some("192.168.10.0/24".to_string()),
        dst_ip: None,
        dst_port: Some("22".to_string()),
        action: FirewallAction::Accept,
    });
    assert!(cfg.validate().is_ok());
}

#[test]
fn rejects_invalid_vlan_and_mac() {
    let mut cfg = RouterConfig::default();
    cfg.vlans.push(VlanConfig {
        parent: "eth1".to_string(),
        id: 5000,
        role: InterfaceRole::Lan,
        cidr: None,
        ipv6_ra_enabled: true,
        ipv6_subnet_id: None,
    });
    assert!(cfg.validate().is_err());

    let mut cfg2 = RouterConfig::default();
    cfg2.interface_pins.push(InterfacePin {
        mac: "not-a-mac".to_string(),
        name: "eth0".to_string(),
    });
    assert!(cfg2.validate().is_err());
}

#[test]
fn rejects_invalid_multi_wan_settings() {
    let mut cfg = RouterConfig::default();
    cfg.multi_wan.enabled = true;
    cfg.wans[0].weight = 0;
    assert!(cfg.validate().is_err());

    let mut cfg2 = RouterConfig::default();
    cfg2.wans[0].table_id = Some(254);
    assert!(cfg2.validate().is_err());
}

#[test]
fn validates_bonds_and_rejects_port_conflicts() {
    let mut cfg = RouterConfig::default();
    cfg.lans[0].ports = vec!["bond0".to_string(), "eth3".to_string()];
    cfg.bonds.push(crabrouter_model::BondConfig {
        name: "bond0".to_string(),
        slaves: vec!["eth1".to_string(), "eth2".to_string()],
        mode: crabrouter_model::BondMode::Ieee8023ad,
        xmit_hash_policy: crabrouter_model::BondXmitHashPolicy::Layer34,
        lacp_rate: crabrouter_model::BondLacpRate::Fast,
        miimon_ms: 100,
        mtu: Some(9000),
        primary: None,
    });
    assert!(cfg.validate().is_ok());

    // Slave also attached directly to LAN bridge must be rejected.
    let mut conflict = cfg.clone();
    conflict.lans[0].ports.push("eth1".to_string());
    assert!(conflict.validate().is_err());

    // Primary not in slaves must be rejected.
    let mut bad_primary = cfg;
    bad_primary.bonds[0].primary = Some("eth9".to_string());
    assert!(bad_primary.validate().is_err());
}

#[test]
fn accepts_ipv6_entries_in_xdp_blocklist() {
    let mut cfg = RouterConfig::default();
    cfg.ebpf.enabled = true;
    cfg.ebpf.xdp_guard_enabled = true;
    cfg.ebpf.xdp_blocklist = vec![
        "198.51.100.0/24".to_string(),
        "2001:db8::/32".to_string(),
        "fe80::1".to_string(),
    ];
    assert!(cfg.validate().is_ok());

    cfg.ebpf.xdp_blocklist.push("not-an-ip".to_string());
    assert!(cfg.validate().is_err());
}
