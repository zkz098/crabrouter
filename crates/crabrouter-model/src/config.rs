use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::bond::{BondConfig, validate_bonds};
use crate::ebpf::EbpfConfig;
use crate::error::{ModelError, Result};
use crate::firewall::FirewallConfig;
use crate::network::{
    AcmeConfig, DdnsConfig, InterfacePin, Ipv6LanMode, LanConfig, MultiWanConfig, PolicyRuleConfig,
    QosConfig, StaticRouteConfig, VlanConfig, WanConfig, WanProto, is_valid_unicast_mac,
};
use crate::nic_tuning::{NicTuningConfig, validate_nic_tuning};
use crate::upnp::UpnpConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DnsIntegrationConfig {
    /// Frontline DNS service (`crabdns`) listen port facing LAN clients.
    #[serde(default = "default_crabdns_port")]
    pub crabdns_port: u16,
    /// `crabdns` web UI/API port for management proxying or links.
    #[serde(default = "default_crabdns_web_port")]
    pub crabdns_web_port: u16,
    /// Upstream DNS servers used by `crabdns` for public resolution.
    #[serde(default = "default_public_upstreams")]
    pub public_upstreams: Vec<String>,
    /// Backend `dnsmasq` DNS port for local domain & private PTR resolution.
    #[serde(default = "default_dnsmasq_port")]
    pub dnsmasq_dns_port: u16,
    /// Enable `crabdns` `DoT` / `DoH` listeners if TLS cert is configured.
    #[serde(default = "default_true")]
    pub encrypted_dns_enabled: bool,
}

impl Default for DnsIntegrationConfig {
    fn default() -> Self {
        Self {
            crabdns_port: default_crabdns_port(),
            crabdns_web_port: default_crabdns_web_port(),
            public_upstreams: default_public_upstreams(),
            dnsmasq_dns_port: default_dnsmasq_port(),
            encrypted_dns_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WireGuardPeer {
    pub name: String,
    pub public_key: String,
    pub allowed_ips: Vec<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub persistent_keepalive: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WireGuardConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_wg_interface")]
    pub interface: String,
    #[serde(default = "default_wg_port")]
    pub listen_port: u16,
    #[serde(default = "default_wg_cidr")]
    pub address_cidr: String,
    #[serde(default)]
    pub private_key: Option<String>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub peers: Vec<WireGuardPeer>,
}

impl Default for WireGuardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interface: default_wg_interface(),
            listen_port: default_wg_port(),
            address_cidr: default_wg_cidr(),
            private_key: None,
            mtu: None,
            peers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SystemConfig {
    #[serde(default = "default_hostname")]
    pub hostname: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_web_listen")]
    pub web_listen: String,
    #[serde(default)]
    pub web_lan_only: bool,
    #[serde(default)]
    pub web_https_enabled: bool,
    #[serde(default = "default_https_listen")]
    pub web_https_listen: String,
    #[serde(default)]
    pub web_tls_cert: Option<String>,
    #[serde(default)]
    pub web_tls_key: Option<String>,
    #[serde(default = "default_ota_url")]
    pub ota_update_url: String,
    #[serde(default = "default_commit_confirm_secs")]
    #[ts(type = "number")]
    pub commit_confirm_timeout_secs: u64,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            hostname: default_hostname(),
            timezone: default_timezone(),
            web_listen: default_web_listen(),
            web_lan_only: false,
            web_https_enabled: false,
            web_https_listen: default_https_listen(),
            web_tls_cert: None,
            web_tls_key: None,
            ota_update_url: default_ota_url(),
            commit_confirm_timeout_secs: default_commit_confirm_secs(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RouterConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub system: SystemConfig,
    #[serde(default = "default_wans")]
    pub wans: Vec<WanConfig>,
    #[serde(default = "default_lans")]
    pub lans: Vec<LanConfig>,
    #[serde(default)]
    pub vlans: Vec<VlanConfig>,
    #[serde(default)]
    pub bonds: Vec<BondConfig>,
    #[serde(default)]
    pub interface_pins: Vec<InterfacePin>,
    #[serde(default)]
    pub routes: Vec<StaticRouteConfig>,
    #[serde(default)]
    pub policy_rules: Vec<PolicyRuleConfig>,
    #[serde(default)]
    pub firewall: FirewallConfig,
    #[serde(default)]
    pub dns: DnsIntegrationConfig,
    #[serde(default)]
    pub wireguard: WireGuardConfig,
    #[serde(default)]
    pub qos: QosConfig,
    #[serde(default)]
    pub multi_wan: MultiWanConfig,
    #[serde(default)]
    pub ebpf: EbpfConfig,
    #[serde(default)]
    pub ddns: DdnsConfig,
    #[serde(default)]
    pub acme: AcmeConfig,
    #[serde(default)]
    pub upnp: UpnpConfig,
    #[serde(default)]
    pub nic_tuning: Vec<NicTuningConfig>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            system: SystemConfig::default(),
            wans: default_wans(),
            lans: default_lans(),
            vlans: Vec::new(),
            bonds: Vec::new(),
            interface_pins: Vec::new(),
            routes: Vec::new(),
            policy_rules: Vec::new(),
            firewall: FirewallConfig::default(),
            dns: DnsIntegrationConfig::default(),
            wireguard: WireGuardConfig::default(),
            qos: QosConfig::default(),
            multi_wan: MultiWanConfig::default(),
            ebpf: EbpfConfig::default(),
            ddns: DdnsConfig::default(),
            acme: AcmeConfig::default(),
            upnp: UpnpConfig::default(),
            nic_tuning: Vec::new(),
        }
    }
}

impl RouterConfig {
    pub fn validate(&self) -> Result<()> {
        if self.system.hostname.trim().is_empty() {
            return Err(ModelError::Validation(
                "system.hostname cannot be empty".to_string(),
            ));
        }
        if self.dns.crabdns_port == self.dns.dnsmasq_dns_port {
            return Err(ModelError::Validation(
                "crabdns_port and dnsmasq_dns_port must be distinct to prevent DNS loops"
                    .to_string(),
            ));
        }

        self.validate_wans()?;
        self.validate_lans()?;
        self.validate_vlans()?;
        validate_bonds(&self.bonds, &self.lans)?;
        self.validate_interface_pins()?;
        self.validate_routing()?;
        self.firewall.validate()?;
        self.validate_wireguard()?;
        self.validate_qos()?;
        self.validate_multi_wan()?;
        self.ebpf.validate()?;
        self.ddns.validate()?;
        self.acme.validate()?;
        self.upnp.validate()?;
        validate_nic_tuning(&self.nic_tuning)?;
        Ok(())
    }

    fn validate_wans(&self) -> Result<()> {
        let mut names = HashSet::new();
        for wan in &self.wans {
            if wan.name.trim().is_empty() || wan.device.trim().is_empty() {
                return Err(ModelError::Validation(
                    "WAN name and device cannot be empty".to_string(),
                ));
            }
            if !names.insert(wan.name.as_str()) {
                return Err(ModelError::Validation(format!(
                    "duplicate WAN name '{}'",
                    wan.name
                )));
            }
            if wan.mtu < 576 {
                return Err(ModelError::Validation(format!(
                    "WAN '{}' MTU {} is below minimum 576",
                    wan.name, wan.mtu
                )));
            }
            if wan.ipv6_pd && !(48..=64).contains(&wan.ipv6_pd_prefix_len) {
                return Err(ModelError::Validation(format!(
                    "WAN '{}' ipv6_pd_prefix_len {} must be between 48 and 64",
                    wan.name, wan.ipv6_pd_prefix_len
                )));
            }
            if let Some(mac) = wan
                .mac_override
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                && !is_valid_unicast_mac(mac)
            {
                return Err(ModelError::Validation(format!(
                    "WAN '{}' invalid mac_override '{mac}' (must be a unicast MAC)",
                    wan.name
                )));
            }
            match wan.proto {
                WanProto::Static => {
                    let cidr = wan.static_cidr.as_deref().ok_or_else(|| {
                        ModelError::Validation(format!(
                            "WAN '{}' uses static proto but static_cidr is missing",
                            wan.name
                        ))
                    })?;
                    cidr.parse::<IpNet>().map_err(|e| {
                        ModelError::Validation(format!(
                            "WAN '{}' invalid static_cidr: {e}",
                            wan.name
                        ))
                    })?;
                    if let Some(gw) = wan.gateway.as_deref() {
                        gw.parse::<IpAddr>().map_err(|e| {
                            ModelError::Validation(format!(
                                "WAN '{}' invalid gateway: {e}",
                                wan.name
                            ))
                        })?;
                    }
                }
                WanProto::Pppoe => {
                    if wan
                        .pppoe_username
                        .as_deref()
                        .is_none_or(|u| u.trim().is_empty())
                    {
                        return Err(ModelError::Validation(format!(
                            "WAN '{}' uses pppoe proto without pppoe_username",
                            wan.name
                        )));
                    }
                }
                WanProto::Dhcp => {}
            }
        }
        Ok(())
    }

    fn validate_lans(&self) -> Result<()> {
        if self.lans.is_empty() {
            return Err(ModelError::Validation(
                "at least one LAN configuration is required".to_string(),
            ));
        }
        let mut bridges = HashSet::new();
        for lan in &self.lans {
            if lan.bridge.trim().is_empty() {
                return Err(ModelError::Validation(format!(
                    "LAN '{}' bridge name cannot be empty",
                    lan.name
                )));
            }
            if !bridges.insert(lan.bridge.as_str()) {
                return Err(ModelError::Validation(format!(
                    "duplicate LAN bridge '{}'",
                    lan.bridge
                )));
            }
            let net: IpNet = lan.cidr.parse().map_err(|e| {
                ModelError::Validation(format!("LAN '{}' invalid cidr: {e}", lan.name))
            })?;
            if matches!(net, IpNet::V4(v4) if v4.prefix_len() >= 31) {
                return Err(ModelError::Validation(format!(
                    "LAN '{}' IPv4 prefix must allow host addresses (< /31)",
                    lan.name
                )));
            }
            for lease in &lan.static_leases {
                if !is_valid_mac(&lease.mac) {
                    return Err(ModelError::Validation(format!(
                        "LAN '{}' static lease '{}' invalid MAC '{}'",
                        lan.name, lease.name, lease.mac
                    )));
                }
                lease.ip.parse::<Ipv4Addr>().map_err(|e| {
                    ModelError::Validation(format!(
                        "LAN '{}' static lease '{}' invalid IPv4 '{}': {e}",
                        lan.name, lease.name, lease.ip
                    ))
                })?;
            }
            if let Some(ula) = lan
                .ipv6_ula_cidr
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let ula_net: ipnet::Ipv6Net = ula.parse().map_err(|e| {
                    ModelError::Validation(format!(
                        "LAN '{}' invalid ipv6_ula_cidr '{}': {e}",
                        lan.name, ula
                    ))
                })?;
                if ula_net.prefix_len() > 64 {
                    return Err(ModelError::Validation(format!(
                        "LAN '{}' ipv6_ula_cidr '{}' prefix length must be <= 64 for SLAAC/RA",
                        lan.name, ula
                    )));
                }
            }
            if lan.ipv6_mode == Ipv6LanMode::Relay
                && let Some(wan_ref) = lan
                    .ipv6_relay_wan
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                && !self.wans.iter().any(|w| w.name == wan_ref)
            {
                return Err(ModelError::Validation(format!(
                    "LAN '{}' ipv6_relay_wan '{wan_ref}' references unknown WAN",
                    lan.name
                )));
            }
        }
        self.validate_ipv6_subnet_ids()?;
        Ok(())
    }

    fn validate_ipv6_subnet_ids(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for lan in &self.lans {
            if lan.ipv6_ra_enabled
                && let Some(id) = lan.ipv6_subnet_id
                && !seen.insert(id)
            {
                return Err(ModelError::Validation(format!(
                    "duplicate IPv6-PD subnet_id {id} on LAN '{}'",
                    lan.name
                )));
            }
        }
        for vlan in &self.vlans {
            if vlan.role == crate::network::InterfaceRole::Lan
                && vlan.ipv6_ra_enabled
                && let Some(id) = vlan.ipv6_subnet_id
                && !seen.insert(id)
            {
                return Err(ModelError::Validation(format!(
                    "duplicate IPv6-PD subnet_id {id} on VLAN '{}.{}'",
                    vlan.parent, vlan.id
                )));
            }
        }
        Ok(())
    }

    fn validate_vlans(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for vlan in &self.vlans {
            if vlan.parent.trim().is_empty() {
                return Err(ModelError::Validation(
                    "VLAN parent interface cannot be empty".to_string(),
                ));
            }
            if !(1..=4094).contains(&vlan.id) {
                return Err(ModelError::Validation(format!(
                    "VLAN ID {} on '{}' out of valid range 1..=4094",
                    vlan.id, vlan.parent
                )));
            }
            let ifname = format!("{}.{}", vlan.parent, vlan.id);
            if ifname.len() > 15 {
                return Err(ModelError::Validation(format!(
                    "VLAN interface name '{ifname}' exceeds Linux 15-char IFNAMSIZ limit"
                )));
            }
            if !seen.insert((vlan.parent.as_str(), vlan.id)) {
                return Err(ModelError::Validation(format!(
                    "duplicate VLAN {}.{}",
                    vlan.parent, vlan.id
                )));
            }
            if let Some(cidr) = vlan.cidr.as_deref() {
                cidr.parse::<IpNet>().map_err(|e| {
                    ModelError::Validation(format!("VLAN '{ifname}' invalid cidr: {e}"))
                })?;
            }
        }
        Ok(())
    }

    fn validate_interface_pins(&self) -> Result<()> {
        let mut macs = HashSet::new();
        let mut names = HashSet::new();
        for pin in &self.interface_pins {
            if !is_valid_mac(&pin.mac) {
                return Err(ModelError::Validation(format!(
                    "interface pin invalid MAC '{}'",
                    pin.mac
                )));
            }
            let name = pin.name.trim();
            if name.is_empty() || name.len() > 15 || name.contains(char::is_whitespace) {
                return Err(ModelError::Validation(format!(
                    "interface pin invalid target name '{}'",
                    pin.name
                )));
            }
            if !macs.insert(pin.mac.to_ascii_lowercase()) {
                return Err(ModelError::Validation(format!(
                    "duplicate interface pin MAC '{}'",
                    pin.mac
                )));
            }
            if !names.insert(name) {
                return Err(ModelError::Validation(format!(
                    "duplicate interface pin name '{}'",
                    pin.name
                )));
            }
        }
        Ok(())
    }

    fn validate_routing(&self) -> Result<()> {
        for route in &self.routes {
            route.destination.parse::<IpNet>().map_err(|e| {
                ModelError::Validation(format!("static route invalid destination: {e}"))
            })?;
            if let Some(gw) = route.gateway.as_deref() {
                gw.parse::<IpAddr>().map_err(|e| {
                    ModelError::Validation(format!("static route invalid gateway: {e}"))
                })?;
            }
            if route.gateway.is_none() && route.device.as_deref().is_none_or(str::is_empty) {
                return Err(ModelError::Validation(format!(
                    "static route '{}' must specify gateway or device",
                    route.destination
                )));
            }
        }
        for rule in &self.policy_rules {
            if rule.table_id == 0 {
                return Err(ModelError::Validation(
                    "policy rule table_id must be > 0".to_string(),
                ));
            }
            if let Some(src) = rule.src.as_deref() {
                src.parse::<IpNet>().map_err(|e| {
                    ModelError::Validation(format!("policy rule invalid src prefix: {e}"))
                })?;
            }
            if let Some(dst) = rule.dst.as_deref() {
                dst.parse::<IpNet>().map_err(|e| {
                    ModelError::Validation(format!("policy rule invalid dst prefix: {e}"))
                })?;
            }
        }
        Ok(())
    }

    fn validate_wireguard(&self) -> Result<()> {
        if self.wireguard.enabled {
            if self.wireguard.interface.trim().is_empty() {
                return Err(ModelError::Validation(
                    "wireguard.interface cannot be empty when enabled".to_string(),
                ));
            }
            self.wireguard.address_cidr.parse::<IpNet>().map_err(|e| {
                ModelError::Validation(format!("wireguard invalid address_cidr: {e}"))
            })?;
        }
        Ok(())
    }

    fn validate_qos(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for item in &self.qos.interfaces {
            let iface = item.interface.trim();
            if iface.is_empty() || iface.len() > 15 || iface.contains(char::is_whitespace) {
                return Err(ModelError::Validation(format!(
                    "QoS invalid interface name '{}'",
                    item.interface
                )));
            }
            if !seen.insert(iface) {
                return Err(ModelError::Validation(format!(
                    "duplicate QoS configuration for interface '{iface}'"
                )));
            }
            if !(-64..=256).contains(&item.overhead_bytes) {
                return Err(ModelError::Validation(format!(
                    "QoS interface '{iface}' overhead_bytes {} must be within -64..=256",
                    item.overhead_bytes
                )));
            }
        }
        Ok(())
    }

    fn validate_multi_wan(&self) -> Result<()> {
        if self.multi_wan.probe_interval_secs == 0 {
            return Err(ModelError::Validation(
                "multi_wan.probe_interval_secs must be >= 1".to_string(),
            ));
        }
        if self.multi_wan.probe_timeout_ms < 200 {
            return Err(ModelError::Validation(
                "multi_wan.probe_timeout_ms must be >= 200".to_string(),
            ));
        }
        if self.multi_wan.fail_threshold == 0 || self.multi_wan.recover_threshold == 0 {
            return Err(ModelError::Validation(
                "multi_wan.fail_threshold and recover_threshold must be >= 1".to_string(),
            ));
        }

        let mut used_tables = HashSet::new();
        for (idx, wan) in self.wans.iter().enumerate() {
            if self.multi_wan.enabled && wan.weight == 0 {
                return Err(ModelError::Validation(format!(
                    "WAN '{}' weight must be >= 1 when multi_wan is enabled",
                    wan.name
                )));
            }
            let table_id = wan.table_id.unwrap_or(101 + idx as u32);
            if matches!(table_id, 0 | 253 | 254 | 255) {
                return Err(ModelError::Validation(format!(
                    "WAN '{}' table_id {table_id} is reserved (0, 253, 254, 255)",
                    wan.name
                )));
            }
            if !used_tables.insert(table_id) {
                return Err(ModelError::Validation(format!(
                    "duplicate WAN routing table_id {table_id} on '{}'",
                    wan.name
                )));
            }
        }
        Ok(())
    }
}

pub fn is_valid_mac(mac: &str) -> bool {
    let parts: Vec<&str> = mac.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

const fn default_schema_version() -> u32 {
    1
}

const fn default_crabdns_port() -> u16 {
    53
}

const fn default_crabdns_web_port() -> u16 {
    3000
}

fn default_public_upstreams() -> Vec<String> {
    vec![
        "tls://1.1.1.1:853".to_string(),
        "tls://8.8.8.8:853".to_string(),
    ]
}

const fn default_dnsmasq_port() -> u16 {
    5354
}

const fn default_true() -> bool {
    true
}

fn default_wg_interface() -> String {
    "wg0".to_string()
}

const fn default_wg_port() -> u16 {
    51820
}

fn default_wg_cidr() -> String {
    "10.10.0.1/24".to_string()
}

fn default_hostname() -> String {
    "crabrouter".to_string()
}

fn default_timezone() -> String {
    "Asia/Shanghai".to_string()
}

fn default_web_listen() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_https_listen() -> String {
    "0.0.0.0:8443".to_string()
}

fn default_ota_url() -> String {
    "https://updates.example.com/crabrouter".to_string()
}

const fn default_commit_confirm_secs() -> u64 {
    60
}

fn default_wans() -> Vec<WanConfig> {
    vec![WanConfig {
        name: "wan".to_string(),
        device: "eth0".to_string(),
        proto: WanProto::Dhcp,
        static_cidr: None,
        gateway: None,
        pppoe_username: None,
        pppoe_password: None,
        mtu: 1500,
        ipv6_pd: true,
        ipv6_pd_prefix_len: 60,
        metric: 100,
        weight: 1,
        table_id: None,
        health_check_enabled: true,
        probe_targets: Vec::new(),
        mac_override: None,
    }]
}

fn default_lans() -> Vec<LanConfig> {
    vec![LanConfig {
        name: "lan".to_string(),
        bridge: "br-lan".to_string(),
        ports: vec!["eth1".to_string(), "eth2".to_string(), "eth3".to_string()],
        cidr: "192.168.10.1/24".to_string(),
        domain: "lan".to_string(),
        dhcp_enabled: true,
        dhcp_start_offset: 100,
        dhcp_limit: 150,
        dhcp_lease_secs: 43_200,
        static_leases: Vec::new(),
        ipv6_ra_enabled: true,
        ipv6_mode: Ipv6LanMode::Server,
        ipv6_relay_wan: None,
        ipv6_subnet_id: None,
        ipv6_ula_cidr: None,
        stp_enabled: false,
        forward_delay_cs: 0,
    }]
}
