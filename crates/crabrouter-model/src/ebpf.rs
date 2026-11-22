use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DomainPolicyRule {
    pub domain: String,
    pub wan_name: String,
    pub tag_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct EbpfConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub attach_interfaces: Vec<String>,
    #[serde(default)]
    pub domain_rules: Vec<DomainPolicyRule>,
    #[serde(default)]
    pub xdp_guard_enabled: bool,
    #[serde(default)]
    pub xdp_interfaces: Vec<String>,
    #[serde(default)]
    pub xdp_blocklist: Vec<String>,
    #[serde(default = "default_xdp_syn_limit")]
    pub xdp_syn_rate_limit: u32,
    #[serde(default)]
    pub fast_forward_enabled: bool,
}

const fn default_xdp_syn_limit() -> u32 {
    200
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EbpfTrafficStat {
    pub client_ip: String,
    pub tag_id: u32,
    pub domain_label: Option<String>,
    pub wan_name: Option<String>,
    #[ts(type = "number")]
    pub tx_packets: u64,
    #[ts(type = "number")]
    pub tx_bytes: u64,
    #[ts(type = "number")]
    pub rx_packets: u64,
    #[ts(type = "number")]
    pub rx_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EbpfRuntimeStatus {
    pub enabled: bool,
    pub attached_interfaces: Vec<String>,
    pub active_lpm_entries: u32,
    #[serde(default)]
    pub active_lpm_v6_entries: u32,
    pub stats: Vec<EbpfTrafficStat>,
    pub xdp_attached_interfaces: Vec<String>,
    #[ts(type = "number")]
    pub xdp_blocklist_drops: u64,
    #[ts(type = "number")]
    pub xdp_syn_drops: u64,
    #[serde(default)]
    #[ts(type = "number")]
    pub fast_forward_packets: u64,
    pub last_error: Option<String>,
}

impl EbpfConfig {
    pub fn validate(&self) -> Result<()> {
        let mut tags = std::collections::HashSet::new();
        for rule in &self.domain_rules {
            if rule.domain.trim().is_empty() {
                return Err(ModelError::Validation(
                    "ebpf domain_rule domain cannot be empty".to_string(),
                ));
            }
            if rule.tag_id == 0 {
                return Err(ModelError::Validation(format!(
                    "ebpf domain_rule '{}' tag_id must be > 0",
                    rule.domain
                )));
            }
            if !tags.insert(rule.tag_id) {
                return Err(ModelError::Validation(format!(
                    "duplicate ebpf domain_rule tag_id {}",
                    rule.tag_id
                )));
            }
        }
        for entry in &self.xdp_blocklist {
            let trimmed = entry.trim();
            if trimmed.parse::<std::net::IpAddr>().is_err()
                && trimmed.parse::<ipnet::IpNet>().is_err()
            {
                return Err(ModelError::Validation(format!(
                    "ebpf.xdp_blocklist invalid IP/CIDR '{entry}'"
                )));
            }
        }
        Ok(())
    }
}
