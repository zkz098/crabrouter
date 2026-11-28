pub mod acme;
pub mod app_store;
pub mod chrony;
pub mod cmd;
pub mod containers;
pub mod crabdns;
pub mod crowdsec;
pub mod ddns;
pub mod ddns_domestic;
pub mod dnsmasq;
pub mod iptv;
pub mod ipv6_relay;
pub mod mdns;
pub mod ota;
pub mod supervisor;
pub mod sysext;
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
pub use app_store::{
    install_catalog_app, list_installed_apps, toggle_catalog_app, uninstall_catalog_app,
};
pub use chrony::render_chrony_conf;
pub use containers::config::{
    render_containers_conf, render_containers_slice, render_registries_conf, render_storage_conf,
    sync_container_host_configs,
};
pub use containers::dry_run::DryRunContainerBackend;
pub use containers::socket::SocketContainerBackend;
pub use containers::{ContainerAction, ContainerBackend};
pub use crabdns::{render_crabdns_toml, render_crabdns_toml_with_isp_dns};
pub use crowdsec::{CrowdSecManager, render_crowdsec_acquis_yaml, render_crowdsec_bouncer_conf};
pub use ddns::{DdnsManager, spawn_ddns_engine, spawn_ddns_engine_with_shutdown};
pub use dnsmasq::{read_online_clients, render_dnsmasq_conf};
pub use iptv::{render_igmpproxy_conf, render_udpxy_env};
pub use ipv6_relay::{has_ipv6_relay_enabled, render_ndppd_conf};
pub use mdns::render_avahi_daemon_conf;
pub use ota::{
    OtaCommandResult, OtaStatus, StagedOtaImage, apply_ota_update, auto_bless_if_tentative,
    bless_current_boot, check_ota_updates, compute_file_sha256, query_ota_status,
    render_sysupdate_root_transfer, render_sysupdate_uki_transfer, sanitize_ota_filename,
    trigger_reboot_via_dbus,
};
pub use supervisor::{DryRunSupervisor, ServiceSupervisor, SystemdSupervisor, SystemdUnitStatus};
pub use sysext::{query_sysext_status, refresh_sysext, remove_sysext_image, unmerge_sysext};
pub use upnp::{parse_upnp_leases, read_upnp_leases, render_miniupnpd_conf};
pub use wireguard::{
    WireGuardKeypair, derive_public_key, generate_wireguard_keypair, render_wg_quick_conf,
    sync_wireguard_files,
};
