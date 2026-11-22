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
    #[serde(default = "default_wan_weight")]
    pub weight: u32,
    #[serde(default)]
    pub table_id: Option<u32>,
    #[serde(default = "default_true")]
    pub health_check_enabled: bool,
    #[serde(default)]
    pub probe_targets: Vec<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MultiWanMode {
    #[default]
    Failover,
    LoadBalance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProbeProtocol {
    #[default]
    Icmp,
    Http,
    IcmpAndHttp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MultiWanConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: MultiWanMode,
    #[serde(default)]
    pub probe_protocol: ProbeProtocol,
    #[serde(default = "default_probe_targets")]
    pub probe_targets: Vec<String>,
    #[serde(default = "default_probe_interval_secs")]
    pub probe_interval_secs: u32,
    #[serde(default = "default_probe_timeout_ms")]
    pub probe_timeout_ms: u32,
    #[serde(default = "default_fail_threshold")]
    pub fail_threshold: u8,
    #[serde(default = "default_recover_threshold")]
    pub recover_threshold: u8,
    #[serde(default = "default_true")]
    pub sticky_sessions: bool,
    #[serde(default = "default_true")]
    pub flush_conntrack_on_failover: bool,
}

impl Default for MultiWanConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: MultiWanMode::default(),
            probe_protocol: ProbeProtocol::default(),
            probe_targets: default_probe_targets(),
            probe_interval_secs: default_probe_interval_secs(),
            probe_timeout_ms: default_probe_timeout_ms(),
            fail_threshold: default_fail_threshold(),
            recover_threshold: default_recover_threshold(),
            sticky_sessions: true,
            flush_conntrack_on_failover: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WanHealthState {
    #[default]
    Unknown,
    Healthy,
    Degraded,
    Down,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WanProbeResult {
    pub wan_name: String,
    pub device: String,
    pub table_id: u32,
    pub fwmark: u32,
    pub weight: u32,
    pub metric: u32,
    pub health: WanHealthState,
    pub active: bool,
    pub effective_weight: u32,
    pub rtt_ms: Option<f64>,
    pub packet_loss_pct: u8,
    pub consecutive_failures: u8,
    pub consecutive_successes: u8,
    pub last_target: Option<String>,
    pub last_error: Option<String>,
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MultiWanRuntimeStatus {
    pub enabled: bool,
    pub mode: MultiWanMode,
    pub active_wans: Vec<String>,
    pub wans: Vec<WanProbeResult>,
    pub last_failover_at: Option<String>,
    #[ts(type = "number")]
    pub failover_count: u64,
}

const fn default_wan_weight() -> u32 {
    1
}

/// Returns the deterministic routing table ID for a WAN interface.
pub fn wan_table_id(idx: usize, wan: &WanConfig) -> u32 {
    wan.table_id.unwrap_or(101 + idx as u32)
}

/// Returns the `fwmark` value associated with a WAN routing table ID.
pub const fn wan_fwmark(table_id: u32) -> u32 {
    0x1000 + table_id
}

fn default_probe_targets() -> Vec<String> {
    vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()]
}

const fn default_probe_interval_secs() -> u32 {
    5
}

const fn default_probe_timeout_ms() -> u32 {
    2000
}

const fn default_fail_threshold() -> u8 {
    3
}

const fn default_recover_threshold() -> u8 {
    2
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
