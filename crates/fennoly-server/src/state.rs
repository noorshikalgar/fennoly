//! Shared application state.

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::storage::Storage;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub struct Inner {
    pub db: PgPool,
    pub cfg: Config,
    pub storage: Storage,
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
const LOGIN_LOCKOUT: Duration = Duration::from_secs(60);

impl AppState {
    pub fn new(db: PgPool, cfg: Config, storage: Storage) -> Self {
        AppState(Arc::new(Inner {
            db,
            cfg,
            storage,
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

    pub fn drop_upload_lock(&self, id: Uuid) {
        self.upload_locks.lock().expect("lock poisoned").remove(&id);
    }

    /// True when this username has failed too often in the last minute.
    pub fn login_locked(&self, username: &str) -> bool {
        let map = self.login_failures.lock().expect("lock poisoned");
        map.get(username)
            .is_some_and(|(n, at)| *n >= MAX_LOGIN_FAILURES && at.elapsed() < LOGIN_LOCKOUT)
    }

    pub fn record_login_failure(&self, username: &str) {
        let mut map = self.login_failures.lock().expect("lock poisoned");
        let entry = map.entry(username.to_string()).or_insert((0, Instant::now()));
        if entry.1.elapsed() >= LOGIN_LOCKOUT {
            *entry = (0, Instant::now());
        }
        entry.0 += 1;
        entry.1 = Instant::now();
    }

    pub fn clear_login_failures(&self, username: &str) {
        self.login_failures.lock().expect("lock poisoned").remove(username);
    }
}
