use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};

/// Custom static DNS host record rendered into `dnsmasq` (`host-record=<hostname>,<ip>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomHostEntry {
    pub hostname: String,
    pub ip: String,
}

/// Custom DHCP option rendered per-LAN into `dnsmasq` (`dhcp-option=tag:<lan>,<option>,<value>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DhcpCustomOption {
    /// Option code or name (e.g. `"42"`, `"66"`, `"option:ntp-server"`).
    pub option: String,
    /// Option value (e.g. `"192.168.10.1"`).
    pub value: String,
}

/// Request payload for `POST /api/wol` to send a Wake-on-LAN magic packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WolRequest {
    pub mac: String,
    #[serde(default)]
    pub broadcast_ip: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

/// Result returned after sending (or dry-run validating) a Wake-on-LAN magic packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WolResult {
    pub ok: bool,
    pub mac: String,
    pub broadcast_ip: String,
    pub port: u16,
    pub message: String,
}

pub fn validate_custom_hosts(hosts: &[CustomHostEntry]) -> Result<()> {
    for entry in hosts {
        let host = entry.hostname.trim();
        if host.is_empty()
            || host.contains(char::is_whitespace)
            || host.contains('/')
            || host.contains(',')
        {
            return Err(ModelError::Validation(format!(
                "invalid custom DNS hostname '{}'",
                entry.hostname
            )));
        }
        entry.ip.trim().parse::<IpAddr>().map_err(|e| {
            ModelError::Validation(format!(
                "custom host '{}' invalid IP '{}': {e}",
                entry.hostname, entry.ip
            ))
        })?;
    }
    Ok(())
}

pub fn validate_dhcp_custom_options(lan_name: &str, options: &[DhcpCustomOption]) -> Result<()> {
    for opt in options {
        let key = opt.option.trim();
        let val = opt.value.trim();
        if key.is_empty() || key.contains(['\n', '\r', ',']) {
            return Err(ModelError::Validation(format!(
                "LAN '{lan_name}' invalid DHCP custom option key '{}'",
                opt.option
            )));
        }
        if val.is_empty() || val.contains(['\n', '\r']) {
            return Err(ModelError::Validation(format!(
                "LAN '{lan_name}' invalid DHCP custom option value for '{}'",
                opt.option
            )));
        }
    }
    Ok(())
}

pub fn is_valid_unicast_mac(mac: &str) -> bool {
    let mut bytes = [0u8; 6];
    let mut count = 0usize;
    for part in mac.split(':') {
        if count >= 6 || part.len() != 2 {
            return false;
        }
        let Ok(b) = u8::from_str_radix(part, 16) else {
            return false;
        };
        bytes[count] = b;
        count += 1;
    }
    count == 6 && bytes != [0u8; 6] && bytes != [0xff; 6] && (bytes[0] & 0x01) == 0
}

pub fn validate_wol_mac(mac: &str) -> Result<String> {
    let normalized = mac.trim().replace('-', ":").to_ascii_lowercase();
    if !is_valid_unicast_mac(&normalized) {
        return Err(ModelError::Validation(format!(
            "invalid Wake-on-LAN MAC address '{mac}'"
        )));
    }
    Ok(normalized)
}

pub(crate) fn validate_wan_modem_cidr(wan_name: &str, modem_cidr: Option<&str>) -> Result<()> {
    let Some(modem_cidr) = modem_cidr.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let net: ipnet::IpNet = modem_cidr.parse().map_err(|e| {
        ModelError::Validation(format!(
            "WAN '{wan_name}' invalid modem_cidr '{modem_cidr}': {e}"
        ))
    })?;
    if !matches!(net, ipnet::IpNet::V4(_)) {
        return Err(ModelError::Validation(format!(
            "WAN '{wan_name}' modem_cidr '{modem_cidr}' must be an IPv4 CIDR"
        )));
    }
    Ok(())
}

pub(crate) fn validate_wan_macvlan(
    wan_name: &str,
    device: &str,
    macvlan_parent: Option<&str>,
) -> Result<()> {
    let Some(parent) = macvlan_parent.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if parent == device.trim() {
        return Err(ModelError::Validation(format!(
            "WAN '{wan_name}' macvlan_parent '{parent}' cannot equal device"
        )));
    }
    if device.trim().len() > 15 {
        return Err(ModelError::Validation(format!(
            "WAN '{wan_name}' macvlan device '{device}' exceeds Linux 15-char interface limit"
        )));
    }
    Ok(())
}
