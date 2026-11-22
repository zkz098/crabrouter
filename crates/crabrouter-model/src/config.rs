use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};
use crate::firewall::FirewallConfig;
use crate::network::{
    InterfacePin, LanConfig, PolicyRuleConfig, StaticRouteConfig, VlanConfig, WanConfig, WanProto,
};

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
    pub peers: Vec<WireGuardPeer>,
}

impl Default for WireGuardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interface: default_wg_interface(),
            listen_port: default_wg_port(),
            address_cidr: default_wg_cidr(),
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
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            system: SystemConfig::default(),
            wans: default_wans(),
            lans: default_lans(),
            vlans: Vec::new(),
            interface_pins: Vec::new(),
            routes: Vec::new(),
            policy_rules: Vec::new(),
            firewall: FirewallConfig::default(),
            dns: DnsIntegrationConfig::default(),
            wireguard: WireGuardConfig::default(),
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
        self.validate_interface_pins()?;
        self.validate_routing()?;
        self.validate_firewall()?;
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

    fn validate_firewall(&self) -> Result<()> {
        let zone_names: HashSet<&str> = self
            .firewall
            .zones
            .iter()
            .map(|z| z.name.as_str())
            .collect();

        for rule in &self.firewall.rules {
            if !zone_names.contains(rule.src_zone.as_str()) {
                return Err(ModelError::Validation(format!(
                    "firewall rule '{}' references unknown src_zone '{}'",
                    rule.name, rule.src_zone
                )));
            }
            if let Some(dst_zone) = rule.dst_zone.as_deref()
                && !zone_names.contains(dst_zone)
            {
                return Err(ModelError::Validation(format!(
                    "firewall rule '{}' references unknown dst_zone '{}'",
                    rule.name, dst_zone
                )));
            }
            validate_opt_ip_or_net(rule.src_ip.as_deref(), &rule.name, "src_ip")?;
            validate_opt_ip_or_net(rule.dst_ip.as_deref(), &rule.name, "dst_ip")?;
            if let Some(port_expr) = rule.dst_port.as_deref() {
                validate_port_expr(port_expr, &rule.name)?;
            }
        }

        for fwd in &self.firewall.port_forwards {
            if fwd.external_port == 0 || fwd.internal_port == 0 {
                return Err(ModelError::Validation(format!(
                    "port forward '{}' ports must be non-zero",
                    fwd.name
                )));
            }
            fwd.dest_ip.parse::<IpAddr>().map_err(|e| {
                ModelError::Validation(format!("port forward '{}' invalid dest_ip: {e}", fwd.name))
            })?;
        }
        Ok(())
    }
}

fn validate_opt_ip_or_net(val: Option<&str>, rule_name: &str, field: &str) -> Result<()> {
    let Some(raw) = val.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if raw.parse::<IpNet>().is_err() && raw.parse::<IpAddr>().is_err() {
        return Err(ModelError::Validation(format!(
            "firewall rule '{rule_name}' invalid {field} '{raw}'"
        )));
    }
    Ok(())
}

fn validate_port_expr(expr: &str, rule_name: &str) -> Result<()> {
    let trimmed = expr.trim();
    if let Some((start_s, end_s)) = trimmed.split_once('-') {
        let start: u16 = start_s.parse().map_err(|_| {
            ModelError::Validation(format!(
                "firewall rule '{rule_name}' invalid port range '{trimmed}'"
            ))
        })?;
        let end: u16 = end_s.parse().map_err(|_| {
            ModelError::Validation(format!(
                "firewall rule '{rule_name}' invalid port range '{trimmed}'"
            ))
        })?;
        if start == 0 || start > end {
            return Err(ModelError::Validation(format!(
                "firewall rule '{rule_name}' port range '{trimmed}' must have 1 <= start <= end"
            )));
        }
        return Ok(());
    }
    let port: u16 = trimmed.parse().map_err(|_| {
        ModelError::Validation(format!(
            "firewall rule '{rule_name}' invalid port '{trimmed}'"
        ))
    })?;
    if port == 0 {
        return Err(ModelError::Validation(format!(
            "firewall rule '{rule_name}' port cannot be 0"
        )));
    }
    Ok(())
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
        metric: 100,
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
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firewall::{FirewallAction, FirewallRule, TransportProto};
    use crate::network::InterfaceRole;

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
        });
        assert!(cfg.validate().is_err());

        let mut cfg2 = RouterConfig::default();
        cfg2.interface_pins.push(InterfacePin {
            mac: "not-a-mac".to_string(),
            name: "eth0".to_string(),
        });
        assert!(cfg2.validate().is_err());
    }
}
