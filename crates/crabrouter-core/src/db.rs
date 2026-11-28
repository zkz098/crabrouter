use std::path::Path;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use crabrouter_model::WanRuntimeState;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{CoreError, Result};

const ADMIN_TABLE: TableDefinition<&str, &str> = TableDefinition::new("admin");
const PENDING_TABLE: TableDefinition<&str, &str> = TableDefinition::new("pending_commit");
const HISTORY_TABLE: TableDefinition<u64, &str> = TableDefinition::new("config_history");
const WAN_STATE_TABLE: TableDefinition<&str, &str> = TableDefinition::new("wan_states");
const SESSIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("sessions");
const TRAFFIC_DAILY_TABLE: TableDefinition<&str, &str> = TableDefinition::new("traffic_daily");

const ADMIN_KEY: &str = "primary";
const PENDING_KEY: &str = "active";
const MAX_HISTORY_SNAPSHOTS: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdminRecord {
    username: String,
    password_hash: String,
    initialized_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionRecord {
    username: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMarker {
    pub generation: u64,
    pub expires_at: DateTime<Utc>,
    pub previous_toml: String,
    pub candidate_toml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub generation: u64,
    pub created_at: DateTime<Utc>,
    pub summary: String,
    pub config_toml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DailyTrafficRecord {
    pub date: String,
    pub interface: String,
    #[ts(type = "number")]
    pub rx_bytes: u64,
    #[ts(type = "number")]
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigSnapshotSummary {
    #[ts(type = "number")]
    pub generation: u64,
    pub created_at: DateTime<Utc>,
    pub summary: String,
}

/// Pure-Rust embedded ACID state store backed by `redb`.
pub struct StateDb {
    db: Database,
}

impl StateDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let db = Database::create(path).map_err(|e| CoreError::Db(e.to_string()))?;
        let state = Self { db };
        state.init_tables()?;
        Ok(state)
    }

    fn init_tables(&self) -> Result<()> {
        let txn = self
            .db
            .begin_write()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        {
            let _ = txn
                .open_table(ADMIN_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            let _ = txn
                .open_table(PENDING_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            let _ = txn
                .open_table(HISTORY_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            let _ = txn
                .open_table(WAN_STATE_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            let _ = txn
                .open_table(SESSIONS_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            let _ = txn
                .open_table(TRAFFIC_DAILY_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
        }
        txn.commit().map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn is_initialized(&self) -> Result<bool> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let table = txn
            .open_table(ADMIN_TABLE)
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let exists = table
            .get(ADMIN_KEY)
            .map_err(|e| CoreError::Db(e.to_string()))?
            .is_some();
        Ok(exists)
    }

    pub fn set_admin_credentials(&self, username: &str, password: &str) -> Result<()> {
        if username.trim().is_empty() || password.len() < 6 {
            return Err(CoreError::Auth(
                "username cannot be empty and password must be at least 6 characters".to_string(),
            ));
        }
        let mut salt_bytes = [0u8; 16];
        getrandom::fill(&mut salt_bytes)
            .map_err(|e| CoreError::Auth(format!("salt rng failed: {e}")))?;
        let salt = SaltString::encode_b64(&salt_bytes)
            .map_err(|e| CoreError::Auth(format!("salt encode failed: {e}")))?;
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| CoreError::Auth(format!("password hash error: {e}")))?
            .to_string();

        let rec = AdminRecord {
            username: username.trim().to_string(),
            password_hash: hash,
            initialized_at: Utc::now(),
        };
        let encoded = serde_json::to_string(&rec).map_err(|e| CoreError::Db(e.to_string()))?;
        let txn = self
            .db
            .begin_write()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        {
            let mut table = txn
                .open_table(ADMIN_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            table
                .insert(ADMIN_KEY, encoded.as_str())
                .map_err(|e| CoreError::Db(e.to_string()))?;
        }
        txn.commit().map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn verify_login_and_issue_token(&self, username: &str, password: &str) -> Result<String> {
        let rec: AdminRecord = {
            let txn = self
                .db
                .begin_read()
                .map_err(|e| CoreError::Db(e.to_string()))?;
            let table = txn
                .open_table(ADMIN_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            let raw = table
                .get(ADMIN_KEY)
                .map_err(|e| CoreError::Db(e.to_string()))?
                .ok_or_else(|| {
                    CoreError::Auth("router has not been initialized yet".to_string())
                })?;
            serde_json::from_str(raw.value()).map_err(|e| CoreError::Db(e.to_string()))?
        };

        if rec.username != username.trim() {
            return Err(CoreError::Auth("invalid username or password".to_string()));
        }
        let parsed = PasswordHash::new(&rec.password_hash)
            .map_err(|e| CoreError::Auth(format!("stored password hash invalid: {e}")))?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| CoreError::Auth("invalid username or password".to_string()))?;

        self.issue_session_token(&rec.username)
    }

    pub fn issue_session_token(&self, username: &str) -> Result<String> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| CoreError::Auth(format!("rng failed: {e}")))?;
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let session = SessionRecord {
            username: username.to_string(),
            expires_at: Utc::now() + Duration::days(7),
        };
        let encoded = serde_json::to_string(&session).map_err(|e| CoreError::Db(e.to_string()))?;

        let txn = self
            .db
            .begin_write()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        {
            let mut table = txn
                .open_table(SESSIONS_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            table
                .insert(token.as_str(), encoded.as_str())
                .map_err(|e| CoreError::Db(e.to_string()))?;
        }
        txn.commit().map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(token)
    }

    pub fn verify_session_token(&self, token: &str) -> Result<bool> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let table = txn
            .open_table(SESSIONS_TABLE)
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let Some(raw) = table.get(token).map_err(|e| CoreError::Db(e.to_string()))? else {
            return Ok(false);
        };
        let rec: SessionRecord =
            serde_json::from_str(raw.value()).map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(rec.expires_at > Utc::now())
    }

    pub fn revoke_session_token(&self, token: &str) -> Result<()> {
        let txn = self
            .db
            .begin_write()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        {
            let mut table = txn
                .open_table(SESSIONS_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            let _ = table
                .remove(token)
                .map_err(|e| CoreError::Db(e.to_string()))?;
        }
        txn.commit().map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn write_pending_marker(&self, marker: &PendingMarker) -> Result<()> {
        let encoded = serde_json::to_string(marker).map_err(|e| CoreError::Db(e.to_string()))?;
        let txn = self
            .db
            .begin_write()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        {
            let mut table = txn
                .open_table(PENDING_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            table
                .insert(PENDING_KEY, encoded.as_str())
                .map_err(|e| CoreError::Db(e.to_string()))?;
        }
        txn.commit().map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn take_pending_marker(&self) -> Result<Option<PendingMarker>> {
        let txn = self
            .db
            .begin_write()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let found = {
            let mut table = txn
                .open_table(PENDING_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            let removed = table
                .remove(PENDING_KEY)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            removed
                .map(|guard| serde_json::from_str::<PendingMarker>(guard.value()))
                .transpose()
                .map_err(|e| CoreError::Db(e.to_string()))?
        };
        txn.commit().map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(found)
    }

    pub fn clear_pending_marker(&self) -> Result<()> {
        let _ = self.take_pending_marker()?;
        Ok(())
    }

    pub fn save_history_snapshot(&self, snapshot: &ConfigSnapshot) -> Result<()> {
        let encoded = serde_json::to_string(snapshot).map_err(|e| CoreError::Db(e.to_string()))?;
        let txn = self
            .db
            .begin_write()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        {
            let mut table = txn
                .open_table(HISTORY_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            table
                .insert(snapshot.generation, encoded.as_str())
                .map_err(|e| CoreError::Db(e.to_string()))?;

            let mut keys: Vec<u64> = Vec::new();
            for item in table.iter().map_err(|e| CoreError::Db(e.to_string()))? {
                let (k, _) = item.map_err(|e| CoreError::Db(e.to_string()))?;
                keys.push(k.value());
            }
            if keys.len() > MAX_HISTORY_SNAPSHOTS {
                let to_drop = keys.len() - MAX_HISTORY_SNAPSHOTS;
                for old_key in keys.into_iter().take(to_drop) {
                    let _ = table
                        .remove(old_key)
                        .map_err(|e| CoreError::Db(e.to_string()))?;
                }
            }
        }
        txn.commit().map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn list_history_summaries(&self) -> Result<Vec<ConfigSnapshotSummary>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let table = txn
            .open_table(HISTORY_TABLE)
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for item in table.iter().map_err(|e| CoreError::Db(e.to_string()))? {
            let (_, val) = item.map_err(|e| CoreError::Db(e.to_string()))?;
            let snap: ConfigSnapshot =
                serde_json::from_str(val.value()).map_err(|e| CoreError::Db(e.to_string()))?;
            out.push(ConfigSnapshotSummary {
                generation: snap.generation,
                created_at: snap.created_at,
                summary: snap.summary,
            });
        }
        out.sort_by_key(|a| std::cmp::Reverse(a.generation));
        Ok(out)
    }

    pub fn get_history_snapshot(&self, generation: u64) -> Result<Option<ConfigSnapshot>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let table = txn
            .open_table(HISTORY_TABLE)
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let Some(raw) = table
            .get(generation)
            .map_err(|e| CoreError::Db(e.to_string()))?
        else {
            return Ok(None);
        };
        let snap: ConfigSnapshot =
            serde_json::from_str(raw.value()).map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(Some(snap))
    }

    pub fn latest_generation(&self) -> Result<u64> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let table = txn
            .open_table(HISTORY_TABLE)
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let mut max_gen = 1_u64;
        for item in table.iter().map_err(|e| CoreError::Db(e.to_string()))? {
            let (k, _) = item.map_err(|e| CoreError::Db(e.to_string()))?;
            max_gen = max_gen.max(k.value());
        }
        Ok(max_gen)
    }

    pub fn upsert_wan_state(&self, state: &WanRuntimeState) -> Result<()> {
        let encoded = serde_json::to_string(state).map_err(|e| CoreError::Db(e.to_string()))?;
        let txn = self
            .db
            .begin_write()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        {
            let mut table = txn
                .open_table(WAN_STATE_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            table
                .insert(state.wan_name.as_str(), encoded.as_str())
                .map_err(|e| CoreError::Db(e.to_string()))?;
        }
        txn.commit().map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn list_wan_states(&self) -> Result<Vec<WanRuntimeState>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let table = txn
            .open_table(WAN_STATE_TABLE)
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for item in table.iter().map_err(|e| CoreError::Db(e.to_string()))? {
            let (_, val) = item.map_err(|e| CoreError::Db(e.to_string()))?;
            let state: WanRuntimeState =
                serde_json::from_str(val.value()).map_err(|e| CoreError::Db(e.to_string()))?;
            out.push(state);
        }
        out.sort_by(|a, b| a.wan_name.cmp(&b.wan_name));
        Ok(out)
    }

    pub fn record_today_traffic_deltas(&self, deltas: &[(String, u64, u64)]) -> Result<()> {
        let date = Utc::now().format("%Y-%m-%d").to_string();
        self.record_traffic_deltas(&date, deltas)
    }

    pub fn record_traffic_deltas(&self, date: &str, deltas: &[(String, u64, u64)]) -> Result<()> {
        if deltas.is_empty() {
            return Ok(());
        }
        let txn = self
            .db
            .begin_write()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        {
            let mut table = txn
                .open_table(TRAFFIC_DAILY_TABLE)
                .map_err(|e| CoreError::Db(e.to_string()))?;
            for (iface, rx_delta, tx_delta) in deltas {
                if *rx_delta == 0 && *tx_delta == 0 {
                    continue;
                }
                let key = format!("{date}:{iface}");
                let mut rec = match table
                    .get(key.as_str())
                    .map_err(|e| CoreError::Db(e.to_string()))?
                {
                    Some(raw) => serde_json::from_str::<DailyTrafficRecord>(raw.value()).unwrap_or(
                        DailyTrafficRecord {
                            date: date.to_string(),
                            interface: iface.clone(),
                            rx_bytes: 0,
                            tx_bytes: 0,
                        },
                    ),
                    None => DailyTrafficRecord {
                        date: date.to_string(),
                        interface: iface.clone(),
                        rx_bytes: 0,
                        tx_bytes: 0,
                    },
                };
                rec.rx_bytes = rec.rx_bytes.saturating_add(*rx_delta);
                rec.tx_bytes = rec.tx_bytes.saturating_add(*tx_delta);
                let encoded =
                    serde_json::to_string(&rec).map_err(|e| CoreError::Db(e.to_string()))?;
                table
                    .insert(key.as_str(), encoded.as_str())
                    .map_err(|e| CoreError::Db(e.to_string()))?;
            }
        }
        txn.commit().map_err(|e| CoreError::Db(e.to_string()))?;
        Ok(())
    }

    pub fn list_daily_traffic(&self, max_records: usize) -> Result<Vec<DailyTrafficRecord>> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let table = txn
            .open_table(TRAFFIC_DAILY_TABLE)
            .map_err(|e| CoreError::Db(e.to_string()))?;
        let mut out = Vec::new();
        for item in table.iter().map_err(|e| CoreError::Db(e.to_string()))? {
            let (_, val) = item.map_err(|e| CoreError::Db(e.to_string()))?;
            let rec: DailyTrafficRecord =
                serde_json::from_str(val.value()).map_err(|e| CoreError::Db(e.to_string()))?;
            out.push(rec);
        }
        out.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then_with(|| a.interface.cmp(&b.interface))
        });
        out.truncate(max_records);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redb_auth_pending_and_history_roundtrip() {
        let dir = std::env::temp_dir().join(format!("crabrouter-redb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = StateDb::open(&dir.join("state.redb")).unwrap();

        assert!(!db.is_initialized().unwrap());
        db.set_admin_credentials("admin", "hunter22").unwrap();
        assert!(db.is_initialized().unwrap());

        let token = db
            .verify_login_and_issue_token("admin", "hunter22")
            .unwrap();
        assert!(db.verify_session_token(&token).unwrap());
        assert!(db.verify_login_and_issue_token("admin", "wrong").is_err());

        db.save_history_snapshot(&ConfigSnapshot {
            generation: 2,
            created_at: Utc::now(),
            summary: "hostname=gw".to_string(),
            config_toml: "schema_version = 1".to_string(),
        })
        .unwrap();
        assert_eq!(db.list_history_summaries().unwrap().len(), 1);
        assert_eq!(db.latest_generation().unwrap(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
