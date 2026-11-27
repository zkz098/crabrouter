use std::collections::HashSet;
use std::net::IpAddr;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FirewallAction {
    #[default]
    Accept,
    Drop,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TransportProto {
    #[default]
    Tcp,
    Udp,
    TcpUdp,
    Icmp,
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FirewallZone {
    pub name: String,
    pub interfaces: Vec<String>,
    pub input: FirewallAction,
    pub output: FirewallAction,
    pub forward: FirewallAction,
    #[serde(default)]
    pub masquerade: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FirewallRule {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub src_zone: String,
    /// If `None`, applies to router `input` chain; if `Some(zone)`, applies to `forward` chain.
    #[serde(default)]
    pub dst_zone: Option<String>,
    pub proto: TransportProto,
    #[serde(default)]
    pub src_ip: Option<String>,
    #[serde(default)]
    pub dst_ip: Option<String>,
    /// Single port (`"443"`) or port range (`"8000-8100"`).
    #[serde(default)]
    pub dst_port: Option<String>,
    pub action: FirewallAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PortForwardRule {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub proto: TransportProto,
    pub src_zone: String,
    pub external_port: u16,
    pub dest_ip: String,
    pub internal_port: u16,
    /// Enable NAT loopback (hairpin NAT) for LAN clients accessing the public IP.
    #[serde(default = "default_true")]
    pub reflection: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[allow(clippy::struct_excessive_bools)]
pub struct FirewallConfig {
    #[serde(default = "default_true")]
    pub flowtable_offload: bool,
    #[serde(default = "default_true")]
    pub syn_flood_protect: bool,
    #[serde(default = "default_syn_flood_rate")]
    pub syn_flood_rate: u32,
    #[serde(default = "default_true")]
    pub mss_clamping: bool,
    /// Global switch for NAT loopback (hairpin NAT) on enabled port forwards.
    #[serde(default = "default_true")]
    pub nat_loopback: bool,
    #[serde(default)]
    pub zones: Vec<FirewallZone>,
    #[serde(default)]
    pub rules: Vec<FirewallRule>,
    #[serde(default)]
    pub port_forwards: Vec<PortForwardRule>,
}

impl Default for FirewallConfig {
    fn default() -> Self {
        Self {
            flowtable_offload: true,
            syn_flood_protect: true,
            syn_flood_rate: default_syn_flood_rate(),
            mss_clamping: true,
            nat_loopback: true,
            zones: vec![
                FirewallZone {
                    name: "lan".to_string(),
                    interfaces: vec!["br-lan".to_string()],
                    input: FirewallAction::Accept,
                    output: FirewallAction::Accept,
                    forward: FirewallAction::Accept,
                    masquerade: false,
                },
                FirewallZone {
                    name: "wan".to_string(),
                    interfaces: vec!["eth0".to_string(), "ppp0".to_string()],
                    input: FirewallAction::Drop,
                    output: FirewallAction::Accept,
                    forward: FirewallAction::Drop,
                    masquerade: true,
                },
            ],
            rules: Vec::new(),
            port_forwards: Vec::new(),
        }
    }
}

impl FirewallConfig {
    pub fn validate(&self) -> Result<()> {
        let zone_names: HashSet<&str> = self.zones.iter().map(|z| z.name.as_str()).collect();

        for rule in &self.rules {
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

        for fwd in &self.port_forwards {
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

const fn default_true() -> bool {
    true
}

const fn default_syn_flood_rate() -> u32 {
    25
}
