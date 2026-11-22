use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum InterfaceRole {
    Wan,
    #[default]
    Lan,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WanProto {
    #[default]
    Dhcp,
    Static,
    Pppoe,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WanConfig {
    pub name: String,
    pub device: String,
    pub proto: WanProto,
    #[serde(default)]
    pub static_cidr: Option<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub pppoe_username: Option<String>,
    #[serde(default)]
    pub pppoe_password: Option<String>,
    #[serde(default = "default_mtu")]
    pub mtu: u16,
    #[serde(default = "default_true")]
    pub ipv6_pd: bool,
    #[serde(default = "default_ipv6_pd_prefix_len")]
    pub ipv6_pd_prefix_len: u8,
    #[serde(default = "default_metric")]
    pub metric: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DhcpStaticLease {
    pub name: String,
    pub mac: String,
    pub ip: String,
    #[serde(default)]
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LanConfig {
    pub name: String,
    pub bridge: String,
    pub ports: Vec<String>,
    pub cidr: String,
    #[serde(default = "default_domain")]
    pub domain: String,
    #[serde(default = "default_true")]
    pub dhcp_enabled: bool,
    #[serde(default = "default_dhcp_start")]
    pub dhcp_start_offset: u32,
    #[serde(default = "default_dhcp_limit")]
    pub dhcp_limit: u32,
    #[serde(default = "default_lease_time")]
    pub dhcp_lease_secs: u32,
    #[serde(default)]
    pub static_leases: Vec<DhcpStaticLease>,
    #[serde(default = "default_true")]
    pub ipv6_ra_enabled: bool,
    #[serde(default)]
    pub ipv6_subnet_id: Option<u8>,
    #[serde(default)]
    pub ipv6_ula_cidr: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VlanConfig {
    pub parent: String,
    pub id: u16,
    pub role: InterfaceRole,
    #[serde(default)]
    pub cidr: Option<String>,
    #[serde(default = "default_true")]
    pub ipv6_ra_enabled: bool,
    #[serde(default)]
    pub ipv6_subnet_id: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InterfacePin {
    pub mac: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StaticRouteConfig {
    #[serde(default = "default_main_table")]
    pub table_id: u32,
    pub destination: String,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub metric: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PolicyRuleConfig {
    pub priority: u32,
    #[serde(default)]
    pub src: Option<String>,
    #[serde(default)]
    pub dst: Option<String>,
    #[serde(default)]
    pub iif: Option<String>,
    pub table_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HostInterfaceSnapshot {
    pub name: String,
    pub mac: Option<String>,
    pub oper_state: String,
    pub mtu: u32,
    pub speed_mbps: Option<u32>,
    #[ts(type = "number")]
    pub rx_bytes: u64,
    #[ts(type = "number")]
    pub tx_bytes: u64,
    #[ts(type = "number")]
    pub rx_packets: u64,
    #[ts(type = "number")]
    pub tx_packets: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DhcpLeaseEntry {
    #[ts(type = "number")]
    pub expires_at: u64,
    pub mac: String,
    pub ip: String,
    pub hostname: String,
    pub client_id: Option<String>,
    pub is_static: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Ipv6PdSlice {
    pub interface: String,
    pub subnet_id: u8,
    pub prefix_cidr: String,
    pub router_addr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WanRuntimeState {
    pub wan_name: String,
    pub device: String,
    pub proto: WanProto,
    pub state: String,
    pub ip: Option<String>,
    pub gateway: Option<String>,
    pub dns_servers: Vec<String>,
    pub ifindex: Option<u32>,
    #[serde(default)]
    pub ipv6_addr: Option<String>,
    #[serde(default)]
    pub ipv6_pd_prefix: Option<String>,
    #[serde(default)]
    pub ipv6_slices: Vec<Ipv6PdSlice>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum QdiscAlgo {
    #[default]
    Cake,
    FqCodel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CakeDiffserv {
    #[default]
    Diffserv4,
    Diffserv3,
    BestEffort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QosInterfaceConfig {
    pub interface: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub algorithm: QdiscAlgo,
    /// Egress (upload) shaping rate in Kbit/s (`0` = unlimited line-rate).
    #[serde(default)]
    pub upload_kbps: u32,
    /// Ingress (download) shaping rate in Kbit/s via IFB redirect (`0` = disabled).
    #[serde(default)]
    pub download_kbps: u32,
    /// Framing overhead in bytes (`18` Ethernet, `26` `PPPoE`, `44` VDSL2/FTTP).
    #[serde(default = "default_overhead_bytes")]
    pub overhead_bytes: i16,
    /// Enable NAT-aware per-host fairness (`nat dual-srchost` / `nat dual-dsthost`).
    #[serde(default = "default_true")]
    pub nat_aware: bool,
    /// Enable TCP ACK filtering on egress (`ack-filter`).
    #[serde(default)]
    pub ack_filter: bool,
    /// Diffserv priority tin profile for CAKE.
    #[serde(default)]
    pub diffserv: CakeDiffserv,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct QosConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub interfaces: Vec<QosInterfaceConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConntrackEntry {
    pub family: String,
    pub proto: String,
    pub state: String,
    pub src: String,
    pub dst: String,
    pub sport: Option<u16>,
    pub dport: Option<u16>,
    #[ts(type = "number")]
    pub timeout_secs: u64,
    #[ts(type = "number")]
    pub orig_packets: u64,
    #[ts(type = "number")]
    pub orig_bytes: u64,
    #[ts(type = "number")]
    pub reply_packets: u64,
    #[ts(type = "number")]
    pub reply_bytes: u64,
    pub mark: u32,
    pub assured: bool,
    pub offloaded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConntrackKillRequest {
    pub family: Option<String>,
    pub proto: String,
    pub src: String,
    pub dst: String,
    pub sport: Option<u16>,
    pub dport: Option<u16>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default, TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HealthSeverity {
    #[default]
    Ok,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThermalSensorReading {
    pub chip: String,
    pub label: String,
    pub temp_celsius: f32,
    pub high_celsius: Option<f32>,
    pub crit_celsius: Option<f32>,
    pub severity: HealthSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiskHealthReport {
    pub device: String,
    pub model: String,
    pub serial: Option<String>,
    pub protocol: String,
    pub passed: bool,
    pub temperature_celsius: Option<f32>,
    /// Percentage of rated SSD life used (`0..=100`).
    pub percentage_used: Option<u8>,
    /// Available spare flash capacity percentage (`0..=100`).
    pub available_spare_pct: Option<u8>,
    #[ts(type = "number | null")]
    pub power_on_hours: Option<u64>,
    #[ts(type = "number | null")]
    pub unsafe_shutdowns: Option<u64>,
    #[ts(type = "number | null")]
    pub media_errors: Option<u64>,
    pub severity: HealthSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HardwareAlert {
    pub severity: HealthSeverity,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HardwareHealthSnapshot {
    pub collected_at: String,
    pub overall_severity: HealthSeverity,
    pub sensors: Vec<ThermalSensorReading>,
    pub disks: Vec<DiskHealthReport>,
    pub alerts: Vec<HardwareAlert>,
}

const fn default_overhead_bytes() -> i16 {
    18
}

const fn default_mtu() -> u16 {
    1500
}

const fn default_true() -> bool {
    true
}

const fn default_ipv6_pd_prefix_len() -> u8 {
    60
}

const fn default_metric() -> u32 {
    100
}

const fn default_main_table() -> u32 {
    254
}

fn default_domain() -> String {
    "lan".to_string()
}

const fn default_dhcp_start() -> u32 {
    100
}

const fn default_dhcp_limit() -> u32 {
    150
}

const fn default_lease_time() -> u32 {
    43_200
}
