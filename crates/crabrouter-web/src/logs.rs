use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use ts_rs::TS;

const MAX_LOG_HISTORY: usize = 250;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogEvent {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Clone)]
pub struct LogHub {
    history: Arc<Mutex<VecDeque<LogEvent>>>,
    tx: broadcast::Sender<LogEvent>,
}

impl Default for LogHub {
    fn default() -> Self {
        Self::new()
    }
}

impl LogHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            history: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_HISTORY))),
            tx,
        }
    }

    pub fn push(
        &self,
        level: impl Into<String>,
        target: impl Into<String>,
        message: impl Into<String>,
    ) {
        let event = LogEvent {
            timestamp: Utc::now().to_rfc3339(),
            level: level.into(),
            target: target.into(),
            message: message.into(),
        };
        if let Ok(mut guard) = self.history.lock() {
            if guard.len() >= MAX_LOG_HISTORY {
                guard.pop_front();
            }
            guard.push_back(event.clone());
        }
        let _ = self.tx.send(event);
    }

    pub fn snapshot(&self) -> Vec<LogEvent> {
        self.history
            .lock()
            .map_or_default(|g| g.iter().cloned().collect())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogEvent> {
        self.tx.subscribe()
    }
}
