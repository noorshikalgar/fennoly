//! Backups: scheduled restic snapshots of the database and every space, to
//! local drives, NAS shares, SFTP servers and S3-compatible storage, plus
//! restoring files from any snapshot.
//!
//! A run, step by step:
//! 1. claim the destination (one run at a time per destination),
//! 2. dump the database (`pg_dump`) and write a manifest into backup-staging,
//! 3. create the restic repository if needed, then `restic backup`,
//! 4. apply the retention policy (`restic forget --prune`),
//! 5. once a month, re-read a 5% sample of the stored data (`restic check`),
//! 6. record the result. Failures are recorded with a plain-language reason.

pub mod engine;
pub mod schedule;

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::remove_path;
use engine::{BackupMessage, Destination, Kind, SecretSettings, Settings, Summary};
use schedule::Schedule;

#[derive(Debug, Clone, FromRow)]
pub struct TargetRow {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub settings: Json<Settings>,
    pub secrets: Option<String>,
    pub encrypted: bool,
    pub repo_password: Option<String>,
    pub schedule: String,
    pub keep_daily: i32,
    pub keep_weekly: i32,
    pub keep_monthly: i32,
    pub include_trash: bool,
    pub enabled: bool,
    pub initialized: bool,
    pub next_run_at: Option<DateTime<Utc>>,
}

pub const TARGET_COLS: &str = "id, name, kind, settings, secrets, encrypted, repo_password, schedule, \
     keep_daily, keep_weekly, keep_monthly, include_trash, enabled, initialized, next_run_at";

pub async fn load_target(state: &AppState, id: Uuid) -> AppResult<TargetRow> {
    sqlx::query_as(&format!("SELECT {TARGET_COLS} FROM backup_targets WHERE id = $1"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

/// Decrypts what the engine needs to reach a destination.
pub fn destination(state: &AppState, t: &TargetRow) -> AppResult<Destination> {
    let secrets: SecretSettings = match &t.secrets {
        Some(sealed) => serde_json::from_str(&state.secrets.open(sealed)?)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("bad stored secrets: {e}")))?,
        None => SecretSettings::default(),
    };
    let repo_password = match (&t.repo_password, t.encrypted) {
        (Some(sealed), true) => Some(state.secrets.open(sealed)?),
        (None, true) => return Err(AppError::Internal(anyhow::anyhow!("encrypted destination has no recovery key"))),
        _ => None,
    };
    Ok(Destination { kind: Kind::parse(&t.kind)?, settings: t.settings.0.clone(), secrets, repo_password })
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct RunRow {
    pub id: Uuid,
    pub target_id: Uuid,
    pub kind: String,
    pub status: String,
    pub trigger: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub snapshot_id: Option<String>,
    pub files_new: Option<i64>,
    pub files_changed: Option<i64>,
    pub bytes_added: Option<i64>,
    pub total_files: Option<i64>,
    pub total_bytes: Option<i64>,
    pub error: Option<String>,
}

pub const RUN_COLS: &str = "id, target_id, kind, status, trigger, started_at, finished_at, snapshot_id, \
     files_new, files_changed, bytes_added, total_files, total_bytes, error";

async fn start_run(state: &AppState, target: Uuid, kind: &str, trigger: &str, by: Option<Uuid>) -> AppResult<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO backup_runs (target_id, kind, status, trigger, started_by) VALUES ($1, $2, 'running', $3, $4) RETURNING id",
    )
    .bind(target)
    .bind(kind)
    .bind(trigger)
    .bind(by)
    .fetch_one(&state.db)
    .await?)
}

async fn finish_run(state: &AppState, run: Uuid, result: &AppResult<Summary>) {
    let outcome = match result {
        Ok(s) => sqlx::query(
            "UPDATE backup_runs SET status = 'success', finished_at = now(), snapshot_id = $2, files_new = $3,
                 files_changed = $4, bytes_added = $5, total_files = $6, total_bytes = $7 WHERE id = $1",
        )
        .bind(run)
        .bind(&s.snapshot_id)
        .bind(s.files_new)
        .bind(s.files_changed)
        .bind(s.data_added)
        .bind(s.total_files_processed)
        .bind(s.total_bytes_processed)
        .execute(&state.db)
        .await,
        Err(e) => sqlx::query("UPDATE backup_runs SET status = 'failed', finished_at = now(), error = $2 WHERE id = $1")
            .bind(run)
            .bind(e.to_string())
            .execute(&state.db)
            .await,
    };
    if let Err(e) = outcome {
        tracing::error!(error = %e, %run, "could not record backup run");
    }
}

/// Starts a backup in the background and returns its run id.
pub async fn start_backup(state: &AppState, target_id: Uuid, trigger: &str, by: Option<Uuid>) -> AppResult<Uuid> {
    let target = load_target(state, target_id).await?;
    if !state.claim_backup(target_id) {
        return Err(AppError::Conflict("a backup to this destination is already running".into()));
    }
    let run = match start_run(state, target_id, "backup", trigger, by).await {
        Ok(r) => r,
        Err(e) => {
            state.release_backup(target_id);
            return Err(e);
        }
    };
    let state = state.clone();
    tokio::spawn(async move {
        let result = run_backup(&state, &target).await;
        if let Err(e) = &result {
            tracing::warn!(target = %target.name, error = %e, "backup failed");
        }
        finish_run(&state, run, &result).await;
        // Monthly deep check, after a successful backup.
        if result.is_ok() {
            if let Err(e) = maybe_check(&state, &target).await {
                tracing::warn!(target = %target.name, error = %e, "backup check failed");
            }
        }
        state.release_backup(target.id);
    });
    Ok(run)
}

/// Program and leading arguments for pg_dump (`FENNOLY_PG_DUMP`, default `pg_dump`).
fn pg_dump_command(state: &AppState) -> (String, Vec<String>) {
    let raw = state.cfg.pg_dump.clone().unwrap_or_else(|| "pg_dump".into());
    let mut parts = raw.split_whitespace().map(String::from);
    let program = parts.next().unwrap_or_else(|| "pg_dump".into());
    (program, parts.collect())
}

/// Dumps the database (custom format) into the staging folder.
async fn dump_database(state: &AppState, staging: &Path) -> AppResult<()> {
    let (program, mut args) = pg_dump_command(state);
    let url = state.cfg.pg_dump_url.clone().unwrap_or_else(|| state.cfg.database_url.clone());
    args.extend(["--format=custom".into(), "--no-owner".into(), format!("--dbname={url}")]);
    let out = tokio::process::Command::new(&program)
        .args(&args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::Unavailable("pg_dump is not installed on the server, so the database cannot be backed up".into())
            } else {
                e.into()
            }
        })?;
    if !out.status.success() || out.stdout.is_empty() {
        let msg = String::from_utf8_lossy(&out.stderr);
        return Err(AppError::BadRequest(format!("database dump failed: {}", msg.trim().chars().take(300).collect::<String>())));
    }
    tokio::fs::write(staging.join("fennoly.dump"), &out.stdout).await?;
    Ok(())
}

#[derive(Serialize)]
struct Manifest {
    fennoly_version: &'static str,
    schema_version: i64,
    created_at: DateTime<Utc>,
    files: i64,
    bytes: i64,
    users: i64,
}

async fn write_manifest(state: &AppState, staging: &Path) -> AppResult<()> {
    let schema_version: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
        .fetch_one(&state.db)
        .await?;
    let (files, bytes): (i64, i64) = sqlx::query_as(
        "SELECT count(*), COALESCE(SUM(size), 0)::BIGINT FROM nodes WHERE kind = 'file' AND trashed_at IS NULL",
    )
    .fetch_one(&state.db)
    .await?;
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users").fetch_one(&state.db).await?;
    let manifest = Manifest { fennoly_version: env!("CARGO_PKG_VERSION"), schema_version, created_at: Utc::now(), files, bytes, users };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| AppError::Internal(e.into()))?;
    tokio::fs::write(staging.join("manifest.json"), json).await?;
    Ok(())
}

/// Backups run one at a time: they share the staging folder (the database
/// dump), and two full reads of the same disk at once only slow both down.
static BACKUP_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Longest a single backup may take before it is stopped (a hung network
/// share must not keep the destination busy forever).
const BACKUP_TIMEOUT: Duration = Duration::from_secs(24 * 3600);

async fn run_backup(state: &AppState, target: &TargetRow) -> AppResult<Summary> {
    let _gate = BACKUP_GATE.lock().await;
    match tokio::time::timeout(BACKUP_TIMEOUT, backup_steps(state, target)).await {
        Ok(result) => result,
        Err(_) => {
            let _ = remove_path(&state.storage.backup_staging()).await;
            Err(AppError::BadRequest("the backup took longer than 24 hours and was stopped".into()))
        }
    }
}

async fn backup_steps(state: &AppState, target: &TargetRow) -> AppResult<Summary> {
    let dest = destination(state, target)?;
    if !target.initialized {
        state.engine.init(&dest).await?;
        sqlx::query("UPDATE backup_targets SET initialized = TRUE WHERE id = $1")
            .bind(target.id)
            .execute(&state.db)
            .await?;
    }

    let staging = state.storage.backup_staging();
    remove_path(&staging).await?;
    tokio::fs::create_dir_all(&staging).await?;
    dump_database(state, &staging).await?;
    write_manifest(state, &staging).await?;

    let mut paths: Vec<PathBuf> = vec![state.storage.spaces_root(), staging.clone()];
    if target.include_trash {
        paths.push(state.storage.trash_root());
    }
    let paths: Vec<String> = paths.iter().map(|p| plain(p)).collect();
    let mut args: Vec<&str> = vec!["backup", "--json", "--tag", "fennoly", "--host", "fennoly"];
    args.extend(paths.iter().map(String::as_str));

    let mut summary = None;
    let id = target.id;
    let result = state
        .engine
        .run(&dest, &args, |line| match engine::parse_backup_line(line) {
            BackupMessage::Progress(p) => state.backup_progress(id, p.percent_done),
            BackupMessage::Summary(s) => summary = Some(s),
            BackupMessage::Other => {}
        })
        .await;
    // The dump holds every account; never leave it lying around.
    let _ = remove_path(&staging).await;
    result?;
    let summary = summary.ok_or_else(|| AppError::BadRequest("the backup engine did not report a result".into()))?;

    // Retention: keep N daily / weekly / monthly snapshots, drop the rest.
    if target.keep_daily + target.keep_weekly + target.keep_monthly > 0 {
        let (d, w, m) = (target.keep_daily.to_string(), target.keep_weekly.to_string(), target.keep_monthly.to_string());
        let mut forget = vec!["forget", "--prune", "--tag", "fennoly", "--host", "fennoly"];
        for (flag, v) in [("--keep-daily", &d), ("--keep-weekly", &w), ("--keep-monthly", &m)] {
            if v != "0" {
                forget.extend([flag, v.as_str()]);
            }
        }
        state.engine.run(&dest, &forget, |_| {}).await?;
    }
    Ok(summary)
}

/// Once a month, re-read a 5% sample of the stored data to catch bit rot or a
/// failing disk early. Recorded as its own run.
async fn maybe_check(state: &AppState, target: &TargetRow) -> AppResult<()> {
    let recent: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM backup_runs WHERE target_id = $1 AND kind = 'check'
                        AND status = 'success' AND started_at > now() - interval '30 days')",
    )
    .bind(target.id)
    .fetch_one(&state.db)
    .await?;
    if recent {
        return Ok(());
    }
    let run = start_run(state, target.id, "check", "schedule", None).await?;
    let dest = destination(state, target)?;
    let result = match tokio::time::timeout(BACKUP_TIMEOUT, state.engine.run(&dest, &["check", "--read-data-subset=5%"], |_| {})).await {
        Ok(r) => r.map(|_| Summary::default()),
        Err(_) => Err(AppError::BadRequest("the check took longer than 24 hours and was stopped".into())),
    };
    finish_run(state, run, &result).await;
    result.map(|_| ())
}

/// Every minute: start destinations whose scheduled time has come. Call
/// [`recover_after_restart`] first.
pub fn spawn_scheduler(state: AppState) {
    tokio::spawn(async move {
        // Fill in next run times for destinations that have none yet.
        if let Err(e) = refresh_schedules(&state).await {
            tracing::warn!(error = %e, "could not compute backup schedules");
        }
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            if let Err(e) = run_due(&state).await {
                tracing::warn!(error = %e, "backup scheduler");
            }
        }
    });
}

/// Runs cut short by a shutdown or crash are marked failed, and their
/// leftover files (the database dump, half-restored files) are removed.
pub async fn recover_after_restart(state: &AppState) -> AppResult<()> {
    let n = sqlx::query(
        "UPDATE backup_runs SET status = 'failed', finished_at = now(),
             error = 'The server stopped while this was running.'
         WHERE status = 'running'",
    )
    .execute(&state.db)
    .await?
    .rows_affected();
    if n > 0 {
        tracing::warn!(runs = n, "backup runs were interrupted by the last shutdown");
    }
    remove_path(&state.storage.backup_staging()).await?;
    let restore_root = state.storage.tmp_dir().join("restore");
    remove_path(&restore_root).await?;
    tokio::fs::create_dir_all(&restore_root).await?;
    Ok(())
}

async fn refresh_schedules(state: &AppState) -> AppResult<()> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, schedule FROM backup_targets WHERE next_run_at IS NULL")
        .fetch_all(&state.db)
        .await?;
    for (id, sched) in rows {
        let next = Schedule::parse(&sched).ok().and_then(Schedule::next_from_now);
        sqlx::query("UPDATE backup_targets SET next_run_at = $2 WHERE id = $1")
            .bind(id)
            .bind(next)
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

async fn run_due(state: &AppState) -> AppResult<()> {
    let due: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, schedule FROM backup_targets WHERE enabled AND next_run_at IS NOT NULL AND next_run_at <= now()",
    )
    .fetch_all(&state.db)
    .await?;
    for (id, sched) in due {
        // Move the next time forward first, so a slow or failing run is not
        // retried every minute.
        let next = Schedule::parse(&sched).ok().and_then(Schedule::next_from_now);
        sqlx::query("UPDATE backup_targets SET next_run_at = $2 WHERE id = $1")
            .bind(id)
            .bind(next)
            .execute(&state.db)
            .await?;
        match start_backup(state, id, "schedule", None).await {
            Ok(_) | Err(AppError::Conflict(_)) => {}
            Err(e) => tracing::warn!(%id, error = %e, "scheduled backup did not start"),
        }
    }
    Ok(())
}

/// A path as restic should see it: without the Windows `\\?\` prefix that
/// `canonicalize` adds, which would change the paths stored in snapshots.
pub fn plain(p: &Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

// ---- snapshots and restore -------------------------------------------------

/// A snapshot path for something inside the spaces folder (`users/noor/Tax`).
pub fn spaces_snapshot_path(state: &AppState, rel: &str) -> String {
    let root = engine::snapshot_path(&state.storage.spaces_root());
    let rel = rel.trim_matches('/');
    if rel.is_empty() { root } else { format!("{root}/{rel}") }
}

/// Keeps a relative path inside the spaces folder (no `..`, no absolute paths).
pub fn safe_rel(rel: &str) -> AppResult<String> {
    let rel = rel.trim().trim_matches('/').replace('\\', "/");
    if rel.split('/').any(|seg| seg == ".." || seg == "." || seg.contains(':')) {
        return Err(AppError::bad("invalid path"));
    }
    Ok(rel)
}

#[derive(Debug, Deserialize)]
pub struct RestoreRequest {
    pub paths: Vec<String>,
}

/// Restores items from a snapshot into a new "Restored …" folder in the
/// requesting admin's files. Nothing existing is overwritten.
pub async fn start_restore(
    state: &AppState,
    target_id: Uuid,
    snapshot: String,
    rels: Vec<String>,
    user_id: Uuid,
) -> AppResult<Uuid> {
    if rels.is_empty() || rels.len() > 200 {
        return Err(AppError::bad("choose between 1 and 200 items to restore"));
    }
    if !snapshot.chars().all(|c| c.is_ascii_hexdigit()) || snapshot.len() < 8 {
        return Err(AppError::bad("invalid snapshot"));
    }
    let rels = rels.iter().map(|r| safe_rel(r)).collect::<AppResult<Vec<_>>>()?;
    let target = load_target(state, target_id).await?;
    if !state.claim_backup(target_id) {
        return Err(AppError::Conflict("this destination is busy; try again when the current run finishes".into()));
    }
    let run = match start_run(state, target_id, "restore", "manual", Some(user_id)).await {
        Ok(r) => r,
        Err(e) => {
            state.release_backup(target_id);
            return Err(e);
        }
    };
    let state = state.clone();
    tokio::spawn(async move {
        let result = restore_into_folder(&state, &target, run, &snapshot, &rels, user_id).await;
        if let Err(e) = &result {
            tracing::warn!(error = %e, "restore failed");
        }
        let snapshot_id = Some(snapshot.clone());
        finish_run(&state, run, &result.map(|_| Summary { snapshot_id, ..Summary::default() })).await;
        let _ = remove_path(&state.storage.restore_dir(run)).await;
        state.release_backup(target_id);
    });
    Ok(run)
}

async fn restore_into_folder(
    state: &AppState,
    target: &TargetRow,
    run: Uuid,
    snapshot: &str,
    rels: &[String],
    user_id: Uuid,
) -> AppResult<Uuid> {
    let dest = destination(state, target)?;
    let tmp = state.storage.restore_dir(run);
    tokio::fs::create_dir_all(&tmp).await?;
    let includes: Vec<String> = rels.iter().map(|r| spaces_snapshot_path(state, r)).collect();
    let tmp_s = plain(&tmp);
    let mut args = vec!["restore", snapshot, "--target", tmp_s.as_str()];
    for inc in &includes {
        args.extend(["--include", inc.as_str()]);
    }
    state.engine.run(&dest, &args, |_| {}).await?;

    // restic recreates the original absolute path under the target folder.
    let restored_root = tmp.join(engine::snapshot_path(&state.storage.spaces_root()).trim_start_matches('/'));
    let user = crate::auth::user_by_id(state, user_id).await?;
    let (home, _) = crate::spaces::roots(&state.db, user_id).await?;
    let home = home.ok_or(AppError::NotFound)?;
    let label = format!("Restored {}", chrono::Local::now().format("%Y-%m-%d %H.%M"));
    let label = crate::access::free_name(state, home, &label).await?;
    let folder = crate::routes::nodes::create_folder_node(state, &user, home, &label).await?;
    let mut found = 0;
    for rel in rels {
        let src = restored_root.join(rel);
        if tokio::fs::metadata(&src).await.is_ok() {
            import_tree(state, &user, folder, &src).await?;
            found += 1;
        }
    }
    if found == 0 {
        return Err(AppError::BadRequest("nothing was found in the backup at the chosen place".into()));
    }
    Ok(folder)
}

/// Adds a restored file or folder (recursively) under `parent`.
fn import_tree<'a>(
    state: &'a AppState,
    user: &'a crate::auth::CurrentUser,
    parent: Uuid,
    src: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<()>> + Send + 'a>> {
    Box::pin(async move {
        let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "restored".into());
        let meta = tokio::fs::metadata(src).await?;
        if meta.is_dir() {
            let free = crate::access::free_name(state, parent, &fennoly_core::validate_name(&name).unwrap_or_else(|_| "restored".into())).await?;
            let folder = crate::routes::nodes::create_folder_node(state, user, parent, &free).await?;
            let mut entries = tokio::fs::read_dir(src).await?;
            while let Some(entry) = entries.next_entry().await? {
                import_tree(state, user, folder, &entry.path()).await?;
            }
        } else {
            let hash = crate::routes::uploads::blake3_file(src).await?;
            crate::routes::uploads::place_file(state, user, parent, &name, src, meta.len() as i64, &hash).await?;
        }
        Ok(())
    })
}
