pub mod acme;
pub mod cmd;
pub mod crabdns;
pub mod ddns;
pub mod dnsmasq;
pub mod ipv6_relay;
pub mod ota;
pub mod supervisor;
pub mod upnp;
pub mod wireguard;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ServiceError>;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config render error: {0}")]
    Render(String),
    #[error("service orchestration error: {0}")]
    Orchestration(String),
}

pub use acme::TlsCertManager;
pub use crabdns::render_crabdns_toml;
pub use ddns::{DdnsManager, spawn_ddns_engine, spawn_ddns_engine_with_shutdown};
pub use dnsmasq::{read_online_clients, render_dnsmasq_conf};
pub use ipv6_relay::{has_ipv6_relay_enabled, render_ndppd_conf};
pub use ota::{
    OtaCommandResult, OtaStatus, StagedOtaImage, apply_ota_update, auto_bless_if_tentative,
    bless_current_boot, check_ota_updates, compute_file_sha256, query_ota_status,
    render_sysupdate_root_transfer, render_sysupdate_uki_transfer, sanitize_ota_filename,
    trigger_reboot_via_dbus,
};
pub use supervisor::{DryRunSupervisor, ServiceSupervisor, SystemdSupervisor, SystemdUnitStatus};
pub use upnp::{parse_upnp_leases, read_upnp_leases, render_miniupnpd_conf};
pub use wireguard::{
    WireGuardKeypair, derive_public_key, generate_wireguard_keypair, render_wg_quick_conf,
    sync_wireguard_files,
};
