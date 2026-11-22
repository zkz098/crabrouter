pub mod db;
pub mod store;
pub mod transaction;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("validation failed: {0}")]
    Model(#[from] crabrouter_model::ModelError),
    #[error("network layer error: {0}")]
    Net(#[from] crabrouter_net::NetError),
    #[error("service orchestration error: {0}")]
    Service(#[from] crabrouter_services::ServiceError),
    #[error("config storage error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config parse/encode error: {0}")]
    Toml(String),
    #[error("transaction error: {0}")]
    Transaction(String),
    #[error("state database error: {0}")]
    Db(String),
    #[error("authentication error: {0}")]
    Auth(String),
}

pub use db::{ConfigSnapshot, ConfigSnapshotSummary, StateDb};
pub use store::ConfigStore;
pub use transaction::{CommitStatus, PendingCommit, RouterEngine};
