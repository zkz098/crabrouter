use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use crabrouter_model::{RouterConfig, WanRuntimeState};
use crabrouter_net::{NetworkBackend, WanStateObserver};
use crabrouter_services::ServiceSupervisor;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use ts_rs::TS;

use crate::db::{ConfigSnapshot, ConfigSnapshotSummary, PendingMarker, StateDb};
use crate::store::ConfigStore;
use crate::{CoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PendingCommit {
    #[ts(type = "number")]
    pub generation: u64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CommitStatus {
    #[ts(type = "number")]
    pub active_generation: u64,
    pub pending: Option<PendingCommit>,
}

struct TxState {
    generation: u64,
    pending: Option<PendingCommitEntry>,
}

struct PendingCommitEntry {
    generation: u64,
    expires_at: DateTime<Utc>,
    previous_config: RouterConfig,
    rollback_task: JoinHandle<()>,
}

pub struct RouterEngine {
    store: ConfigStore,
    db: StateDb,
    active: ArcSwap<RouterConfig>,
    net: Arc<dyn NetworkBackend>,
    services: Arc<dyn ServiceSupervisor>,
    tx: Mutex<TxState>,
}

impl RouterEngine {
    pub async fn bootstrap(
        store: ConfigStore,
        net: Arc<dyn NetworkBackend>,
        services: Arc<dyn ServiceSupervisor>,
    ) -> Result<Arc<Self>> {
        let db_path = store
            .path()
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(
                || std::path::PathBuf::from("state.redb"),
                |p| p.join("state.redb"),
            );
        let db = StateDb::open(&db_path)?;

        // Crash/power-loss recovery: if a pending commit was never confirmed before reboot,
        // ensure last-good config is restored to router.toml and cleared from pending table.
        if let Some(stale) = db.take_pending_marker()? {
            tracing::warn!(
                generation = stale.generation,
                "detected unconfirmed pending commit from previous boot; restoring last-good config"
            );
            if let Ok(prev) = toml::from_str::<RouterConfig>(&stale.previous_toml)
                && prev.validate().is_ok()
            {
                store.save_atomic(&prev)?;
            }
        }

        let initial = store.load_or_init()?;
        let generation = db.latest_generation()?.max(1);
        let engine = Arc::new(Self {
            store,
            db,
            active: ArcSwap::from_pointee(initial.clone()),
            net,
            services,
            tx: Mutex::new(TxState {
                generation,
                pending: None,
            }),
        });
        engine.apply_runtime(&initial).await?;
        Ok(engine)
    }

    pub fn db(&self) -> &StateDb {
        &self.db
    }

    pub fn active_config(&self) -> Arc<RouterConfig> {
        self.active.load_full()
    }

    pub async fn status(&self) -> CommitStatus {
        let guard = self.tx.lock().await;
        CommitStatus {
            active_generation: guard.generation,
            pending: guard.pending.as_ref().map(|p| PendingCommit {
                generation: p.generation,
                expires_at: p.expires_at,
            }),
        }
    }

    pub fn list_history(&self) -> Result<Vec<ConfigSnapshotSummary>> {
        self.db.list_history_summaries()
    }

    pub async fn restore_snapshot(self: &Arc<Self>, generation: u64) -> Result<CommitStatus> {
        let snap = self.db.get_history_snapshot(generation)?.ok_or_else(|| {
            CoreError::Transaction(format!("snapshot generation {generation} not found"))
        })?;
        let cfg: RouterConfig =
            toml::from_str(&snap.config_toml).map_err(|e| CoreError::Toml(e.to_string()))?;
        self.begin_commit(cfg).await
    }

    /// Applies `next` immediately and arms an automatic rollback timer unless `confirm_commit`
    /// is called before `commit_confirm_timeout_secs` elapses.
    pub async fn begin_commit(self: &Arc<Self>, next: RouterConfig) -> Result<CommitStatus> {
        next.validate()?;

        let mut guard = self.tx.lock().await;
        if guard.pending.is_some() {
            return Err(CoreError::Transaction(
                "a pending commit already awaits confirmation; confirm or rollback first"
                    .to_string(),
            ));
        }

        let previous = (*self.active.load_full()).clone();
        let previous_toml =
            toml::to_string_pretty(&previous).map_err(|e| CoreError::Toml(e.to_string()))?;
        let candidate_toml =
            toml::to_string_pretty(&next).map_err(|e| CoreError::Toml(e.to_string()))?;

        let timeout_secs = next.system.commit_confirm_timeout_secs.max(5);
        let generation = guard.generation + 1;
        let expires_at =
            Utc::now() + chrono::Duration::seconds(i64::try_from(timeout_secs).unwrap_or(60));

        self.db.write_pending_marker(&PendingMarker {
            generation,
            expires_at,
            previous_toml,
            candidate_toml,
        })?;

        if let Err(err) = self.apply_runtime(&next).await {
            let _ = self.db.clear_pending_marker();
            return Err(err);
        }

        self.active.store(Arc::new(next));
        guard.generation = generation;

        let weak_self = Arc::downgrade(self);
        let rollback_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(timeout_secs)).await;
            if let Some(engine) = weak_self.upgrade()
                && let Err(err) = engine.rollback_if_generation(generation).await
            {
                tracing::error!(generation, error = %err, "auto-rollback failed");
            }
        });

        guard.pending = Some(PendingCommitEntry {
            generation,
            expires_at,
            previous_config: previous,
            rollback_task,
        });

        Ok(CommitStatus {
            active_generation: generation,
            pending: Some(PendingCommit {
                generation,
                expires_at,
            }),
        })
    }

    pub async fn confirm_commit(&self) -> Result<CommitStatus> {
        let mut guard = self.tx.lock().await;
        let Some(entry) = guard.pending.take() else {
            return Err(CoreError::Transaction(
                "no pending commit awaiting confirmation".to_string(),
            ));
        };
        entry.rollback_task.abort();
        let active = self.active.load_full();
        self.store.save_atomic(&active)?;
        let config_toml =
            toml::to_string_pretty(&*active).map_err(|e| CoreError::Toml(e.to_string()))?;
        let summary = format!(
            "hostname={}, wans={}, lans={}, vlans={}, fw_rules={}",
            active.system.hostname,
            active.wans.len(),
            active.lans.len(),
            active.vlans.len(),
            active.firewall.rules.len(),
        );
        self.db.save_history_snapshot(&ConfigSnapshot {
            generation: guard.generation,
            created_at: Utc::now(),
            summary,
            config_toml,
        })?;
        self.db.clear_pending_marker()?;

        tracing::info!(
            generation = guard.generation,
            "confirmed and persisted router config"
        );
        Ok(CommitStatus {
            active_generation: guard.generation,
            pending: None,
        })
    }

    pub async fn rollback_pending(&self) -> Result<CommitStatus> {
        let mut guard = self.tx.lock().await;
        let Some(entry) = guard.pending.take() else {
            return Err(CoreError::Transaction(
                "no pending commit to rollback".to_string(),
            ));
        };
        entry.rollback_task.abort();
        self.apply_runtime(&entry.previous_config).await?;
        self.active.store(Arc::new(entry.previous_config));
        self.db.clear_pending_marker()?;
        guard.generation += 1;
        tracing::warn!(
            generation = guard.generation,
            "rolled back unconfirmed router config"
        );
        Ok(CommitStatus {
            active_generation: guard.generation,
            pending: None,
        })
    }

    async fn rollback_if_generation(&self, target_gen: u64) -> Result<()> {
        let mut guard = self.tx.lock().await;
        let Some(ref entry) = guard.pending else {
            return Ok(());
        };
        if entry.generation != target_gen {
            return Ok(());
        }
        let entry = guard.pending.take().expect("checked above");
        tracing::warn!(
            generation = target_gen,
            "commit-confirm timeout expired; restoring previous router config"
        );
        self.apply_runtime(&entry.previous_config).await?;
        self.active.store(Arc::new(entry.previous_config));
        self.db.clear_pending_marker()?;
        guard.generation += 1;
        Ok(())
    }

    async fn apply_runtime(&self, cfg: &RouterConfig) -> Result<()> {
        self.net.apply_network_and_firewall(cfg).await?;
        self.services.sync_services(cfg).await?;
        Ok(())
    }
}

impl WanStateObserver for RouterEngine {
    fn current_config(&self) -> RouterConfig {
        (*self.active.load_full()).clone()
    }

    fn on_wan_state_updated(&self, state: WanRuntimeState) {
        if let Err(err) = self.db.upsert_wan_state(&state) {
            tracing::warn!(wan = %state.wan_name, error = %err, "failed to persist WAN state in redb");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabrouter_net::DryRunNetworkBackend;
    use crabrouter_services::DryRunSupervisor;

    #[tokio::test]
    async fn commit_confirm_persists_new_hostname() {
        let dir = std::env::temp_dir().join(format!("crabrouter-tx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ConfigStore::new(dir.join("router.toml"));
        let engine = RouterEngine::bootstrap(
            store.clone(),
            Arc::new(DryRunNetworkBackend),
            Arc::new(DryRunSupervisor),
        )
        .await
        .unwrap();

        let mut next = (*engine.active_config()).clone();
        next.system.hostname = "edge-gw".to_string();
        let status = engine.begin_commit(next).await.unwrap();
        assert!(status.pending.is_some());

        let confirmed = engine.confirm_commit().await.unwrap();
        assert!(confirmed.pending.is_none());
        assert_eq!(engine.list_history().unwrap().len(), 1);

        let reloaded = store.load_or_init().unwrap();
        assert_eq!(reloaded.system.hostname, "edge-gw");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rollback_restores_previous_config() {
        let dir = std::env::temp_dir().join(format!("crabrouter-rb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ConfigStore::new(dir.join("router.toml"));
        let engine = RouterEngine::bootstrap(
            store,
            Arc::new(DryRunNetworkBackend),
            Arc::new(DryRunSupervisor),
        )
        .await
        .unwrap();

        let mut next = (*engine.active_config()).clone();
        next.system.hostname = "broken-gw".to_string();
        engine.begin_commit(next).await.unwrap();
        assert_eq!(engine.active_config().system.hostname, "broken-gw");

        engine.rollback_pending().await.unwrap();
        assert_eq!(engine.active_config().system.hostname, "crabrouter");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
