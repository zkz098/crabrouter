use std::net::IpAddr;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};

/// `CrowdSec` behavior-based IPS & collaborative threat intelligence configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CrowdSecConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Local API (LAPI) listen address (default `127.0.0.1:8080`).
    #[serde(default = "default_lapi_listen")]
    pub lapi_listen: String,
    /// Enable enforcement bouncer (`nftables` blocklist set + optional `eBPF XDP` sync).
    #[serde(default = "default_true")]
    pub bouncer_enabled: bool,
    /// Decision polling interval in seconds (`>= 5`).
    #[serde(default = "default_update_freq")]
    pub update_frequency_secs: u32,
    /// Default ban duration expression (e.g. `"4h"`, `"24h"`).
    #[serde(default = "default_ban_duration")]
    pub ban_duration: String,
    /// Automatically mirror active IP/CIDR decisions into eBPF `XDP` guard blocklist when enabled.
    #[serde(default = "default_true")]
    pub sync_to_xdp: bool,
    /// `CrowdSec` Hub collections to enable (e.g. `crowdsecurity/linux`, `crowdsecurity/sshd`).
    #[serde(default = "default_collections")]
    pub collections: Vec<String>,
    /// Trusted IP/CIDR ranges exempt from `CrowdSec` bans.
    #[serde(default)]
    pub whitelist_cidrs: Vec<String>,
    /// Optional `CrowdSec` Console enrollment key.
    #[serde(default)]
    pub enroll_key: Option<String>,
}

impl Default for CrowdSecConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            lapi_listen: default_lapi_listen(),
            bouncer_enabled: true,
            update_frequency_secs: default_update_freq(),
            ban_duration: default_ban_duration(),
            sync_to_xdp: true,
            collections: default_collections(),
            whitelist_cidrs: Vec::new(),
            enroll_key: None,
        }
    }
}

impl CrowdSecConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.lapi_listen.trim().is_empty() {
            return Err(ModelError::Validation(
                "crowdsec.lapi_listen cannot be empty when CrowdSec is enabled".to_string(),
            ));
        }
        if self.update_frequency_secs < 5 {
            return Err(ModelError::Validation(
                "crowdsec.update_frequency_secs must be >= 5".to_string(),
            ));
        }
        if self.ban_duration.trim().is_empty() {
            return Err(ModelError::Validation(
                "crowdsec.ban_duration cannot be empty".to_string(),
            ));
        }
        for entry in &self.whitelist_cidrs {
            let trimmed = entry.trim();
            if trimmed.parse::<IpAddr>().is_err() && trimmed.parse::<IpNet>().is_err() {
                return Err(ModelError::Validation(format!(
                    "crowdsec.whitelist_cidrs invalid IP/CIDR '{entry}'"
                )));
            }
        }
        Ok(())
    }
}

/// Active `CrowdSec` enforcement decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CrowdSecDecision {
    #[ts(type = "number")]
    pub id: i64,
    pub origin: String,
    pub decision_type: String,
    pub scope: String,
    pub value: String,
    pub duration: String,
    pub scenario: String,
}

/// Runtime status of `CrowdSec` agent & bouncer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[allow(clippy::struct_excessive_bools)]
pub struct CrowdSecStatus {
    pub enabled: bool,
    pub bouncer_enabled: bool,
    pub agent_running: bool,
    pub bouncer_running: bool,
    pub active_decisions: usize,
    pub decisions: Vec<CrowdSecDecision>,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
}

fn default_lapi_listen() -> String {
    "127.0.0.1:8080".to_string()
}

const fn default_true() -> bool {
    true
}

const fn default_update_freq() -> u32 {
    10
}

fn default_ban_duration() -> String {
    "4h".to_string()
}

fn default_collections() -> Vec<String> {
    vec![
        "crowdsecurity/linux".to_string(),
        "crowdsecurity/sshd".to_string(),
    ]
}
