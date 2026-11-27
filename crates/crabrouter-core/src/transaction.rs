use std::sync::{Arc, OnceLock};
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use crabrouter_model::{RouterConfig, WanRuntimeState};
use crabrouter_net::{EbpfEngine, NetworkBackend, WanStateObserver};
use crabrouter_services::{ServiceSupervisor, SystemdUnitStatus};
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
    ebpf: OnceLock<EbpfEngine>,
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
            ebpf: OnceLock::new(),
            tx: Mutex::new(TxState {
                generation,
                pending: None,
            }),
        });
        engine.apply_runtime(&initial).await?;
        Ok(engine)
    }

    pub fn attach_ebpf(&self, ebpf: EbpfEngine) {
        let _ = self.ebpf.set(ebpf);
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

    pub async fn service_statuses(&self) -> Result<Vec<SystemdUnitStatus>> {
        let cfg = self.active.load_full();
        Ok(self.services.query_unit_statuses(&cfg).await?)
    }

    pub async fn redial_pppoe(&self, wan_name: &str) -> Result<bool> {
        Ok(self.net.redial_pppoe(wan_name).await?)
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
            tracing::warn!(
                generation,
                error = %err,
                "apply_runtime failed during begin_commit; executing compensating rollback"
            );
            if let Err(rb_err) = self.apply_runtime(&previous).await {
                tracing::error!(
                    generation,
                    error = %rb_err,
                    "compensating rollback after failed apply_runtime also failed"
                );
            }
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
        if let Some(ebpf) = self.ebpf.get()
            && let Err(err) = ebpf.sync_config(cfg).await
        {
            tracing::warn!(error = %err, "eBPF tc/tcx sync failed during apply_runtime");
        }
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

    #[tokio::test]
    async fn begin_commit_partial_failure_compensates_previous_runtime() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct RecordingNet {
            applied_hostnames: std::sync::Mutex<Vec<String>>,
        }

        impl NetworkBackend for RecordingNet {
            fn apply_network_and_firewall<'a>(
                &'a self,
                cfg: &'a RouterConfig,
            ) -> crabrouter_net::netlink::BoxFuture<'a, crabrouter_net::Result<()>> {
                Box::pin(async move {
                    self.applied_hostnames
                        .lock()
                        .unwrap()
                        .push(cfg.system.hostname.clone());
                    Ok(())
                })
            }
        }

        struct FailSecondSyncSupervisor {
            fail_next: AtomicBool,
        }

        impl ServiceSupervisor for FailSecondSyncSupervisor {
            fn sync_services<'a>(
                &'a self,
                _cfg: &'a RouterConfig,
            ) -> crabrouter_services::supervisor::BoxFuture<'a, crabrouter_services::Result<()>>
            {
                Box::pin(async move {
                    if self.fail_next.load(Ordering::SeqCst) {
                        return Err(crabrouter_services::ServiceError::Render(
                            "simulated sync failure".to_string(),
                        ));
                    }
                    Ok(())
                })
            }

            fn query_unit_statuses<'a>(
                &'a self,
                _cfg: &'a RouterConfig,
            ) -> crabrouter_services::supervisor::BoxFuture<
                'a,
                crabrouter_services::Result<Vec<SystemdUnitStatus>>,
            > {
                Box::pin(async move { Ok(Vec::new()) })
            }
        }

        let dir = std::env::temp_dir().join(format!("crabrouter-comp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ConfigStore::new(dir.join("router.toml"));
        let net = Arc::new(RecordingNet {
            applied_hostnames: std::sync::Mutex::new(Vec::new()),
        });
        let svc = Arc::new(FailSecondSyncSupervisor {
            fail_next: AtomicBool::new(false),
        });

        let engine = RouterEngine::bootstrap(store, net.clone(), svc.clone())
            .await
            .unwrap();
        svc.fail_next.store(true, Ordering::SeqCst);

        let mut candidate_cfg = (*engine.active_config()).clone();
        candidate_cfg.system.hostname = "broken-gw".to_string();
        let err = engine.begin_commit(candidate_cfg).await.unwrap_err();
        assert!(err.to_string().contains("simulated sync failure"));

        // Allow compensating rollback to succeed on services as well; verify net sequence:
        // bootstrap("crabrouter") -> begin_commit("broken-gw") -> compensating("crabrouter")
        let recorded = net.applied_hostnames.lock().unwrap().clone();
        assert_eq!(recorded, vec!["crabrouter", "broken-gw", "crabrouter"]);
        assert_eq!(engine.active_config().system.hostname, "crabrouter");
        assert!(engine.status().await.pending.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn commit_confirm_timeout_auto_rolls_back() {
        let dir = std::env::temp_dir().join(format!("crabrouter-vclk-rb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ConfigStore::new(dir.join("router.toml"));
        let engine = RouterEngine::bootstrap(
            store.clone(),
            Arc::new(DryRunNetworkBackend),
            Arc::new(DryRunSupervisor),
        )
        .await
        .unwrap();

        let mut candidate_cfg = (*engine.active_config()).clone();
        candidate_cfg.system.hostname = "temp-gw".to_string();
        candidate_cfg.system.commit_confirm_timeout_secs = 60;
        let status = engine.begin_commit(candidate_cfg).await.unwrap();
        assert!(status.pending.is_some());
        assert_eq!(engine.active_config().system.hostname, "temp-gw");
        // Let the spawned rollback_task register its sleep timer at t=0.
        tokio::task::yield_now().await;

        // Halfway through the 60s window: commit is still pending.
        tokio::time::advance(Duration::from_secs(30)).await;
        tokio::task::yield_now().await;
        assert!(engine.status().await.pending.is_some());
        assert_eq!(engine.active_config().system.hostname, "temp-gw");

        // Past the 60s deadline: auto-rollback task fires and restores previous config.
        tokio::time::advance(Duration::from_secs(31)).await;
        for _ in 0..20 {
            tokio::task::yield_now().await;
            if engine.status().await.pending.is_none() {
                break;
            }
        }
        assert!(engine.status().await.pending.is_none());
        assert_eq!(engine.active_config().system.hostname, "crabrouter");
        assert!(engine.db().take_pending_marker().unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn confirm_right_before_timeout_cancels_rollback() {
        let dir = std::env::temp_dir().join(format!("crabrouter-vclk-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ConfigStore::new(dir.join("router.toml"));
        let engine = RouterEngine::bootstrap(
            store.clone(),
            Arc::new(DryRunNetworkBackend),
            Arc::new(DryRunSupervisor),
        )
        .await
        .unwrap();

        let mut candidate_cfg = (*engine.active_config()).clone();
        candidate_cfg.system.hostname = "confirmed-gw".to_string();
        candidate_cfg.system.commit_confirm_timeout_secs = 60;
        engine.begin_commit(candidate_cfg).await.unwrap();

        tokio::time::advance(Duration::from_secs(59)).await;
        tokio::task::yield_now().await;
        let confirmed = engine.confirm_commit().await.unwrap();
        assert!(confirmed.pending.is_none());

        // Advance well past original 60s deadline; config must remain confirmed.
        tokio::time::advance(Duration::from_secs(30)).await;
        tokio::task::yield_now().await;
        assert_eq!(engine.active_config().system.hostname, "confirmed-gw");
        assert_eq!(
            store.load_or_init().unwrap().system.hostname,
            "confirmed-gw"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_begin_commit_rejected_until_timeout() {
        let dir = std::env::temp_dir().join(format!("crabrouter-vclk-dup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = ConfigStore::new(dir.join("router.toml"));
        let engine = RouterEngine::bootstrap(
            store,
            Arc::new(DryRunNetworkBackend),
            Arc::new(DryRunSupervisor),
        )
        .await
        .unwrap();

        let mut cfg1 = (*engine.active_config()).clone();
        cfg1.system.hostname = "gw-1".to_string();
        cfg1.system.commit_confirm_timeout_secs = 15;
        engine.begin_commit(cfg1).await.unwrap();
        // Let the spawned rollback_task register its sleep timer at t=0.
        tokio::task::yield_now().await;

        let mut cfg2 = (*engine.active_config()).clone();
        cfg2.system.hostname = "gw-2".to_string();
        assert!(engine.begin_commit(cfg2.clone()).await.is_err());

        // Once timeout expires and auto-rollback completes, a new commit is accepted.
        tokio::time::advance(Duration::from_secs(16)).await;
        for _ in 0..20 {
            tokio::task::yield_now().await;
            if engine.status().await.pending.is_none() {
                break;
            }
        }
        assert!(engine.status().await.pending.is_none());
        let st = engine.begin_commit(cfg2).await.unwrap();
        assert!(st.pending.is_some());
        assert_eq!(engine.active_config().system.hostname, "gw-2");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
