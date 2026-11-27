use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};

/// Cross-VLAN mDNS / Bonjour reflection configuration (`avahi-daemon`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct MdnsConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Interfaces to reflect mDNS across (empty = all configured LAN bridges and LAN-role VLANs).
    #[serde(default)]
    pub interfaces: Vec<String>,
    /// Allow reflection over point-to-point interfaces (e.g. `WireGuard`).
    #[serde(default)]
    pub allow_point_to_point: bool,
    /// Publish the router's own workstation / hostname record via mDNS.
    #[serde(default)]
    pub publish_workstation: bool,
}

impl MdnsConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        for iface in &self.interfaces {
            let trimmed = iface.trim();
            if trimmed.is_empty() || trimmed.len() > 15 {
                return Err(ModelError::Validation(format!(
                    "mdns interface '{iface}' must be 1..=15 characters"
                )));
            }
        }
        Ok(())
    }
}

/// IPTV multicast proxy (`igmpproxy`) and multicast-to-HTTP unicast relay (`udpxy`) configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IptvConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Upstream WAN or VLAN interface delivering IPTV multicast (e.g. `"eth0.85"` or `"eth0"`).
    #[serde(default)]
    pub upstream_interface: String,
    /// Downstream LAN interfaces receiving multicast (empty = all configured LAN bridges).
    #[serde(default)]
    pub downstream_interfaces: Vec<String>,
    /// Allowed multicast source/group networks (`altnet` in `igmpproxy.conf`).
    #[serde(default = "default_altnet_cidrs")]
    pub altnet_cidrs: Vec<String>,
    /// Enable Linux bridge IGMP snooping on LAN bridges to prevent multicast flooding on Wi-Fi/ports.
    #[serde(default = "default_true")]
    pub igmp_snooping: bool,
    /// Enable `udpxy` multicast-to-HTTP unicast relay for LAN players.
    #[serde(default)]
    pub udpxy_enabled: bool,
    /// TCP listen port for `udpxy` on LAN interfaces.
    #[serde(default = "default_udpxy_port")]
    pub udpxy_port: u16,
    /// Maximum concurrent `udpxy` client streams.
    #[serde(default = "default_udpxy_max_clients")]
    pub udpxy_max_clients: u16,
}

impl Default for IptvConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            upstream_interface: String::new(),
            downstream_interfaces: Vec::new(),
            altnet_cidrs: default_altnet_cidrs(),
            igmp_snooping: true,
            udpxy_enabled: false,
            udpxy_port: default_udpxy_port(),
            udpxy_max_clients: default_udpxy_max_clients(),
        }
    }
}

impl IptvConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let up = self.upstream_interface.trim();
        if up.is_empty() || up.len() > 15 {
            return Err(ModelError::Validation(
                "iptv.upstream_interface must be 1..=15 characters when IPTV is enabled"
                    .to_string(),
            ));
        }
        for iface in &self.downstream_interfaces {
            let trimmed = iface.trim();
            if trimmed.is_empty() || trimmed.len() > 15 {
                return Err(ModelError::Validation(format!(
                    "iptv downstream interface '{iface}' must be 1..=15 characters"
                )));
            }
            if trimmed == up {
                return Err(ModelError::Validation(format!(
                    "iptv downstream interface '{iface}' cannot equal upstream_interface"
                )));
            }
        }
        for cidr in &self.altnet_cidrs {
            cidr.trim().parse::<IpNet>().map_err(|e| {
                ModelError::Validation(format!("iptv.altnet_cidrs invalid CIDR '{cidr}': {e}"))
            })?;
        }
        if self.udpxy_enabled {
            if self.udpxy_port == 0 {
                return Err(ModelError::Validation(
                    "iptv.udpxy_port must be >= 1".to_string(),
                ));
            }
            if self.udpxy_max_clients == 0 {
                return Err(ModelError::Validation(
                    "iptv.udpxy_max_clients must be >= 1".to_string(),
                ));
            }
        }
        Ok(())
    }
}

fn default_altnet_cidrs() -> Vec<String> {
    vec!["10.0.0.0/8".to_string(), "224.0.0.0/4".to_string()]
}

const fn default_true() -> bool {
    true
}

const fn default_udpxy_port() -> u16 {
    4022
}

const fn default_udpxy_max_clients() -> u16 {
    16
}
