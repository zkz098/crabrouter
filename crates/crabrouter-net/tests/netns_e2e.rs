use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use crabrouter_model::{
    InterfaceRole, MultiWanConfig, MultiWanMode, RouterConfig, VlanConfig, WanConfig, WanProto,
    WanRuntimeState,
};
use crabrouter_net::{
    WanStateObserver, apply_ipv6_pd_slices_netlink, apply_nft_with_precheck_and_multi_wan,
    apply_rtnetlink_diff_with_wans, slice_delegated_ipv6_prefix, spawn_multi_wan_engine,
    spawn_network_event_engine,
};

struct TestObserver {
    cfg: RouterConfig,
}

impl WanStateObserver for TestObserver {
    fn current_config(&self) -> RouterConfig {
        self.cfg.clone()
    }

    fn on_wan_state_updated(&self, _state: WanRuntimeState) {}
}

struct PeerNs {
    child: Child,
    pid: u32,
    pid_file: PathBuf,
}

impl Drop for PeerNs {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.pid_file);
    }
}

fn spawn_peer_netns(name: &str) -> PeerNs {
    let pid_file = std::env::temp_dir().join(format!(
        "crabrouter-netns-{}-{}-{name}.pid",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let _ = fs::remove_file(&pid_file);

    let cmd = format!(
        "echo $$ > '{}'; ip link set lo up; exec sleep 60",
        pid_file.display()
    );
    let child = Command::new("unshare")
        .args(["--net", "bash", "-c", &cmd])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn peer netns");

    let mut peer = PeerNs {
        child,
        pid: 0,
        pid_file: pid_file.clone(),
    };

    for _ in 0..100 {
        if let Ok(raw) = fs::read_to_string(&pid_file)
            && let Ok(pid) = raw.trim().parse::<u32>()
        {
            peer.pid = pid;
            return peer;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for peer netns '{name}' pid file");
}

fn run_cmd(prog: &str, args: &[&str]) {
    let out = Command::new(prog)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {prog} {args:?}: {e}"));
    assert!(
        out.status.success(),
        "command `{prog} {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn ns_exec(pid: u32, args: &[&str]) {
    let pid_s = pid.to_string();
    let mut full = vec!["-t", &pid_s, "-n"];
    full.extend_from_slice(args);
    run_cmd("nsenter", &full);
}

fn build_multi_wan_test_config() -> RouterConfig {
    let mut cfg = RouterConfig::default();
    cfg.firewall.flowtable_offload = false;
    cfg.lans[0].ports = vec!["eth1".to_string()];
    cfg.lans[0].cidr = "192.168.10.1/24".to_string();

    cfg.wans = vec![
        WanConfig {
            name: "wan".to_string(),
            device: "eth0".to_string(),
            proto: WanProto::Static,
            static_cidr: Some("10.10.1.2/24".to_string()),
            gateway: Some("10.10.1.1".to_string()),
            pppoe_username: None,
            pppoe_password: None,
            mtu: 1500,
            ipv6_pd: false,
            ipv6_pd_prefix_len: 60,
            metric: 100,
            weight: 1,
            table_id: Some(101),
            health_check_enabled: true,
            probe_targets: vec!["198.51.100.1".to_string()],
            mac_override: None,
            modem_cidr: None,
            pppoe_lcp_interval: None,
            pppoe_lcp_failure: None,
        },
        WanConfig {
            name: "wan2".to_string(),
            device: "eth4".to_string(),
            proto: WanProto::Static,
            static_cidr: Some("10.10.2.2/24".to_string()),
            gateway: Some("10.10.2.1".to_string()),
            pppoe_username: None,
            pppoe_password: None,
            mtu: 1500,
            ipv6_pd: false,
            ipv6_pd_prefix_len: 60,
            metric: 200,
            weight: 1,
            table_id: Some(102),
            health_check_enabled: true,
            probe_targets: vec!["203.0.113.1".to_string()],
            mac_override: None,
            modem_cidr: None,
            pppoe_lcp_interval: None,
            pppoe_lcp_failure: None,
        },
    ];
    for zone in &mut cfg.firewall.zones {
        if zone.name == "wan" {
            zone.interfaces = vec!["eth0".to_string(), "eth4".to_string()];
        }
    }
    cfg.multi_wan = MultiWanConfig {
        enabled: true,
        mode: MultiWanMode::Failover,
        probe_interval_secs: 60,
        probe_timeout_ms: 1000,
        fail_threshold: 2,
        recover_threshold: 1,
        flush_conntrack_on_failover: false,
        ..MultiWanConfig::default()
    };
    cfg
}

fn setup_veth_topology() -> (PeerNs, PeerNs, PeerNs) {
    fs::write("/proc/sys/net/ipv4/ip_forward", "1").expect("enable ip_forward");
    run_cmd("ip", &["link", "set", "lo", "up"]);

    let wan1 = spawn_peer_netns("wan1");
    let wan2 = spawn_peer_netns("wan2");
    let lan = spawn_peer_netns("lan");

    run_cmd(
        "ip",
        &[
            "link",
            "add",
            "eth0",
            "type",
            "veth",
            "peer",
            "name",
            "wan1-peer",
        ],
    );
    run_cmd(
        "ip",
        &[
            "link",
            "add",
            "eth4",
            "type",
            "veth",
            "peer",
            "name",
            "wan2-peer",
        ],
    );
    run_cmd(
        "ip",
        &[
            "link", "add", "eth1", "type", "veth", "peer", "name", "lan-peer",
        ],
    );

    let wan1_pid_s = wan1.pid.to_string();
    let wan2_pid_s = wan2.pid.to_string();
    let lan_pid_s = lan.pid.to_string();
    run_cmd("ip", &["link", "set", "wan1-peer", "netns", &wan1_pid_s]);
    run_cmd("ip", &["link", "set", "wan2-peer", "netns", &wan2_pid_s]);
    run_cmd("ip", &["link", "set", "lan-peer", "netns", &lan_pid_s]);

    ns_exec(
        wan1.pid,
        &["ip", "addr", "add", "10.10.1.1/24", "dev", "wan1-peer"],
    );
    ns_exec(
        wan1.pid,
        &["ip", "addr", "add", "198.51.100.1/32", "dev", "lo"],
    );
    ns_exec(wan1.pid, &["ip", "link", "set", "wan1-peer", "up"]);

    ns_exec(
        wan2.pid,
        &["ip", "addr", "add", "10.10.2.1/24", "dev", "wan2-peer"],
    );
    ns_exec(
        wan2.pid,
        &["ip", "addr", "add", "203.0.113.1/32", "dev", "lo"],
    );
    ns_exec(wan2.pid, &["ip", "link", "set", "wan2-peer", "up"]);

    ns_exec(
        lan.pid,
        &["ip", "addr", "add", "192.168.10.50/24", "dev", "lan-peer"],
    );
    ns_exec(lan.pid, &["ip", "link", "set", "lan-peer", "up"]);
    ns_exec(
        lan.pid,
        &["ip", "route", "add", "default", "via", "192.168.10.1"],
    );

    run_cmd("ip", &["link", "set", "eth0", "up"]);
    run_cmd("ip", &["link", "set", "eth4", "up"]);
    run_cmd("ip", &["link", "set", "eth1", "up"]);

    (wan1, wan2, lan)
}

#[tokio::test]
async fn netns_veth_multi_wan_nat_and_failover_e2e() {
    if std::env::var_os("CRABROUTER_IN_NETNS").is_none() {
        let exe = std::env::current_exe().expect("current_exe");
        let out = Command::new("unshare")
            .args([
                "--user",
                "--net",
                "--map-root-user",
                exe.to_str().expect("utf8 exe"),
                "netns_veth_multi_wan_nat_and_failover_e2e",
                "--exact",
                "--nocapture",
            ])
            .env("CRABROUTER_IN_NETNS", "1")
            .output();
        match out {
            Ok(res) => {
                assert!(
                    res.status.success(),
                    "netns_e2e inner failed:\nstdout:\n{}\nstderr:\n{}",
                    String::from_utf8_lossy(&res.stdout),
                    String::from_utf8_lossy(&res.stderr)
                );
            }
            Err(err) => {
                eprintln!("skipping netns_e2e (unshare unavailable: {err})");
            }
        }
        return;
    }

    let (wan1, _wan2, lan) = setup_veth_topology();
    let cfg = build_multi_wan_test_config();
    apply_rtnetlink_diff_with_wans(&cfg, &[], &["wan".to_string()])
        .await
        .expect("apply rtnetlink diff");
    apply_nft_with_precheck_and_multi_wan(&cfg, Some(&["wan".to_string()]))
        .await
        .expect("apply nftables");

    // Verify LAN -> WAN1 NAT forwarding works through real kernel netfilter + rtnetlink
    ns_exec(lan.pid, &["ping", "-c", "2", "-W", "2", "198.51.100.1"]);

    // Start MultiWanManager and verify initial healthy state selects wan
    let observer: Arc<dyn WanStateObserver> = Arc::new(TestObserver { cfg: cfg.clone() });
    let wan_bus = spawn_network_event_engine(Arc::clone(&observer), false);
    let multi_wan = spawn_multi_wan_engine(observer, wan_bus, false);

    let st1 = multi_wan.run_probe_cycle().await;
    assert_eq!(st1.active_wans, vec!["wan"]);

    // Simulate WAN1 upstream failure by removing its probe target IP
    ns_exec(
        wan1.pid,
        &["ip", "addr", "del", "198.51.100.1/32", "dev", "lo"],
    );
    let _ = multi_wan.run_probe_cycle().await;
    let st2 = multi_wan.run_probe_cycle().await;
    assert_eq!(st2.active_wans, vec!["wan2"]);
    assert_eq!(st2.failover_count, 1);

    // Verify LAN -> WAN2 NAT forwarding works after failover!
    ns_exec(lan.pid, &["ping", "-c", "2", "-W", "2", "203.0.113.1"]);
}

#[tokio::test]
async fn netns_veth_vlan_ipv6_pd_and_load_balance_e2e() {
    if std::env::var_os("CRABROUTER_IN_NETNS").is_none() {
        let exe = std::env::current_exe().expect("current_exe");
        let out = Command::new("unshare")
            .args([
                "--user",
                "--net",
                "--map-root-user",
                exe.to_str().expect("utf8 exe"),
                "netns_veth_vlan_ipv6_pd_and_load_balance_e2e",
                "--exact",
                "--nocapture",
            ])
            .env("CRABROUTER_IN_NETNS", "1")
            .output();
        if let Ok(res) = out {
            assert!(
                res.status.success(),
                "netns_e2e vlan/ipv6-pd inner failed:\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&res.stdout),
                String::from_utf8_lossy(&res.stderr)
            );
        }
        return;
    }

    let (_wan1, _wan2, lan) = setup_veth_topology();
    let mut cfg = build_multi_wan_test_config();
    cfg.multi_wan.mode = MultiWanMode::LoadBalance;
    cfg.wans[0].weight = 2;
    cfg.wans[1].weight = 1;
    cfg.vlans.push(VlanConfig {
        parent: "eth1".to_string(),
        id: 20,
        role: InterfaceRole::Lan,
        cidr: Some("192.168.20.1/24".to_string()),
        ipv6_ra_enabled: true,
        ipv6_subnet_id: Some(1),
    });

    apply_rtnetlink_diff_with_wans(&cfg, &[], &["wan".to_string(), "wan2".to_string()])
        .await
        .expect("apply rtnetlink with vlan and load balance");
    apply_nft_with_precheck_and_multi_wan(&cfg, Some(&["wan".to_string(), "wan2".to_string()]))
        .await
        .expect("apply load-balance nftables");

    // Apply IPv6-PD /60 -> /64 slices onto br-lan & eth1.20 via rtnetlink
    let slices = slice_delegated_ipv6_prefix("2001:db8:abcd::/60", &cfg).expect("slice v6");
    assert_eq!(slices.len(), 2);
    apply_ipv6_pd_slices_netlink(&[], &slices)
        .await
        .expect("apply ipv6-pd slices to netlink");

    let addr_out = Command::new("ip")
        .args(["-6", "addr", "show", "dev", "eth1.20"])
        .output()
        .expect("ip -6 addr");
    let addr_str = String::from_utf8_lossy(&addr_out.stdout);
    assert!(addr_str.contains("2001:db8:abcd:1::1/64"));

    // Verify LoadBalance mode forwards LAN traffic cleanly
    ns_exec(lan.pid, &["ping", "-c", "2", "-W", "2", "198.51.100.1"]);
}

#[tokio::test]
async fn netns_veth_link_flap_netem_and_concurrent_nft_e2e() {
    if std::env::var_os("CRABROUTER_IN_NETNS").is_none() {
        let exe = std::env::current_exe().expect("current_exe");
        let out = Command::new("unshare")
            .args([
                "--user",
                "--net",
                "--map-root-user",
                exe.to_str().expect("utf8 exe"),
                "netns_veth_link_flap_netem_and_concurrent_nft_e2e",
                "--exact",
                "--nocapture",
            ])
            .env("CRABROUTER_IN_NETNS", "1")
            .output();
        if let Ok(res) = out {
            assert!(
                res.status.success(),
                "netns_e2e fault-injection inner failed:\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&res.stdout),
                String::from_utf8_lossy(&res.stderr)
            );
        }
        return;
    }

    let (wan1, _wan2, lan) = setup_veth_topology();
    let cfg = build_multi_wan_test_config();
    apply_rtnetlink_diff_with_wans(&cfg, &[], &["wan".to_string()])
        .await
        .expect("apply initial rtnetlink diff");
    apply_nft_with_precheck_and_multi_wan(&cfg, Some(&["wan".to_string()]))
        .await
        .expect("apply initial nftables");

    // 1. Link flap: bring eth0 down, then up and reapply rtnetlink diff -> forwarding recovers.
    run_cmd("ip", &["link", "set", "eth0", "down"]);
    run_cmd("ip", &["link", "set", "eth0", "up"]);
    apply_rtnetlink_diff_with_wans(&cfg, &[], &["wan".to_string()])
        .await
        .expect("reapply rtnetlink after link flap");
    ns_exec(lan.pid, &["ping", "-c", "2", "-W", "2", "198.51.100.1"]);

    // 2. Concurrent nftables + WanEventBus updates must serialize cleanly without race errors.
    let observer: Arc<dyn WanStateObserver> = Arc::new(TestObserver { cfg: cfg.clone() });
    let wan_bus = spawn_network_event_engine(Arc::clone(&observer), false);
    let multi_wan = spawn_multi_wan_engine(observer, wan_bus.clone(), false);

    let wans_a = ["wan".to_string()];
    let wans_b = ["wan".to_string(), "wan2".to_string()];
    let (nft_res_a, nft_res_b, ()) = tokio::join!(
        apply_nft_with_precheck_and_multi_wan(&cfg, Some(&wans_a)),
        apply_nft_with_precheck_and_multi_wan(&cfg, Some(&wans_b)),
        wan_bus.emit(crabrouter_net::WanLeaseEvent {
            source: "fault-test".to_string(),
            wan_name: "wan".to_string(),
            device: "eth0".to_string(),
            event: "BOUND".to_string(),
            ip: Some("100.64.1.2/24".to_string()),
            gateway: Some("100.64.1.1".to_string()),
            dns_servers: Vec::new(),
            ifindex: None,
            ipv6_addr: None,
            ipv6_pd_prefix: None,
        }),
    );
    nft_res_a.expect("concurrent nft apply A");
    nft_res_b.expect("concurrent nft apply B");
    let nft_list = Command::new("nft")
        .args(["list", "table", "inet", "crabrouter"])
        .output()
        .expect("nft list table");
    assert!(nft_list.status.success(), "nft table must remain intact");

    // 3. Optional tc netem 100% loss injection (when sch_netem is available on host kernel).
    let netem_add = Command::new("nsenter")
        .args([
            "-t",
            &wan1.pid.to_string(),
            "-n",
            "tc",
            "qdisc",
            "replace",
            "dev",
            "wan-peer",
            "root",
            "netem",
            "loss",
            "100%",
        ])
        .output();
    if let Ok(res) = netem_add
        && res.status.success()
    {
        let _ = multi_wan.run_probe_cycle().await;
        let st_down = multi_wan.run_probe_cycle().await;
        assert_eq!(st_down.active_wans, vec!["wan2"]);

        ns_exec(wan1.pid, &["tc", "qdisc", "del", "dev", "wan-peer", "root"]);
        let _ = multi_wan.run_probe_cycle().await;
        let _ = multi_wan.run_probe_cycle().await;
        let st_up = multi_wan.run_probe_cycle().await;
        assert_eq!(st_up.active_wans, vec!["wan"]);
    }
}
