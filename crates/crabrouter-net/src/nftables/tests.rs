use std::collections::HashSet;

use crabrouter_model::{
    FirewallAction, FirewallRule, Ipv6LanMode, MultiWanMode, PortForwardRule, RouterConfig,
    TransportProto,
};

use super::{render_nftables_ruleset, render_nftables_ruleset_with_options};

#[test]
fn renders_flowtable_syn_flood_and_masquerade() {
    let cfg = RouterConfig::default();
    let nft = render_nftables_ruleset(&cfg);
    assert!(nft.contains("flowtable f_fastpath"));
    assert!(nft.contains("oifname { \"eth0\", \"ppp0\" } masquerade"));
    assert!(nft.contains("tcp flags syn tcp option maxseg size set rt mtu"));
    assert!(nft.contains("meter syn_flood_input"));
    assert!(!nft.contains("container-dns"));
    assert!(!nft.contains("container-intra"));
}

#[test]
fn renders_multi_wan_weighted_mangle_rules() {
    let mut cfg = RouterConfig::default();
    let mut wan2 = cfg.wans[0].clone();
    wan2.name = "wan2".to_string();
    wan2.device = "eth4".to_string();
    wan2.weight = 3;
    cfg.wans[0].weight = 1;
    cfg.wans.push(wan2);
    cfg.multi_wan.enabled = true;
    cfg.multi_wan.mode = MultiWanMode::LoadBalance;

    let nft = render_nftables_ruleset(&cfg);
    assert!(nft.contains("chain mangle_prerouting"));
    assert!(nft.contains("ct mark != 0 meta mark set ct mark return"));
    assert!(
        nft.contains("jhash ip saddr mod 4 map { 0 : 0x1065, 1 : 0x1066, 2 : 0x1066, 3 : 0x1066 }")
    );
    assert!(nft.contains("ct state new ct mark set meta mark"));
}

#[test]
fn filters_missing_flowtable_devices_and_renders_acl_rules() {
    let mut cfg = RouterConfig::default();
    cfg.firewall.rules.push(FirewallRule {
        name: "allow-https".to_string(),
        enabled: true,
        src_zone: "wan".to_string(),
        dst_zone: None,
        proto: TransportProto::Tcp,
        src_ip: None,
        dst_ip: None,
        dst_port: Some("443".to_string()),
        action: FirewallAction::Accept,
    });
    let live: HashSet<String> = ["br-lan".to_string()].into_iter().collect();
    let nft = render_nftables_ruleset_with_options(&cfg, Some(&live), true);
    assert!(nft.contains("devices = { \"br-lan\" }"));
    assert!(!nft.contains("devices = { \"eth0\""));
    assert!(nft.contains("tcp dport 443 accept comment \"acl:allow-https\""));
}

#[test]
fn renders_nat_loopback_upnp_and_ipv6_relay_rules() {
    let mut cfg = RouterConfig::default();
    cfg.upnp.enabled = true;
    cfg.lans[0].ipv6_mode = Ipv6LanMode::Relay;
    cfg.firewall.port_forwards.push(PortForwardRule {
        name: "web-srv".to_string(),
        enabled: true,
        proto: TransportProto::Tcp,
        src_zone: "wan".to_string(),
        external_port: 8443,
        dest_ip: "192.168.10.20".to_string(),
        internal_port: 443,
        reflection: true,
    });
    let nft = render_nftables_ruleset(&cfg);
    assert!(nft.contains("udp dport { 1900, 5351 } accept comment \"upnp-natpmp-pcp\""));
    assert!(nft.contains("meta l4proto ipv6-icmp accept comment \"ipv6-relay-ndp\""));
    assert!(nft.contains(
        "iifname \"br-lan\" ip daddr != 192.168.10.1 fib daddr type local tcp dport 8443 dnat to 192.168.10.20:443 comment \"hairpin:web-srv\""
    ));
    assert!(nft.contains(
        "meta nfproto ipv4 ct status dnat oifname \"br-lan\" ip saddr 192.168.10.0/24 ip daddr 192.168.10.20 tcp dport 443 masquerade comment \"hairpin:web-srv\""
    ));
}

#[test]
fn renders_container_firewall_and_multi_wan_rules_when_enabled() {
    let mut cfg = RouterConfig::default();
    cfg.containers.enabled = true;
    cfg.multi_wan.enabled = true;

    let nft = render_nftables_ruleset(&cfg);
    assert!(nft.contains(
        "iifname \"podman*\" meta l4proto { tcp, udp } th dport { 53, 5355 } accept comment \"container-dns\""
    ));
    assert!(
        nft.contains("iifname \"podman*\" oifname \"podman*\" accept comment \"container-intra\"")
    );
    assert!(nft.contains(
        "iifname \"podman*\" oifname { \"eth0\", \"ppp0\" } accept comment \"container-to-wan\""
    ));
    assert!(!nft.contains("container-to-lan"));
    assert!(nft.contains("iifname != \"br-lan\" return"));
    assert!(nft.contains("ip daddr 10.88.0.0/16 return"));

    cfg.containers.allow_lan_access = true;
    cfg.containers.multi_wan_enabled = true;
    let nft_lan = render_nftables_ruleset(&cfg);
    assert!(
        nft_lan
            .contains("iifname \"podman*\" oifname \"br-lan\" accept comment \"container-to-lan\"")
    );
    assert!(nft_lan.contains("iifname != { \"br-lan\", \"podman*\" } return"));
}

#[test]
fn renders_ipv6_suffix_acl_nat66_nptv6_and_modem_bridge() {
    use crabrouter_model::{FirewallAction, FirewallRule, Ipv6NatMode, TransportProto};

    let mut cfg = RouterConfig::default();
    cfg.wans[0].modem_cidr = Some("192.168.1.2/24".to_string());
    cfg.firewall.ipv6_nat_mode = Ipv6NatMode::Masquerade;
    cfg.firewall.rules.push(FirewallRule {
        name: "allow-nas-v6".to_string(),
        enabled: true,
        src_zone: "wan".to_string(),
        dst_zone: Some("lan".to_string()),
        proto: TransportProto::Tcp,
        src_ip: None,
        dst_ip: Some("::1234/::ffff:ffff:ffff:ffff".to_string()),
        dst_port: Some("443".to_string()),
        action: FirewallAction::Accept,
    });

    let nft = render_nftables_ruleset(&cfg);
    assert!(nft.contains("ip6 daddr & ::ffff:ffff:ffff:ffff == ::1234"));
    assert!(
        nft.contains(
            "meta nfproto ipv6 oifname { \"eth0\", \"ppp0\" } masquerade comment \"nat66\""
        )
    );
    assert!(nft.contains("meta nfproto ipv4 oifname \"eth0\" ip daddr 192.168.1.0/24 masquerade comment \"modem-bridge:wan\""));

    cfg.firewall.ipv6_nat_mode = Ipv6NatMode::Nptv6;
    cfg.firewall.nptv6_internal_prefix = Some("fd00:10::/64".to_string());
    cfg.firewall.nptv6_external_prefix = Some("2001:db8:1::/64".to_string());
    let nft_npt = render_nftables_ruleset(&cfg);
    assert!(nft_npt.contains(
        "ip6 daddr 2001:db8:1::/64 dnat ip6 prefix to fd00:10::/64 comment \"nptv6-in\""
    ));
    assert!(nft_npt.contains(
        "ip6 saddr fd00:10::/64 snat ip6 prefix to 2001:db8:1::/64 comment \"nptv6-out\""
    ));
}
