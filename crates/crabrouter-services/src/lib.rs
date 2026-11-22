pub mod crabdns;
pub mod dnsmasq;
pub mod supervisor;
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

pub use crabdns::render_crabdns_toml;
pub use dnsmasq::{read_online_clients, render_dnsmasq_conf};
pub use supervisor::{DryRunSupervisor, ServiceSupervisor, SystemdSupervisor, SystemdUnitStatus};
pub use wireguard::{
    WireGuardKeypair, derive_public_key, generate_wireguard_keypair, render_wg_quick_conf,
    sync_wireguard_files,
};
