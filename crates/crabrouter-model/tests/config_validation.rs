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
