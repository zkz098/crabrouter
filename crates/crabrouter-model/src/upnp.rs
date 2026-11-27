use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};

/// Configuration for `UPnP` IGD / NAT-PMP / PCP dynamic port mapping (`miniupnpd`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[allow(clippy::struct_excessive_bools)]
pub struct UpnpConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Enable `UPnP` IGD (SSDP / SOAP).
    #[serde(default = "default_true")]
    pub enable_upnp: bool,
    /// Enable NAT-PMP & PCP (UDP 5351).
    #[serde(default = "default_true")]
    pub enable_natpmp: bool,
    /// Only allow clients to forward ports to their own source IP.
    #[serde(default = "default_true")]
    pub secure_mode: bool,
    /// Explicit external WAN interface (defaults to primary WAN device / `ppp0`).
    #[serde(default)]
    pub wan_interface: Option<String>,
    /// Explicit LAN interfaces to listen on (empty = all configured LAN bridges).
    #[serde(default)]
    pub lan_interfaces: Vec<String>,
    /// Minimum allowed external mapped port (`1..=65535`).
    #[serde(default = "default_min_ext_port")]
    pub min_ext_port: u16,
    /// Maximum allowed external mapped port (`min_ext_port..=65535`).
    #[serde(default = "default_max_ext_port")]
    pub max_ext_port: u16,
    /// Default lease duration in seconds (`>= 60`).
    #[serde(default = "default_lease_secs")]
    pub lease_duration_secs: u32,
}

impl Default for UpnpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            enable_upnp: true,
            enable_natpmp: true,
            secure_mode: true,
            wan_interface: None,
            lan_interfaces: Vec::new(),
            min_ext_port: default_min_ext_port(),
            max_ext_port: default_max_ext_port(),
            lease_duration_secs: default_lease_secs(),
        }
    }
}

impl UpnpConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if !self.enable_upnp && !self.enable_natpmp {
            return Err(ModelError::Validation(
                "upnp: enable at least one of enable_upnp or enable_natpmp".to_string(),
            ));
        }
        if self.min_ext_port == 0 || self.min_ext_port > self.max_ext_port {
            return Err(ModelError::Validation(format!(
                "upnp port range {}..={} must satisfy 1 <= min <= max",
                self.min_ext_port, self.max_ext_port
            )));
        }
        if self.lease_duration_secs < 60 {
            return Err(ModelError::Validation(
                "upnp.lease_duration_secs must be >= 60".to_string(),
            ));
        }
        Ok(())
    }
}

/// Active dynamic port mapping entry read from `miniupnpd` lease file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpnpLeaseEntry {
    pub proto: String,
    pub external_port: u16,
    pub internal_ip: String,
    pub internal_port: u16,
    #[ts(type = "number")]
    pub expires_at: u64,
    pub description: String,
}

const fn default_true() -> bool {
    true
}

const fn default_min_ext_port() -> u16 {
    1024
}

const fn default_max_ext_port() -> u16 {
    65535
}

const fn default_lease_secs() -> u32 {
    3600
}
