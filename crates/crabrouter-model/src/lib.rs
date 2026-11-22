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
    DhcpLeaseEntry, DhcpStaticLease, HostInterfaceSnapshot, InterfacePin, InterfaceRole,
    Ipv6PdSlice, LanConfig, PolicyRuleConfig, StaticRouteConfig, VlanConfig, WanConfig, WanProto,
    WanRuntimeState,
};
