use std::collections::HashSet;
use std::net::{IpAddr, Ipv6Addr};

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
    /// Optional inclusive end port for external port-range forwarding (`external_port-external_port_end`).
    #[serde(default)]
    pub external_port_end: Option<u16>,
    pub dest_ip: String,
    pub internal_port: u16,
    /// Optional inclusive end port for internal destination range (`internal_port-internal_port_end`).
    #[serde(default)]
    pub internal_port_end: Option<u16>,
    /// Enable NAT loopback (hairpin NAT) for LAN clients accessing the public IP.
    #[serde(default = "default_true")]
    pub reflection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Ipv6NatMode {
    #[default]
    Disabled,
    /// Standard IPv6 masquerade (NAT66) on masquerade-enabled WAN zones.
    Masquerade,
    /// Stateless 1:1 IPv6 Network Prefix Translation (`NPTv6`) via `snat/dnat ip6 prefix to`.
    Nptv6,
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
    /// IPv6 NAT mode (`disabled`, `masquerade` for NAT66, or `nptv6` for 1:1 prefix translation).
    #[serde(default)]
    pub ipv6_nat_mode: Ipv6NatMode,
    /// Internal ULA/LAN IPv6 prefix for `NPTv6` (e.g. `fd00:10::/64`).
    #[serde(default)]
    pub nptv6_internal_prefix: Option<String>,
    /// External WAN IPv6 prefix for `NPTv6` (e.g. `2001:db8:1::/64`).
    #[serde(default)]
    pub nptv6_external_prefix: Option<String>,
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
            ipv6_nat_mode: Ipv6NatMode::Disabled,
            nptv6_internal_prefix: None,
            nptv6_external_prefix: None,
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
            validate_port_forward(fwd)?;
        }

        if self.ipv6_nat_mode == Ipv6NatMode::Nptv6 {
            validate_nptv6_prefixes(
                self.nptv6_internal_prefix.as_deref(),
                self.nptv6_external_prefix.as_deref(),
            )?;
        }
        Ok(())
    }
}

fn validate_port_forward(fwd: &PortForwardRule) -> Result<()> {
    if fwd.external_port == 0 || fwd.internal_port == 0 {
        return Err(ModelError::Validation(format!(
            "port forward '{}' ports must be non-zero",
            fwd.name
        )));
    }
    if let Some(ext_end) = fwd.external_port_end {
        if ext_end < fwd.external_port {
            return Err(ModelError::Validation(format!(
                "port forward '{}' external_port_end ({ext_end}) must be >= external_port ({})",
                fwd.name, fwd.external_port
            )));
        }
        if fwd.internal_port_end.is_none() {
            let span = u32::from(ext_end - fwd.external_port);
            if u32::from(fwd.internal_port) + span > 65_535 {
                return Err(ModelError::Validation(format!(
                    "port forward '{}' shifted internal port range exceeds 65535",
                    fwd.name
                )));
            }
        }
    }
    if let Some(int_end) = fwd.internal_port_end {
        if int_end < fwd.internal_port {
            return Err(ModelError::Validation(format!(
                "port forward '{}' internal_port_end ({int_end}) must be >= internal_port ({})",
                fwd.name, fwd.internal_port
            )));
        }
        if let Some(ext_end) = fwd.external_port_end
            && int_end != fwd.internal_port
            && (ext_end - fwd.external_port) != (int_end - fwd.internal_port)
        {
            return Err(ModelError::Validation(format!(
                "port forward '{}' external and internal port range widths must match",
                fwd.name
            )));
        }
    }
    fwd.dest_ip.parse::<IpAddr>().map_err(|e| {
        ModelError::Validation(format!("port forward '{}' invalid dest_ip: {e}", fwd.name))
    })?;
    Ok(())
}

/// Parses an IPv6 suffix/bitwise mask expression `<ipv6_addr>/<ipv6_mask>`
/// (for example `::1234/::ffff:ffff:ffff:ffff`).
pub fn parse_ipv6_mask_expr(raw: &str) -> Option<(Ipv6Addr, Ipv6Addr)> {
    let (addr_s, mask_s) = raw.trim().split_once('/')?;
    let addr = addr_s.trim().parse::<Ipv6Addr>().ok()?;
    let mask = mask_s.trim().parse::<Ipv6Addr>().ok()?;
    Some((addr, mask))
}

fn validate_nptv6_prefixes(internal: Option<&str>, external: Option<&str>) -> Result<()> {
    let int_raw = internal
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ModelError::Validation("NPTv6 requires nptv6_internal_prefix".to_string())
        })?;
    let ext_raw = external
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ModelError::Validation("NPTv6 requires nptv6_external_prefix".to_string())
        })?;
    let IpNet::V6(int_v6) = int_raw.parse::<IpNet>().map_err(|e| {
        ModelError::Validation(format!("invalid nptv6_internal_prefix '{int_raw}': {e}"))
    })?
    else {
        return Err(ModelError::Validation(format!(
            "nptv6_internal_prefix '{int_raw}' must be an IPv6 CIDR"
        )));
    };
    let IpNet::V6(ext_v6) = ext_raw.parse::<IpNet>().map_err(|e| {
        ModelError::Validation(format!("invalid nptv6_external_prefix '{ext_raw}': {e}"))
    })?
    else {
        return Err(ModelError::Validation(format!(
            "nptv6_external_prefix '{ext_raw}' must be an IPv6 CIDR"
        )));
    };
    if int_v6.prefix_len() != ext_v6.prefix_len() || !(1..=64).contains(&int_v6.prefix_len()) {
        return Err(ModelError::Validation(
            "NPTv6 internal and external prefixes must have matching prefix lengths in 1..=64"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_opt_ip_or_net(val: Option<&str>, rule_name: &str, field: &str) -> Result<()> {
    let Some(raw) = val.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if raw.parse::<IpNet>().is_err()
        && raw.parse::<IpAddr>().is_err()
        && parse_ipv6_mask_expr(raw).is_none()
    {
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
