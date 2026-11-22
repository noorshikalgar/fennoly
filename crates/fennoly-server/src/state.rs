//! Shared application state.

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sqlx::PgPool;
use uuid::Uuid;

use crate::backup::engine::Engine;
use crate::config::Config;
use crate::secrets::Secrets;
use crate::storage::Storage;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub struct Inner {
    pub db: PgPool,
    pub cfg: Config,
    pub storage: Storage,
    pub secrets: Secrets,
    pub engine: Engine,
    /// Backup destinations with a run in progress, and that run's progress (0–1).
    backups_running: Mutex<HashMap<Uuid, f64>>,
    upload_locks: Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
    login_failures: Mutex<HashMap<String, (u32, Instant)>>,
}

impl Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

const MAX_LOGIN_FAILURES: u32 = 5;
/// Failures are forgotten after an hour without a new one.
const FAILURE_MEMORY: Duration = Duration::from_secs(3600);
/// Bound on remembered names, so floods of made-up names cannot eat memory.
const MAX_TRACKED_NAMES: usize = 10_000;

/// How long a name stays locked after `n` failures: 1 minute after the 5th,
/// then doubling with every further failure, up to 16 minutes.
pub fn lockout_for(n: u32) -> Duration {
    if n < MAX_LOGIN_FAILURES {
        return Duration::ZERO;
    }
    Duration::from_secs(60 << (n - MAX_LOGIN_FAILURES).min(4))
}

impl AppState {
    pub fn new(db: PgPool, cfg: Config, storage: Storage, secrets: Secrets) -> Self {
        let engine = Engine::detect(storage.restic_cache());
        AppState(Arc::new(Inner {
            db,
            cfg,
            storage,
            secrets,
            engine,
            backups_running: Mutex::new(HashMap::new()),
            upload_locks: Mutex::new(HashMap::new()),
            login_failures: Mutex::new(HashMap::new()),
        }))
    }
}

impl Inner {
    /// One lock per upload, so two PATCH requests for the same upload
    /// never write at the same time.
    pub fn upload_lock(&self, id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self.upload_locks.lock().expect("lock poisoned");
        map.entry(id).or_default().clone()
    }

    /// Marks a destination busy. Returns false if a run is already going.
    pub fn claim_backup(&self, target: Uuid) -> bool {
        let mut map = self.backups_running.lock().expect("lock poisoned");
        if map.contains_key(&target) {
            return false;
        }
        map.insert(target, 0.0);
        true
    }

    pub fn backup_progress(&self, target: Uuid, fraction: f64) {
        if let Some(p) = self.backups_running.lock().expect("lock poisoned").get_mut(&target) {
            *p = fraction.clamp(0.0, 1.0);
        }
    }

    pub fn release_backup(&self, target: Uuid) {
        self.backups_running.lock().expect("lock poisoned").remove(&target);
    }

    /// Progress of a running backup, if one is running.
    pub fn backup_running(&self, target: Uuid) -> Option<f64> {
        self.backups_running.lock().expect("lock poisoned").get(&target).copied()
    }

    pub fn drop_upload_lock(&self, id: Uuid) {
        self.upload_locks.lock().expect("lock poisoned").remove(&id);
    }

    /// True when this username has failed too often in the last minute.
    pub fn login_locked(&self, username: &str) -> bool {
        let map = self.login_failures.lock().expect("lock poisoned");
        map.get(username).is_some_and(|(n, at)| at.elapsed() < lockout_for(*n))
    }

    pub fn record_login_failure(&self, username: &str) {
        let mut map = self.login_failures.lock().expect("lock poisoned");
        if map.len() >= MAX_TRACKED_NAMES && !map.contains_key(username) {
            map.retain(|_, (_, at)| at.elapsed() < FAILURE_MEMORY);
            if map.len() >= MAX_TRACKED_NAMES {
                // Still full of fresh entries: keep only names that are locked.
                map.retain(|_, (n, _)| *n >= MAX_LOGIN_FAILURES);
            }
        }
        let entry = map.entry(username.to_string()).or_insert((0, Instant::now()));
        if entry.1.elapsed() >= FAILURE_MEMORY {
            *entry = (0, Instant::now());
        }
        entry.0 += 1;
        entry.1 = Instant::now();
    }

    pub fn clear_login_failures(&self, username: &str) {
        self.login_failures.lock().expect("lock poisoned").remove(username);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockouts_grow_and_stop_growing() {
        assert_eq!(lockout_for(4), Duration::ZERO);
        assert_eq!(lockout_for(5), Duration::from_secs(60));
        assert_eq!(lockout_for(6), Duration::from_secs(120));
        assert_eq!(lockout_for(9), Duration::from_secs(960));
        assert_eq!(lockout_for(50), Duration::from_secs(960));
    }
}
