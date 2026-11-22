pub mod dhcpcd;
pub mod events;
pub mod ipv6_pd;
pub mod netlink;
pub mod nftables;
pub mod pins;
pub mod pppoe;
pub mod telemetry;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, NetError>;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("network apply error: {0}")]
    Apply(String),
}

pub use dhcpcd::{WanLeaseEvent, render_dhcpcd_conf, render_dhcpcd_hook_script, sync_dhcpcd_files};
pub use events::{WanEventBus, WanStateObserver, spawn_network_event_engine};
pub use ipv6_pd::{
    DownstreamIpv6Target, apply_ipv6_pd_slices_netlink, resolve_downstream_ipv6_targets,
    slice_delegated_ipv6_prefix,
};
pub use netlink::{
    DryRunNetworkBackend, LinuxNetworkBackend, NetworkBackend, NetworkDiffPlan, NetworkPaths,
    compute_network_diff_plan,
};
pub use nftables::{
    apply_nft_with_precheck, check_nft_syntax, render_nftables_ruleset,
    render_nftables_ruleset_with_options,
};
pub use pins::{render_systemd_link_files, render_udev_net_rules, sync_interface_pin_files};
pub use pppoe::{
    PppdManager, render_ppp_secrets, render_pppd_hook_script, render_pppd_peer, sync_pppd_files,
};
pub use telemetry::read_host_interfaces;
