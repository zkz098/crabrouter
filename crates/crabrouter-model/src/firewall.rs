use serde::{Deserialize, Serialize};
use ts_rs::TS;

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

const fn default_true() -> bool {
    true
}

const fn default_syn_flood_rate() -> u32 {
    25
}
