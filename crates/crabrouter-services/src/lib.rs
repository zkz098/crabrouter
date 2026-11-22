pub mod crabdns;
pub mod dnsmasq;
pub mod supervisor;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ServiceError>;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("service render error: {0}")]
    Render(String),
    #[error("service orchestration error: {0}")]
    Orchestration(String),
}

pub use crabdns::render_crabdns_toml;
pub use dnsmasq::{read_online_clients, render_dnsmasq_conf};
pub use supervisor::{DryRunSupervisor, ServiceSupervisor, SystemdSupervisor};
