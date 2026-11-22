pub mod config;
pub mod error;
pub mod firewall;
pub mod network;

pub use config::{
    DnsIntegrationConfig, RouterConfig, SystemConfig, WireGuardConfig, WireGuardPeer,
};
pub use error::{ModelError, Result};
pub use firewall::{
    FirewallAction, FirewallConfig, FirewallRule, FirewallZone, PortForwardRule, TransportProto,
};
pub use network::{
    AcmeChallengeType, AcmeConfig, CakeDiffserv, ConntrackEntry, ConntrackKillRequest, DdnsConfig,
    DdnsProvider, DdnsRuntimeStatus, DhcpLeaseEntry, DhcpStaticLease, DiskHealthReport,
    DomainPolicyRule, EbpfConfig, EbpfRuntimeStatus, EbpfTrafficStat, HardwareAlert,
    HardwareHealthSnapshot, HealthSeverity, HostInterfaceSnapshot, InterfacePin, InterfaceRole,
    Ipv6PdSlice, LanConfig, MultiWanConfig, MultiWanMode, MultiWanRuntimeStatus, PolicyRuleConfig,
    ProbeProtocol, QdiscAlgo, QosConfig, QosInterfaceConfig, StaticRouteConfig,
    ThermalSensorReading, TlsRuntimeStatus, VlanConfig, WanConfig, WanHealthState, WanProbeResult,
    WanProto, WanRuntimeState, wan_fwmark, wan_table_id,
};
