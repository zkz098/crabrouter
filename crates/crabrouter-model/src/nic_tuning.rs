use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};

/// Preset profile for `NIC` driver-level tuning (`ethtool` + `RPS`/`XPS`/`RFS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NicTuningProfile {
    #[default]
    Default,
    LowLatency,
    HighThroughput,
    Custom,
}

/// Receive Packet Steering (`RPS`) mode for software multi-core receive scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RpsMode {
    #[default]
    Off,
    Auto,
    Custom,
}

/// Declarative per-interface driver tuning policy (`ethtool` offloads, pause, ring buffers, `RPS`/`XPS`/`RFS`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NicTuningConfig {
    pub interface: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub profile: NicTuningProfile,
    /// Generic Receive Offload (`GRO`).
    #[serde(default)]
    pub gro: Option<bool>,
    /// Generic Segmentation Offload (`GSO`).
    #[serde(default)]
    pub gso: Option<bool>,
    /// TCP Segmentation Offload (`TSO`).
    #[serde(default)]
    pub tso: Option<bool>,
    /// Large Receive Offload (`LRO`, recommended disabled on routers).
    #[serde(default)]
    pub lro: Option<bool>,
    /// Receive checksum offload.
    #[serde(default)]
    pub rx_checksum: Option<bool>,
    /// Transmit checksum offload.
    #[serde(default)]
    pub tx_checksum: Option<bool>,
    /// Receive pause frame flow control.
    #[serde(default)]
    pub pause_rx: Option<bool>,
    /// Transmit pause frame flow control.
    #[serde(default)]
    pub pause_tx: Option<bool>,
    /// Receive ring buffer descriptor count.
    #[serde(default)]
    pub rx_ring: Option<u32>,
    /// Transmit ring buffer descriptor count.
    #[serde(default)]
    pub tx_ring: Option<u32>,
    /// `RPS` steering mode (`off`, `auto`, or `custom`).
    #[serde(default)]
    pub rps_mode: RpsMode,
    /// Explicit hexadecimal `CPU` bitmask when `rps_mode == custom` (e.g. `"0e"` or `"ff"`).
    #[serde(default)]
    pub rps_mask: Option<String>,
    /// Enable Transmit Packet Steering (`XPS`).
    #[serde(default)]
    pub xps_enabled: bool,
    /// Receive Flow Steering (`RFS`) global flow table entries (e.g. `32768`).
    #[serde(default)]
    pub rfs_flow_entries: Option<u32>,
}

/// Effective resolved driver tuning parameters after merging `profile` defaults with explicit overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveNicTuning {
    pub gro: Option<bool>,
    pub gso: Option<bool>,
    pub tso: Option<bool>,
    pub lro: Option<bool>,
    pub rx_checksum: Option<bool>,
    pub tx_checksum: Option<bool>,
    pub pause_rx: Option<bool>,
    pub pause_tx: Option<bool>,
    pub rx_ring: Option<u32>,
    pub tx_ring: Option<u32>,
    pub rps_mode: RpsMode,
    pub rps_mask: Option<String>,
    pub xps_enabled: bool,
    pub rfs_flow_entries: Option<u32>,
}

impl NicTuningConfig {
    /// Resolves effective tuning values by applying `profile` defaults beneath explicit overrides.
    pub fn resolve_effective(&self) -> EffectiveNicTuning {
        match self.profile {
            NicTuningProfile::Default | NicTuningProfile::Custom => EffectiveNicTuning {
                gro: self.gro,
                gso: self.gso,
                tso: self.tso,
                lro: self.lro,
                rx_checksum: self.rx_checksum,
                tx_checksum: self.tx_checksum,
                pause_rx: self.pause_rx,
                pause_tx: self.pause_tx,
                rx_ring: self.rx_ring,
                tx_ring: self.tx_ring,
                rps_mode: self.rps_mode,
                rps_mask: self.rps_mask.clone(),
                xps_enabled: self.xps_enabled,
                rfs_flow_entries: self.rfs_flow_entries,
            },
            NicTuningProfile::LowLatency => EffectiveNicTuning {
                gro: self.gro,
                gso: self.gso,
                tso: self.tso,
                lro: self.lro.or(Some(false)),
                rx_checksum: self.rx_checksum.or(Some(true)),
                tx_checksum: self.tx_checksum.or(Some(true)),
                pause_rx: self.pause_rx.or(Some(false)),
                pause_tx: self.pause_tx.or(Some(false)),
                rx_ring: self.rx_ring,
                tx_ring: self.tx_ring,
                rps_mode: if self.rps_mode == RpsMode::Off {
                    RpsMode::Auto
                } else {
                    self.rps_mode
                },
                rps_mask: self.rps_mask.clone(),
                xps_enabled: true,
                rfs_flow_entries: self.rfs_flow_entries.or(Some(32_768)),
            },
            NicTuningProfile::HighThroughput => EffectiveNicTuning {
                gro: self.gro.or(Some(true)),
                gso: self.gso.or(Some(true)),
                tso: self.tso.or(Some(true)),
                lro: self.lro.or(Some(false)),
                rx_checksum: self.rx_checksum.or(Some(true)),
                tx_checksum: self.tx_checksum.or(Some(true)),
                pause_rx: self.pause_rx,
                pause_tx: self.pause_tx,
                rx_ring: self.rx_ring,
                tx_ring: self.tx_ring,
                rps_mode: if self.rps_mode == RpsMode::Off {
                    RpsMode::Auto
                } else {
                    self.rps_mode
                },
                rps_mask: self.rps_mask.clone(),
                xps_enabled: true,
                rfs_flow_entries: self.rfs_flow_entries.or(Some(65_536)),
            },
        }
    }
}

/// Live state of an `ethtool` offload feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OffloadFeatureState {
    pub enabled: bool,
    pub fixed: bool,
}

/// Probed hardware offload capabilities for a network interface.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NicOffloadCapabilities {
    pub gro: Option<OffloadFeatureState>,
    pub gso: Option<OffloadFeatureState>,
    pub tso: Option<OffloadFeatureState>,
    pub lro: Option<OffloadFeatureState>,
    pub rx_checksum: Option<OffloadFeatureState>,
    pub tx_checksum: Option<OffloadFeatureState>,
}

/// Read-only snapshot of an interface's `NIC` driver capabilities and current `RPS`/`XPS`/`RFS` state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NicCapabilitiesSnapshot {
    pub interface: String,
    pub driver: Option<String>,
    pub driver_version: Option<String>,
    pub bus_info: Option<String>,
    pub ethtool_available: bool,
    pub offloads: NicOffloadCapabilities,
    pub pause_autoneg: Option<bool>,
    pub pause_rx: Option<bool>,
    pub pause_tx: Option<bool>,
    pub rx_ring_current: Option<u32>,
    pub rx_ring_max: Option<u32>,
    pub tx_ring_current: Option<u32>,
    pub tx_ring_max: Option<u32>,
    pub rx_queues: u32,
    pub tx_queues: u32,
    pub cpu_cores: u32,
    pub current_rps_cpus: Option<String>,
    pub current_xps_cpus: Option<String>,
    pub current_rfs_entries: Option<u32>,
}

/// Validates a list of `NIC` tuning policies.
pub fn validate_nic_tuning(items: &[NicTuningConfig]) -> Result<()> {
    let mut seen = HashSet::new();
    for item in items {
        let iface = item.interface.trim();
        if iface.is_empty()
            || iface.len() > 15
            || iface.contains(char::is_whitespace)
            || iface.contains('/')
        {
            return Err(ModelError::Validation(format!(
                "nic_tuning invalid interface name '{}'",
                item.interface
            )));
        }
        if !seen.insert(iface) {
            return Err(ModelError::Validation(format!(
                "duplicate nic_tuning entry for interface '{iface}'"
            )));
        }
        validate_ring_size(iface, "rx_ring", item.rx_ring)?;
        validate_ring_size(iface, "tx_ring", item.tx_ring)?;

        if item.rps_mode == RpsMode::Custom {
            let mask = item.rps_mask.as_deref().map_or("", str::trim);
            if !is_valid_cpu_mask(mask) {
                return Err(ModelError::Validation(format!(
                    "nic_tuning '{iface}' invalid custom rps_mask '{mask}'"
                )));
            }
        }
        if let Some(entries) = item.rfs_flow_entries
            && entries > 262_144
        {
            return Err(ModelError::Validation(format!(
                "nic_tuning '{iface}' rfs_flow_entries {entries} exceeds max 262144"
            )));
        }
    }
    Ok(())
}

fn validate_ring_size(iface: &str, field: &str, val: Option<u32>) -> Result<()> {
    if let Some(v) = val
        && !(64..=16_384).contains(&v)
    {
        return Err(ModelError::Validation(format!(
            "nic_tuning '{iface}' {field} {v} must be within 64..=16384"
        )));
    }
    Ok(())
}

/// Validates a Linux `sysfs` `CPU` bitmask (comma-separated 1..=8 hex digit chunks, non-zero).
pub fn is_valid_cpu_mask(mask: &str) -> bool {
    let trimmed = mask.trim();
    if trimmed.is_empty() {
        return false;
    }
    let mut any_nonzero = false;
    for chunk in trimmed.split(',') {
        if chunk.is_empty() || chunk.len() > 8 || !chunk.bytes().all(|b| b.is_ascii_hexdigit()) {
            return false;
        }
        if chunk.bytes().any(|b| b != b'0') {
            any_nonzero = true;
        }
    }
    any_nonzero
}

const fn default_true() -> bool {
    true
}
