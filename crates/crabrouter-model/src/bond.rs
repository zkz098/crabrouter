use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{ModelError, Result};
use crate::network::LanConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum BondMode {
    BalanceRr,
    ActiveBackup,
    BalanceXor,
    Broadcast,
    #[default]
    #[serde(rename = "802.3ad")]
    Ieee8023ad,
    BalanceTlb,
    BalanceAlb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum BondXmitHashPolicy {
    Layer2,
    #[default]
    #[serde(rename = "layer2+3")]
    Layer23,
    #[serde(rename = "layer3+4")]
    Layer34,
    #[serde(rename = "encap2+3")]
    Encap23,
    #[serde(rename = "encap3+4")]
    Encap34,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[ts(export)]
#[serde(rename_all = "kebab-case")]
pub enum BondLacpRate {
    #[default]
    Slow,
    Fast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BondConfig {
    pub name: String,
    pub slaves: Vec<String>,
    #[serde(default)]
    pub mode: BondMode,
    #[serde(default)]
    pub xmit_hash_policy: BondXmitHashPolicy,
    #[serde(default)]
    pub lacp_rate: BondLacpRate,
    #[serde(default = "default_miimon_ms")]
    pub miimon_ms: u32,
    #[serde(default)]
    pub mtu: Option<u32>,
    #[serde(default)]
    pub primary: Option<String>,
}

const fn default_miimon_ms() -> u32 {
    100
}

/// Validates declarative link-aggregation (`bond`) definitions and checks for port conflicts.
pub fn validate_bonds(bonds: &[BondConfig], lans: &[LanConfig]) -> Result<()> {
    let mut bond_names = HashSet::new();
    let mut claimed_slaves = HashSet::new();
    let bridge_ports: HashSet<&str> = lans
        .iter()
        .flat_map(|lan| lan.ports.iter().map(String::as_str))
        .collect();

    for bond in bonds {
        validate_single_bond(bond, &mut bond_names, &mut claimed_slaves, &bridge_ports)?;
    }
    Ok(())
}

fn validate_single_bond<'a>(
    bond: &'a BondConfig,
    bond_names: &mut HashSet<&'a str>,
    claimed_slaves: &mut HashSet<&'a str>,
    bridge_ports: &HashSet<&str>,
) -> Result<()> {
    let name = bond.name.trim();
    if name.is_empty() || name.len() > 15 {
        return Err(ModelError::Validation(format!(
            "bond name '{}' must be 1..=15 characters",
            bond.name
        )));
    }
    if !bond_names.insert(name) {
        return Err(ModelError::Validation(format!(
            "duplicate bond interface '{name}'"
        )));
    }
    if bond.slaves.is_empty() {
        return Err(ModelError::Validation(format!(
            "bond '{name}' must specify at least one slave interface"
        )));
    }
    if let Some(mtu) = bond.mtu
        && !(576..=9216).contains(&mtu)
    {
        return Err(ModelError::Validation(format!(
            "bond '{name}' mtu {mtu} must be between 576 and 9216"
        )));
    }

    for slave in &bond.slaves {
        let s = slave.trim();
        if s.is_empty() || s == name {
            return Err(ModelError::Validation(format!(
                "bond '{name}' has invalid slave '{slave}'"
            )));
        }
        if !claimed_slaves.insert(s) {
            return Err(ModelError::Validation(format!(
                "interface '{s}' is assigned to multiple bonds"
            )));
        }
        if bridge_ports.contains(s) {
            return Err(ModelError::Validation(format!(
                "bond '{name}' slave '{s}' cannot also be directly attached to a LAN bridge; attach '{name}' to the bridge instead"
            )));
        }
    }

    if let Some(primary) = bond.primary.as_deref()
        && !bond.slaves.iter().any(|s| s == primary)
    {
        return Err(ModelError::Validation(format!(
            "bond '{name}' primary slave '{primary}' must be in slaves list"
        )));
    }
    Ok(())
}
