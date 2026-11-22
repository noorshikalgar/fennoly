//! Admin: backup destinations, runs, snapshots and restore.

use std::path::Path as FsPath;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::types::Json as SqlJson;
use uuid::Uuid;

use crate::auth::{self, AdminUser};
use crate::backup::engine::{self, Kind, SecretSettings, Settings, Snapshot};
use crate::backup::schedule::Schedule;
use crate::backup::{self, RUN_COLS, RunRow, TARGET_COLS, TargetRow};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// How long quick engine calls (test, list snapshots, browse) may take.
const QUICK: Duration = Duration::from_secs(120);

async fn quick<T>(fut: impl std::future::Future<Output = AppResult<T>>) -> AppResult<T> {
    tokio::time::timeout(QUICK, fut).await.unwrap_or_else(|_| {
        Err(AppError::BadRequest(
            "The backup destination did not answer in time. Check that it is switched on and connected.".into(),
        ))
    })
}

fn require_engine(state: &AppState, kind: Kind) -> AppResult<()> {
    if state.engine.restic.is_none() {
        return Err(AppError::Unavailable("the backup engine (restic) is not installed on the server".into()));
    }
    if kind.needs_rclone() && state.engine.rclone.is_none() {
        return Err(AppError::Unavailable("rclone is needed for network shares but is not installed on the server".into()));
    }
    Ok(())
}

// ---- listing ---------------------------------------------------------------

#[derive(Serialize)]
pub struct TargetDto {
    id: Uuid,
    name: String,
    kind: String,
    settings: Settings,
    /// Which secrets are stored (never their values).
    has_password: bool,
    has_keys: bool,
    encrypted: bool,
    schedule: String,
    keep_daily: i32,
    keep_weekly: i32,
    keep_monthly: i32,
    include_trash: bool,
    enabled: bool,
    initialized: bool,
    next_run_at: Option<DateTime<Utc>>,
    location: String,
    /// Progress (0–1) of the run in progress, if any.
    running: Option<f64>,
    last_run: Option<RunRow>,
    last_success_at: Option<DateTime<Utc>>,
}

async fn to_dto(state: &AppState, t: TargetRow) -> AppResult<TargetDto> {
    let secrets: SecretSettings = match &t.secrets {
        Some(sealed) => serde_json::from_str(&state.secrets.open(sealed)?).unwrap_or_default(),
        None => SecretSettings::default(),
    };
    let last_run: Option<RunRow> = sqlx::query_as(&format!(
        "SELECT {RUN_COLS} FROM backup_runs WHERE target_id = $1 AND kind = 'backup' ORDER BY started_at DESC LIMIT 1"
    ))
    .bind(t.id)
    .fetch_optional(&state.db)
    .await?;
    let last_success_at: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT max(finished_at) FROM backup_runs WHERE target_id = $1 AND kind = 'backup' AND status = 'success'",
    )
    .bind(t.id)
    .fetch_one(&state.db)
    .await?;
    let kind = Kind::parse(&t.kind)?;
    Ok(TargetDto {
        id: t.id,
        location: engine::describe(kind, &t.settings.0),
        name: t.name,
        kind: t.kind,
        settings: t.settings.0,
        has_password: secrets.password.is_some(),
        has_keys: secrets.access_key.is_some() && secrets.secret_key.is_some(),
        encrypted: t.encrypted,
        schedule: t.schedule,
        keep_daily: t.keep_daily,
        keep_weekly: t.keep_weekly,
        keep_monthly: t.keep_monthly,
        include_trash: t.include_trash,
        enabled: t.enabled,
        initialized: t.initialized,
        next_run_at: if t.enabled { t.next_run_at } else { None },
        running: state.backup_running(t.id),
        last_run,
        last_success_at,
    })
}

pub async fn list(State(state): State<AppState>, _admin: AdminUser) -> AppResult<Json<Value>> {
    let rows: Vec<TargetRow> = sqlx::query_as(&format!("SELECT {TARGET_COLS} FROM backup_targets ORDER BY created_at"))
        .fetch_all(&state.db)
        .await?;
    let mut targets = Vec::with_capacity(rows.len());
    for row in rows {
        targets.push(to_dto(&state, row).await?);
    }
    Ok(Json(json!({
        "engine": {
            "restic": state.engine.restic.is_some(),
            "rclone": state.engine.rclone.is_some(),
        },
        "targets": targets,
    })))
}

pub async fn get_one(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<Uuid>) -> AppResult<Json<TargetDto>> {
    let t = backup::load_target(&state, id).await?;
    Ok(Json(to_dto(&state, t).await?))
}

// ---- create / update / delete ------------------------------------------------

fn clean_name(raw: &str) -> AppResult<String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(AppError::bad("give the destination a name of up to 80 characters"));
    }
    Ok(name.to_string())
}

fn check_keep(d: i32, w: i32, m: i32) -> AppResult<()> {
    if !(0..=365).contains(&d) || !(0..=520).contains(&w) || !(0..=240).contains(&m) {
        return Err(AppError::bad("keep at most 365 daily, 520 weekly and 240 monthly backups"));
    }
    if d + w + m == 0 {
        return Err(AppError::bad("keep at least one backup"));
    }
    Ok(())
}

fn same_or_inside(child: &FsPath, parent: &FsPath) -> bool {
    let norm = |p: &FsPath| {
        let s = backup::plain(p).replace('\\', "/");
        let s = s.trim_end_matches('/').to_string();
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    let (c, p) = (norm(child), norm(parent));
    c == p || c.starts_with(&format!("{p}/"))
}

/// A backup folder inside Fennoly's data (or containing it) would back up
/// into itself, or mix the backup with live files.
fn check_local_path(state: &AppState, kind: Kind, s: &Settings) -> AppResult<()> {
    if kind != Kind::Local {
        return Ok(());
    }
    let raw = s.path.as_deref().unwrap_or_default().trim();
    let path = std::path::absolute(raw).map_err(|_| AppError::bad("invalid folder path"))?;
    let data = state.storage.spaces_root();
    let data = data.parent().unwrap_or(&data);
    if same_or_inside(&path, data) || same_or_inside(data, &path) {
        return Err(AppError::bad(
            "choose a folder outside Fennoly's data folder, ideally on another drive",
        ));
    }
    Ok(())
}

/// A recovery key: 160 random bits written as 8 groups of 4 characters
/// (Crockford base32: no I, L, O or U, so it is easy to copy by hand).
fn new_recovery_key() -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let bytes: [u8; 20] = rand::random();
    let mut bits: u64 = 0;
    let mut n = 0;
    let mut chars = String::with_capacity(39);
    for b in bytes {
        bits = (bits << 8) | u64::from(b);
        n += 8;
        while n >= 5 {
            n -= 5;
            chars.push(ALPHABET[((bits >> n) & 31) as usize] as char);
        }
    }
    chars
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).expect("ascii"))
        .collect::<Vec<_>>()
        .join("-")
}

fn seal_secrets(state: &AppState, s: &SecretSettings) -> AppResult<Option<String>> {
    let empty = s.password.is_none() && s.access_key.is_none() && s.secret_key.is_none();
    if empty {
        return Ok(None);
    }
    let json = serde_json::to_string(s).map_err(|e| AppError::Internal(e.into()))?;
    Ok(Some(state.secrets.seal(&json)))
}

/// Drops blank strings so `""` means "not set".
fn tidy(mut s: SecretSettings) -> SecretSettings {
    let keep = |v: Option<String>| v.filter(|x| !x.trim().is_empty());
    s.password = keep(s.password);
    s.access_key = keep(s.access_key).map(|v| v.trim().to_string());
    s.secret_key = keep(s.secret_key).map(|v| v.trim().to_string());
    s
}

fn tidy_settings(mut s: Settings) -> Settings {
    let keep = |v: Option<String>| v.map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    s.path = keep(s.path);
    s.host = keep(s.host);
    s.share = keep(s.share);
    s.user = keep(s.user);
    s.domain = keep(s.domain);
    s.endpoint = keep(s.endpoint);
    s.bucket = keep(s.bucket);
    s.region = keep(s.region);
    s
}

#[derive(Deserialize)]
pub struct CreateTarget {
    name: String,
    kind: String,
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    secrets: SecretSettings,
    #[serde(default = "yes")]
    encrypted: bool,
    /// To connect to an existing encrypted backup (for example after
    /// reinstalling), give its recovery key instead of making a new one.
    #[serde(default)]
    recovery_key: Option<String>,
    #[serde(default = "default_schedule")]
    schedule: String,
    #[serde(default = "seven")]
    keep_daily: i32,
    #[serde(default = "four")]
    keep_weekly: i32,
    #[serde(default = "twelve")]
    keep_monthly: i32,
    #[serde(default)]
    include_trash: bool,
    #[serde(default = "yes")]
    enabled: bool,
}

fn yes() -> bool {
    true
}
fn default_schedule() -> String {
    "daily 02:00".into()
}
fn seven() -> i32 {
    7
}
fn four() -> i32 {
    4
}
fn twelve() -> i32 {
    12
}

pub async fn create(State(state): State<AppState>, admin: AdminUser, Json(req): Json<CreateTarget>) -> AppResult<Json<Value>> {
    let name = clean_name(&req.name)?;
    let kind = Kind::parse(&req.kind)?;
    let settings = tidy_settings(req.settings);
    let secrets = tidy(req.secrets);
    engine::validate(kind, &settings, &secrets)?;
    check_local_path(&state, kind, &settings)?;
    let schedule = Schedule::parse(&req.schedule)?;
    check_keep(req.keep_daily, req.keep_weekly, req.keep_monthly)?;

    let (repo_password, shown_key) = if req.encrypted {
        match req.recovery_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
            Some(existing) => (Some(state.secrets.seal(existing)), None),
            None => {
                let key = new_recovery_key();
                (Some(state.secrets.seal(&key)), Some(key))
            }
        }
    } else {
        (None, None)
    };
    let next = if req.enabled { schedule.next_from_now() } else { None };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO backup_targets (name, kind, settings, secrets, encrypted, repo_password, schedule,
             keep_daily, keep_weekly, keep_monthly, include_trash, enabled, next_run_at, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING id",
    )
    .bind(&name)
    .bind(kind.as_str())
    .bind(SqlJson(&settings))
    .bind(seal_secrets(&state, &secrets)?)
    .bind(req.encrypted)
    .bind(repo_password)
    .bind(schedule.to_text())
    .bind(req.keep_daily)
    .bind(req.keep_weekly)
    .bind(req.keep_monthly)
    .bind(req.include_trash)
    .bind(req.enabled)
    .bind(next)
    .bind(admin.0.id)
    .fetch_one(&state.db)
    .await?;
    tracing::info!(%id, kind = kind.as_str(), "backup destination added");
    let target = to_dto(&state, backup::load_target(&state, id).await?).await?;
    Ok(Json(json!({ "target": target, "recovery_key": shown_key })))
}

#[derive(Deserialize)]
pub struct UpdateTarget {
    name: Option<String>,
    settings: Option<Settings>,
    /// Only the secrets given (non-empty) replace the stored ones.
    secrets: Option<SecretSettings>,
    schedule: Option<String>,
    keep_daily: Option<i32>,
    keep_weekly: Option<i32>,
    keep_monthly: Option<i32>,
    include_trash: Option<bool>,
    enabled: Option<bool>,
}

pub async fn update(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTarget>,
) -> AppResult<Json<TargetDto>> {
    let t = backup::load_target(&state, id).await?;
    let kind = Kind::parse(&t.kind)?;
    let name = req.name.as_deref().map(clean_name).transpose()?.unwrap_or(t.name.clone());

    let old_secrets = backup::destination(&state, &t)?.secrets;
    let new = req.secrets.map(tidy).unwrap_or_default();
    let secrets = SecretSettings {
        password: new.password.or(old_secrets.password.clone()),
        access_key: new.access_key.or(old_secrets.access_key.clone()),
        secret_key: new.secret_key.or(old_secrets.secret_key.clone()),
    };
    let settings = req.settings.map(tidy_settings).unwrap_or_else(|| t.settings.0.clone());
    let moved = settings != t.settings.0;
    let creds_changed = secrets.password != old_secrets.password
        || secrets.access_key != old_secrets.access_key
        || secrets.secret_key != old_secrets.secret_key;
    if (moved || creds_changed) && state.backup_running(id).is_some() {
        return Err(AppError::Conflict("wait for the current run to finish before changing where backups go".into()));
    }
    engine::validate(kind, &settings, &secrets)?;
    check_local_path(&state, kind, &settings)?;

    let schedule = match &req.schedule {
        Some(s) => Schedule::parse(s)?,
        None => Schedule::parse(&t.schedule)?,
    };
    let (d, w, m) = (
        req.keep_daily.unwrap_or(t.keep_daily),
        req.keep_weekly.unwrap_or(t.keep_weekly),
        req.keep_monthly.unwrap_or(t.keep_monthly),
    );
    check_keep(d, w, m)?;
    let enabled = req.enabled.unwrap_or(t.enabled);
    let next = if enabled { schedule.next_from_now() } else { None };

    sqlx::query(
        "UPDATE backup_targets SET name = $2, settings = $3, secrets = $4, schedule = $5, keep_daily = $6,
             keep_weekly = $7, keep_monthly = $8, include_trash = $9, enabled = $10, next_run_at = $11,
             -- A new location needs its backup store created (or found) again.
             initialized = initialized AND NOT $12
         WHERE id = $1",
    )
    .bind(id)
    .bind(&name)
    .bind(SqlJson(&settings))
    .bind(seal_secrets(&state, &secrets)?)
    .bind(schedule.to_text())
    .bind(d)
    .bind(w)
    .bind(m)
    .bind(req.include_trash.unwrap_or(t.include_trash))
    .bind(enabled)
    .bind(next)
    .bind(moved)
    .execute(&state.db)
    .await?;
    Ok(Json(to_dto(&state, backup::load_target(&state, id).await?).await?))
}

/// Forgets a destination. The backups already stored there are kept.
pub async fn remove(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    if state.backup_running(id).is_some() {
        return Err(AppError::Conflict("wait for the current run to finish".into()));
    }
    let n = sqlx::query("DELETE FROM backup_targets WHERE id = $1").bind(id).execute(&state.db).await?.rows_affected();
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

// ---- actions -----------------------------------------------------------------

/// Checks the destination can be reached and prepares the backup store there.
pub async fn test(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let t = backup::load_target(&state, id).await?;
    let dest = backup::destination(&state, &t)?;
    require_engine(&state, dest.kind)?;
    if !state.claim_backup(id) {
        return Err(AppError::Conflict("a run is in progress; it already proves the connection works".into()));
    }
    let result = quick(state.engine.init(&dest)).await;
    state.release_backup(id);
    result?;
    sqlx::query("UPDATE backup_targets SET initialized = TRUE WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn run_now(State(state): State<AppState>, admin: AdminUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let t = backup::load_target(&state, id).await?;
    require_engine(&state, Kind::parse(&t.kind)?)?;
    let run = backup::start_backup(&state, id, "manual", Some(admin.0.id)).await?;
    Ok(Json(json!({ "run_id": run })))
}

#[derive(Deserialize)]
pub struct RunsQuery {
    limit: Option<i64>,
}

pub async fn runs(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
    Query(q): Query<RunsQuery>,
) -> AppResult<Json<Vec<RunRow>>> {
    backup::load_target(&state, id).await?;
    let limit = q.limit.unwrap_or(30).clamp(1, 200);
    Ok(Json(
        sqlx::query_as(&format!(
            "SELECT {RUN_COLS} FROM backup_runs WHERE target_id = $1 ORDER BY started_at DESC LIMIT $2"
        ))
        .bind(id)
        .bind(limit)
        .fetch_all(&state.db)
        .await?,
    ))
}

pub async fn snapshots(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<Uuid>) -> AppResult<Json<Vec<Snapshot>>> {
    let t = backup::load_target(&state, id).await?;
    let dest = backup::destination(&state, &t)?;
    require_engine(&state, dest.kind)?;
    let out = quick(state.engine.run(&dest, &["snapshots", "--json", "--tag", "fennoly"], |_| {})).await?;
    let mut list: Vec<Snapshot> = serde_json::from_str(out.trim()).unwrap_or_default();
    // RFC 3339 times from one clock sort correctly as text.
    list.sort_by(|a, b| b.time.cmp(&a.time));
    Ok(Json(list))
}

fn check_snapshot_id(s: &str) -> AppResult<()> {
    if s.len() < 8 || s.len() > 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::bad("invalid snapshot"));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct LsQuery {
    #[serde(default)]
    path: String,
}

#[derive(Serialize)]
pub struct BrowseEntry {
    name: String,
    /// A friendlier name where Fennoly knows one (a member's display name).
    label: Option<String>,
    /// Path inside the spaces folder, as used for restore.
    rel: String,
    is_dir: bool,
    size: Option<i64>,
    mtime: Option<String>,
}

/// Lists one folder of a snapshot (relative to the spaces folder).
pub async fn browse(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((id, snap)): Path<(Uuid, String)>,
    Query(q): Query<LsQuery>,
) -> AppResult<Json<Vec<BrowseEntry>>> {
    check_snapshot_id(&snap)?;
    let rel = backup::safe_rel(&q.path)?;
    let t = backup::load_target(&state, id).await?;
    let dest = backup::destination(&state, &t)?;
    require_engine(&state, dest.kind)?;
    let dir = backup::spaces_snapshot_path(&state, &rel);
    let out = quick(state.engine.run(&dest, &["ls", "--json", &snap, &dir], |_| {})).await?;

    // Members' folders are named after their user names; show display names.
    let labels: Vec<(String, String)> = if rel.is_empty() || rel == "users" {
        sqlx::query_as("SELECT s.dir_name, u.display_name FROM spaces s JOIN users u ON u.id = s.owner_id")
            .fetch_all(&state.db)
            .await?
    } else {
        Vec::new()
    };
    let prefix = format!("{dir}/");
    let mut entries: Vec<BrowseEntry> = engine::parse_ls(&out)
        .into_iter()
        // Only direct children of the folder asked for.
        .filter(|e| e.path.strip_prefix(&prefix).is_some_and(|rest| !rest.is_empty() && !rest.contains('/')))
        .map(|e| {
            let child_rel = if rel.is_empty() { e.name.clone() } else { format!("{rel}/{}", e.name) };
            let label = match child_rel.as_str() {
                "family" => Some("Family".to_string()),
                "users" => Some("Members".to_string()),
                other => labels.iter().find(|(d, _)| d == other).map(|(_, n)| n.clone()),
            };
            BrowseEntry {
                is_dir: e.kind == "dir",
                name: e.name,
                label,
                rel: child_rel,
                size: e.size,
                mtime: e.mtime,
            }
        })
        .collect();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Json(entries))
}

pub async fn restore(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((id, snap)): Path<(Uuid, String)>,
    Json(req): Json<backup::RestoreRequest>,
) -> AppResult<Json<Value>> {
    check_snapshot_id(&snap)?;
    let t = backup::load_target(&state, id).await?;
    require_engine(&state, Kind::parse(&t.kind)?)?;
    let run = backup::start_restore(&state, id, snap, req.paths, admin.0.id).await?;
    Ok(Json(json!({ "run_id": run })))
}

#[derive(Deserialize)]
pub struct RevealKey {
    password: String,
}

/// Shows the recovery key again, after the admin confirms their password.
pub async fn recovery_key(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RevealKey>,
) -> AppResult<Json<Value>> {
    let admin = admin.0;
    if state.login_locked(&admin.username) {
        return Err(AppError::TooManyRequests);
    }
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(admin.id)
        .fetch_one(&state.db)
        .await?;
    if !auth::verify_password(req.password, hash).await? {
        state.record_login_failure(&admin.username);
        return Err(AppError::bad("wrong password"));
    }
    let t = backup::load_target(&state, id).await?;
    let key = match (&t.repo_password, t.encrypted) {
        (Some(sealed), true) => Some(state.secrets.open(sealed)?),
        _ => None,
    };
    Ok(Json(json!({ "recovery_key": key })))
}

/// For the warning banner: are backups set up, and did one succeed lately?
pub async fn status(State(state): State<AppState>, _admin: AdminUser) -> AppResult<Json<Value>> {
    let (targets, enabled, oldest_enabled): (i64, i64, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE enabled), min(created_at) FILTER (WHERE enabled) FROM backup_targets",
    )
    .fetch_one(&state.db)
    .await?;
    let last_success: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT max(r.finished_at) FROM backup_runs r JOIN backup_targets t ON t.id = r.target_id
         WHERE t.enabled AND r.kind = 'backup' AND r.status = 'success'",
    )
    .fetch_one(&state.db)
    .await?;
    let last_failed: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT status = 'failed' FROM backup_runs r JOIN backup_targets t ON t.id = r.target_id
                          WHERE t.enabled AND r.kind = 'backup' ORDER BY r.started_at DESC LIMIT 1), FALSE)",
    )
    .fetch_one(&state.db)
    .await?;
    let stale_after = chrono::Duration::days(3);
    let now = Utc::now();
    let stale = enabled > 0
        && match last_success {
            Some(at) => now - at > stale_after,
            None => oldest_enabled.is_some_and(|c| now - c > chrono::Duration::days(1)),
        };
    Ok(Json(json!({
        "targets": targets,
        "enabled": enabled,
        "last_success_at": last_success,
        "last_failed": last_failed,
        "stale": stale,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_keys_look_right() {
        let k = new_recovery_key();
        assert_eq!(k.len(), 39);
        let groups: Vec<&str> = k.split('-').collect();
        assert_eq!(groups.len(), 8);
        assert!(groups.iter().all(|g| g.len() == 4));
        assert!(k.chars().all(|c| c == '-' || "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c)));
        assert_ne!(k, new_recovery_key());
    }

    #[test]
    fn inside_checks() {
        let p = FsPath::new;
        if cfg!(windows) {
            assert!(same_or_inside(p(r"E:\Fennoly\data\backups"), p(r"e:\fennoly\data")));
            assert!(same_or_inside(p(r"\\?\E:\Fennoly\data"), p(r"E:\Fennoly\data\")));
            assert!(!same_or_inside(p(r"E:\Fennoly\data-backups"), p(r"E:\Fennoly\data")));
            assert!(!same_or_inside(p(r"F:\Backups"), p(r"E:\Fennoly\data")));
        } else {
            assert!(same_or_inside(p("/srv/fennoly/data/b"), p("/srv/fennoly/data")));
            assert!(!same_or_inside(p("/srv/fennoly/data2"), p("/srv/fennoly/data")));
            assert!(!same_or_inside(p("/SRV/fennoly/data"), p("/srv/fennoly/data")));
        }
    }

    #[test]
    fn keep_limits() {
        assert!(check_keep(7, 4, 12).is_ok());
        assert!(check_keep(0, 0, 0).is_err());
        assert!(check_keep(366, 0, 0).is_err());
        assert!(check_keep(-1, 4, 0).is_err());
    }
}
