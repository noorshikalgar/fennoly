//! Fennoly's chunked upload protocol: fast, parallel, resumable and verified.
//!
//! The idea (inspired by copyparty's up2k, written from scratch here):
//!
//! 1. **Plan** — the browser splits the file into fixed-size chunks and hashes
//!    each one with BLAKE3. It sends the list of hashes in one request.
//! 2. The server creates (or finds) the upload and answers with the chunks it
//!    is still **missing**. If the same file was sent to the same folder before
//!    and interrupted, only the remainder is missing.
//! 3. **Send** — the browser uploads missing chunks several at a time, in any
//!    order. The server writes each chunk straight to its place in a pre-sized
//!    file while hashing it, and only marks the chunk received when the hash
//!    matches. A chunk cut off by a dropped connection simply stays missing.
//! 4. **Finish** — once every chunk is received the file is complete and
//!    correct by construction, and it is moved into its folder.
//!
//! Memory use is constant: chunks are streamed to disk, never buffered whole.

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use uuid::Uuid;

use super::uploads::{blake3_file, check_quota, check_size, place_file, target_folder};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::remove_path;

const MIB: i64 = 1024 * 1024;
const MIN_CHUNK: i64 = MIB;
const MAX_CHUNK: i64 = 64 * MIB;
const MAX_CHUNKS: usize = 16_384;

#[derive(Debug, Clone, FromRow)]
struct ChunkedRow {
    id: Uuid,
    parent_id: Uuid,
    filename: String,
    size: i64,
    chunk_size: i64,
    chunk_hashes: Vec<String>,
    received: Vec<bool>,
    node_id: Option<Uuid>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

const COLS: &str = "id, parent_id, filename, size, chunk_size, chunk_hashes, received, node_id, completed_at";

impl ChunkedRow {
    fn missing(&self) -> Vec<u32> {
        self.received
            .iter()
            .enumerate()
            .filter(|(_, r)| !**r)
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// Byte length of chunk `index` (the last one may be shorter).
    fn chunk_len(&self, index: usize) -> i64 {
        let start = index as i64 * self.chunk_size;
        (self.size - start).min(self.chunk_size)
    }
}

/// Checks the client's chunk plan. Exposed for tests.
pub fn validate_plan(size: i64, chunk_size: i64, hashes: &[String]) -> AppResult<()> {
    if size < 0 {
        return Err(AppError::bad("size cannot be negative"));
    }
    if !(MIN_CHUNK..=MAX_CHUNK).contains(&chunk_size) || chunk_size % MIB != 0 {
        return Err(AppError::bad("chunk size must be a whole number of MiB between 1 and 64"));
    }
    let expected = if size == 0 { 0 } else { ((size + chunk_size - 1) / chunk_size) as usize };
    if hashes.len() != expected {
        return Err(AppError::bad(format!("expected {expected} chunk hashes, got {}", hashes.len())));
    }
    if hashes.len() > MAX_CHUNKS {
        return Err(AppError::bad("too many chunks; use a bigger chunk size"));
    }
    let well_formed = hashes
        .iter()
        .all(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    if !well_formed {
        return Err(AppError::bad("chunk hashes must be lowercase hex BLAKE3 digests"));
    }
    Ok(())
}

fn manifest(parent_id: Uuid, filename: &str, size: i64, chunk_size: i64, hashes: &[String]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(parent_id.as_bytes());
    h.update(filename.as_bytes());
    h.update(&size.to_le_bytes());
    h.update(&chunk_size.to_le_bytes());
    for hash in hashes {
        h.update(hash.as_bytes());
    }
    h.finalize().to_hex().to_string()
}

#[derive(Deserialize)]
pub struct Plan {
    parent_id: Uuid,
    filename: String,
    size: i64,
    chunk_size: i64,
    chunk_hashes: Vec<String>,
}

#[derive(Serialize)]
pub struct PlanResponse {
    id: Uuid,
    chunk_size: i64,
    chunk_count: usize,
    missing: Vec<u32>,
    /// True when an earlier, interrupted attempt is being continued.
    resumed: bool,
}

pub async fn plan(State(state): State<AppState>, user: CurrentUser, Json(req): Json<Plan>) -> AppResult<Json<PlanResponse>> {
    validate_plan(req.size, req.chunk_size, &req.chunk_hashes)?;
    fennoly_core::validate_name(&req.filename)?;
    check_size(&state, req.size as u64)?;
    let folder = target_folder(&state, &user, &req.parent_id.to_string()).await?;
    let key = manifest(req.parent_id, &req.filename, req.size, req.chunk_size, &req.chunk_hashes);

    // Same file, same folder, not finished yet: pick up where it stopped.
    let existing: Option<ChunkedRow> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM chunked_uploads WHERE user_id = $1 AND manifest = $2 AND completed_at IS NULL"
    ))
    .bind(user.id)
    .bind(&key)
    .fetch_optional(&state.db)
    .await?;
    if let Some(row) = existing {
        if tokio::fs::metadata(state.storage.chunked_file(row.id)).await.is_ok() {
            return Ok(Json(PlanResponse {
                id: row.id,
                chunk_size: row.chunk_size,
                chunk_count: row.received.len(),
                missing: row.missing(),
                resumed: true,
            }));
        }
        // The partial file vanished (cleanup or disk change): start again.
        sqlx::query("DELETE FROM chunked_uploads WHERE id = $1").bind(row.id).execute(&state.db).await?;
    }

    check_quota(&state, &folder, req.size as u64).await?;
    let id = Uuid::new_v4();
    let path = state.storage.chunked_file(id);
    let file = tokio::fs::File::create(&path).await?;
    // Reserve the full length up front so chunks can land in any order.
    file.set_len(req.size as u64).await?;
    drop(file);

    let count = req.chunk_hashes.len();
    sqlx::query(
        "INSERT INTO chunked_uploads (id, user_id, parent_id, filename, size, chunk_size, chunk_hashes, received, manifest)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(user.id)
    .bind(req.parent_id)
    .bind(&req.filename)
    .bind(req.size)
    .bind(req.chunk_size)
    .bind(&req.chunk_hashes)
    .bind(vec![false; count])
    .bind(&key)
    .execute(&state.db)
    .await?;
    Ok(Json(PlanResponse {
        id,
        chunk_size: req.chunk_size,
        chunk_count: count,
        missing: (0..count as u32).collect(),
        resumed: false,
    }))
}

async fn load(state: &AppState, user: &CurrentUser, id: Uuid) -> AppResult<ChunkedRow> {
    sqlx::query_as(&format!("SELECT {COLS} FROM chunked_uploads WHERE id = $1 AND user_id = $2"))
        .bind(id)
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

#[derive(Serialize)]
pub struct Status {
    missing: Vec<u32>,
    chunk_count: usize,
    done: bool,
    node_id: Option<Uuid>,
}

pub async fn status(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Status>> {
    let row = load(&state, &user, id).await?;
    Ok(Json(Status { missing: row.missing(), chunk_count: row.received.len(), done: row.completed_at.is_some(), node_id: row.node_id }))
}

/// Receives one chunk. The body is streamed to its place in the file and
/// hashed on the way; the chunk only counts if the hash matches the plan.
pub async fn put_chunk(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, index)): Path<(Uuid, u32)>,
    body: Body,
) -> AppResult<Json<Value>> {
    let row = load(&state, &user, id).await?;
    if row.completed_at.is_some() {
        return Err(AppError::Conflict("this upload is already finished".into()));
    }
    let index = index as usize;
    if index >= row.chunk_hashes.len() {
        return Err(AppError::bad("chunk index out of range"));
    }
    let expected_len = row.chunk_len(index) as u64;
    let offset = index as u64 * row.chunk_size as u64;

    let mut file = tokio::fs::OpenOptions::new().write(true).open(state.storage.chunked_file(id)).await?;
    file.seek(std::io::SeekFrom::Start(offset)).await?;
    let mut hasher = blake3::Hasher::new();
    let mut written: u64 = 0;
    let mut stream = body.into_data_stream();
    while let Some(frame) = stream.next().await {
        let bytes = frame.map_err(|_| AppError::bad("connection interrupted"))?;
        written += bytes.len() as u64;
        if written > expected_len {
            return Err(AppError::bad("chunk is larger than planned"));
        }
        hasher.update(&bytes);
        file.write_all(&bytes).await?;
    }
    if written != expected_len {
        return Err(AppError::bad(format!("chunk was cut short ({written} of {expected_len} bytes)")));
    }
    file.flush().await?;
    file.sync_data().await?;
    drop(file);

    if hasher.finalize().to_hex().as_str() != row.chunk_hashes[index] {
        // The bytes on disk for this chunk are wrong, but the chunk stays
        // "missing", so the next attempt overwrites them.
        return Err(AppError::bad("chunk checksum mismatch; please resend"));
    }
    let remaining: i64 = sqlx::query_scalar(
        "UPDATE chunked_uploads SET received[$2] = TRUE, updated_at = now() WHERE id = $1
         RETURNING cardinality(array_positions(received, FALSE))::BIGINT",
    )
    .bind(id)
    .bind(index as i32 + 1)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({ "remaining": remaining })))
}

/// Completes the upload once every chunk has arrived intact.
pub async fn finish(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let lock = state.upload_lock(id);
    let _guard = lock.lock().await;
    let row = load(&state, &user, id).await?;
    if let (Some(_), Some(node)) = (row.completed_at, row.node_id) {
        // A retried finish after a lost response: report the same result.
        return Ok(Json(json!({ "node_id": node })));
    }
    let missing = row.missing();
    if !missing.is_empty() {
        return Err(AppError::Conflict(format!("{} chunks are still missing", missing.len())));
    }
    let tmp = state.storage.chunked_file(id);
    let hash = blake3_file(&tmp).await?;
    let node_id = place_file(&state, &user, row.parent_id, &row.filename, &tmp, row.size, &hash).await?;
    sqlx::query("UPDATE chunked_uploads SET completed_at = now(), node_id = $1 WHERE id = $2")
        .bind(node_id)
        .bind(id)
        .execute(&state.db)
        .await?;
    state.drop_upload_lock(id);
    Ok(Json(json!({ "node_id": node_id, "content_hash": hash })))
}

pub async fn cancel(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let row = load(&state, &user, id).await?;
    if row.completed_at.is_none() {
        remove_path(&state.storage.chunked_file(id)).await?;
    }
    sqlx::query("DELETE FROM chunked_uploads WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hashes(n: usize) -> Vec<String> {
        vec!["a".repeat(64); n]
    }

    #[test]
    fn plan_must_match_size() {
        assert!(validate_plan(10 * MIB, 4 * MIB, &hashes(3)).is_ok());
        assert!(validate_plan(8 * MIB, 4 * MIB, &hashes(2)).is_ok());
        assert!(validate_plan(8 * MIB, 4 * MIB, &hashes(3)).is_err());
        assert!(validate_plan(0, 4 * MIB, &hashes(0)).is_ok());
    }

    #[test]
    fn chunk_size_rules() {
        assert!(validate_plan(MIB, MIB / 2, &hashes(2)).is_err());
        assert!(validate_plan(MIB, 3 * MIB / 2, &hashes(1)).is_err());
        assert!(validate_plan(128 * MIB, 128 * MIB, &hashes(1)).is_err());
    }

    #[test]
    fn hashes_must_be_hex() {
        assert!(validate_plan(MIB, MIB, &["Z".repeat(64)]).is_err());
        assert!(validate_plan(MIB, MIB, &["ab".to_string()]).is_err());
    }

    #[test]
    fn last_chunk_is_shorter() {
        let row = ChunkedRow {
            id: Uuid::nil(),
            parent_id: Uuid::nil(),
            filename: "x".into(),
            size: 10 * MIB + 5,
            chunk_size: 4 * MIB,
            chunk_hashes: hashes(3),
            received: vec![true, false, false],
            node_id: None,
            completed_at: None,
        };
        assert_eq!(row.chunk_len(0), 4 * MIB);
        assert_eq!(row.chunk_len(2), 2 * MIB + 5);
        assert_eq!(row.missing(), vec![1, 2]);
    }

    #[test]
    fn manifest_depends_on_folder_and_content() {
        let a = manifest(Uuid::nil(), "f", 1, MIB, &hashes(1));
        let b = manifest(Uuid::new_v4(), "f", 1, MIB, &hashes(1));
        let c = manifest(Uuid::nil(), "f", 1, MIB, &["b".repeat(64)]);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, manifest(Uuid::nil(), "f", 1, MIB, &hashes(1)));
    }
}
