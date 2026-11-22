//! tus 1.0 upload endpoint with the concatenation extension, which is what
//! lets the web app upload one file as several parallel parts.
//!
//! Flow for a parallel upload:
//! 1. `POST /api/uploads` with `Upload-Concat: partial` once per part.
//! 2. `PATCH /api/uploads/{part}` for each part, at the same time.
//! 3. `POST /api/uploads` with `Upload-Concat: final;<part urls>` and the
//!    file's metadata. The server joins the parts and creates the file.
//!
//! A plain (non-parallel) upload is a normal tus upload with metadata.
//! Either way, an interrupted transfer resumes from the last saved offset.

use std::path::Path as FsPath;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use fennoly_tus::{Concat, TUS_EXTENSIONS, TUS_VERSION, header as tus};
use futures_util::StreamExt;
use sqlx::FromRow;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::access::{self, Resolved};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::spaces;
use crate::state::AppState;
use crate::storage::{move_path, remove_path};

const NODE_ID_HEADER: &str = "fennoly-node-id";

#[derive(Debug, Clone, FromRow)]
struct UploadRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    filename: Option<String>,
    length: i64,
    upload_offset: i64,
    is_partial: bool,
    node_id: Option<Uuid>,
    completed_at: Option<DateTime<Utc>>,
}

const UPLOAD_COLS: &str = "id, parent_id, filename, length, upload_offset, is_partial, node_id, completed_at";

fn hdr(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name).and_then(|v| v.to_str().ok()).map(String::from)
}

/// Builds a tus response: always carries `Tus-Resumable`.
fn tus_response(status: StatusCode, extra: &[(&str, String)]) -> Response {
    let mut res = status.into_response();
    let h = res.headers_mut();
    h.insert(HeaderName::from_static(tus::TUS_RESUMABLE), HeaderValue::from_static(TUS_VERSION));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    for (k, v) in extra {
        if let (Ok(name), Ok(value)) = (HeaderName::try_from(*k), HeaderValue::from_str(v)) {
            h.insert(name, value);
        }
    }
    res
}

pub async fn options(State(state): State<AppState>) -> Response {
    tus_response(
        StatusCode::NO_CONTENT,
        &[
            (tus::TUS_VERSION, TUS_VERSION.into()),
            (tus::TUS_EXTENSION, TUS_EXTENSIONS.into()),
            (tus::TUS_MAX_SIZE, state.cfg.max_upload_bytes.to_string()),
        ],
    )
}

async fn load(state: &AppState, user: &CurrentUser, id: Uuid) -> AppResult<UploadRow> {
    sqlx::query_as(&format!("SELECT {UPLOAD_COLS} FROM uploads WHERE id = $1 AND user_id = $2"))
        .bind(id)
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

/// The folder an upload goes into, checked for write access.
pub(super) async fn target_folder(state: &AppState, user: &CurrentUser, parent_id: &str) -> AppResult<Resolved> {
    let id: Uuid = parent_id.parse().map_err(|_| AppError::bad("bad parentId"))?;
    let parent = access::resolve(state, user.id, id).await?;
    parent.require_write()?;
    if !parent.node.is_folder() {
        return Err(AppError::bad("uploads must go into a folder"));
    }
    Ok(parent)
}

pub(super) fn check_size(state: &AppState, size: u64) -> AppResult<()> {
    if size > state.cfg.max_upload_bytes {
        return Err(AppError::TooLarge("this file is larger than the server allows".into()));
    }
    Ok(())
}

/// Personal spaces may have a quota; the family space does not.
pub(super) async fn check_quota(state: &AppState, folder: &Resolved, add: u64) -> AppResult<()> {
    let owner: Option<(Uuid, Option<i64>)> = sqlx::query_as(
        "SELECT u.id, u.quota_bytes FROM spaces s JOIN users u ON u.id = s.owner_id WHERE s.id = $1",
    )
    .bind(folder.node.space_id)
    .fetch_optional(&state.db)
    .await?;
    if let Some((owner_id, Some(quota))) = owner {
        let used = spaces::used_bytes(&state.db, owner_id).await?;
        if used as u64 + add > quota as u64 {
            return Err(AppError::TooLarge("not enough storage space left".into()));
        }
    }
    Ok(())
}

pub(crate) async fn blake3_file(path: &FsPath) -> AppResult<String> {
    let path = path.to_path_buf();
    let hash = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        let mut hasher = blake3::Hasher::new();
        hasher.update_reader(std::fs::File::open(path)?)?;
        Ok(hasher.finalize().to_hex().to_string())
    })
    .await??;
    Ok(hash)
}

/// Turns a finished tus upload into a file in its folder. Returns the node id.
async fn finalize(state: &AppState, user: &CurrentUser, upload: &UploadRow) -> AppResult<Uuid> {
    let parent_id = upload.parent_id.ok_or_else(|| AppError::bad("upload has no folder"))?;
    let tmp = state.storage.upload_file(upload.id);
    let hash = blake3_file(&tmp).await?;
    let name = upload.filename.as_deref().unwrap_or("upload");
    let node_id = place_file(state, user, parent_id, name, &tmp, upload.length, &hash).await?;
    sqlx::query("UPDATE uploads SET completed_at = now(), node_id = $1 WHERE id = $2")
        .bind(node_id)
        .bind(upload.id)
        .execute(&state.db)
        .await?;
    Ok(node_id)
}

/// Moves a fully received file from `tmp` into `parent_id` under a free name
/// based on `filename`, and records it. Shared by every upload protocol.
pub(crate) async fn place_file(
    state: &AppState,
    user: &CurrentUser,
    parent_id: Uuid,
    filename: &str,
    tmp: &FsPath,
    length: i64,
    hash: &str,
) -> AppResult<Uuid> {
    let parent = target_folder(state, user, &parent_id.to_string()).await?;
    // Checked again here, not only when the upload started: several uploads
    // started together would otherwise all fit and together overflow it.
    check_quota(state, &parent, length.max(0) as u64).await?;
    // A name like "../../x.pdf" keeps only its last part; never a path.
    let last = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let wanted = fennoly_core::validate_name(last).unwrap_or_else(|_| "upload".to_string());
    let mime = mime_guess::from_path(&wanted).first_or_octet_stream().essence_str().to_string();

    let mut attempt = 0;
    let node_id = loop {
        attempt += 1;
        let name = access::free_name(state, parent_id, &wanted).await?;
        let dest = parent.path.join(&name);
        let mut tx = state.db.begin().await?;
        let inserted: Result<Uuid, sqlx::Error> = sqlx::query_scalar(
            "INSERT INTO nodes (space_id, parent_id, name, kind, size, mime, content_hash, created_by)
             VALUES ($1, $2, $3, 'file', $4, $5, $6, $7) RETURNING id",
        )
        .bind(parent.node.space_id)
        .bind(parent_id)
        .bind(&name)
        .bind(length)
        .bind(&mime)
        .bind(hash)
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await;
        match inserted {
            Ok(id) => {
                move_path(tmp, &dest).await?;
                if let Err(e) = tx.commit().await {
                    let _ = move_path(&dest, tmp).await;
                    return Err(e.into());
                }
                break id;
            }
            // Another upload took the name a moment ago; pick the next free one.
            Err(sqlx::Error::Database(db)) if db.code().as_deref() == Some("23505") && attempt < 5 => continue,
            Err(e) => return Err(e.into()),
        }
    };
    sqlx::query("UPDATE nodes SET modified_at = now() WHERE id = $1")
        .bind(parent_id)
        .execute(&state.db)
        .await?;
    Ok(node_id)
}

/// Appends a request body to an upload's file. Saves the new offset even
/// if the connection drops part way, so the client can resume.
async fn append(state: &AppState, upload: &UploadRow, body: Body) -> AppResult<i64> {
    let path = state.storage.upload_file(upload.id);
    let mut file = tokio::fs::OpenOptions::new().append(true).open(&path).await?;
    let remaining = (upload.length - upload.upload_offset) as u64;
    let mut written: u64 = 0;
    let mut stream = body.into_data_stream();
    let mut failure: Option<AppError> = None;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                if written + bytes.len() as u64 > remaining {
                    failure = Some(AppError::bad("more data than Upload-Length"));
                    break;
                }
                if let Err(e) = file.write_all(&bytes).await {
                    failure = Some(e.into());
                    break;
                }
                written += bytes.len() as u64;
            }
            Err(_) => {
                failure = Some(AppError::bad("connection interrupted"));
                break;
            }
        }
    }
    file.flush().await?;
    file.sync_data().await?;
    drop(file);
    let new_offset = upload.upload_offset + written as i64;
    sqlx::query("UPDATE uploads SET upload_offset = $1 WHERE id = $2")
        .bind(new_offset)
        .bind(upload.id)
        .execute(&state.db)
        .await?;
    match failure {
        Some(e) => Err(e),
        None => Ok(new_offset),
    }
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    body: Body,
) -> AppResult<Response> {
    let metadata = match hdr(&headers, tus::UPLOAD_METADATA) {
        Some(raw) => fennoly_tus::parse_metadata(&raw).map_err(|e| AppError::bad(e.to_string()))?,
        None => Default::default(),
    };
    let concat = hdr(&headers, tus::UPLOAD_CONCAT)
        .map(|raw| fennoly_tus::parse_concat(&raw))
        .transpose()
        .map_err(|e| AppError::bad(e.to_string()))?;

    if let Some(Concat::Final(parts)) = concat {
        return create_final(&state, &user, &metadata, parts).await;
    }
    if hdr(&headers, tus::UPLOAD_DEFER_LENGTH).is_some() {
        return Err(AppError::bad("Upload-Defer-Length is not supported"));
    }
    let length = hdr(&headers, tus::UPLOAD_LENGTH)
        .ok_or_else(|| AppError::bad("Upload-Length is required"))
        .and_then(|v| fennoly_tus::parse_u64(&v).map_err(|e| AppError::bad(e.to_string())))?;
    check_size(&state, length)?;
    let partial = concat == Some(Concat::Partial);

    let (parent_id, filename) = if partial {
        (None, None)
    } else {
        let parent_raw = metadata.get("parentId").ok_or_else(|| AppError::bad("parentId metadata is required"))?;
        let parent = target_folder(&state, &user, parent_raw).await?;
        check_quota(&state, &parent, length).await?;
        let filename = metadata
            .get("filename")
            .or_else(|| metadata.get("name"))
            .cloned()
            .ok_or_else(|| AppError::bad("filename metadata is required"))?;
        (Some(parent.node.id), Some(filename))
    };

    let id = Uuid::new_v4();
    tokio::fs::File::create(state.storage.upload_file(id)).await?;
    let mut upload: UploadRow = sqlx::query_as(&format!(
        "INSERT INTO uploads (id, user_id, parent_id, filename, length, is_partial)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {UPLOAD_COLS}"
    ))
    .bind(id)
    .bind(user.id)
    .bind(parent_id)
    .bind(&filename)
    .bind(length as i64)
    .bind(partial)
    .fetch_one(&state.db)
    .await?;

    // creation-with-upload: the first bytes may arrive with the POST.
    let has_body = hdr(&headers, header::CONTENT_TYPE.as_str()).as_deref() == Some(fennoly_tus::OFFSET_CONTENT_TYPE);
    if has_body {
        upload.upload_offset = append(&state, &upload, body).await?;
    }

    let mut extra = vec![
        ("location", format!("/api/uploads/{id}")),
        (tus::UPLOAD_OFFSET, upload.upload_offset.to_string()),
    ];
    if !partial && upload.upload_offset == upload.length {
        let node_id = finalize(&state, &user, &upload).await?;
        extra.push((NODE_ID_HEADER, node_id.to_string()));
    }
    Ok(tus_response(StatusCode::CREATED, &extra))
}

async fn create_final(
    state: &AppState,
    user: &CurrentUser,
    metadata: &std::collections::HashMap<String, String>,
    parts: Vec<Uuid>,
) -> AppResult<Response> {
    let parent_raw = metadata.get("parentId").ok_or_else(|| AppError::bad("parentId metadata is required"))?;
    let parent = target_folder(state, user, parent_raw).await?;
    let filename = metadata
        .get("filename")
        .or_else(|| metadata.get("name"))
        .cloned()
        .ok_or_else(|| AppError::bad("filename metadata is required"))?;

    let rows: Vec<UploadRow> = sqlx::query_as(&format!(
        "SELECT {UPLOAD_COLS} FROM uploads WHERE id = ANY($1) AND user_id = $2"
    ))
    .bind(&parts)
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    let mut ordered = Vec::with_capacity(parts.len());
    for id in &parts {
        let row = rows.iter().find(|r| r.id == *id).ok_or(AppError::NotFound)?;
        if !row.is_partial || row.upload_offset != row.length {
            return Err(AppError::bad("all parts must be finished partial uploads"));
        }
        ordered.push(row.clone());
    }
    let total: u64 = ordered.iter().map(|r| r.length as u64).sum();
    check_size(state, total)?;
    check_quota(state, &parent, total).await?;

    // Join the parts into one file.
    let id = Uuid::new_v4();
    let dest_path = state.storage.upload_file(id);
    let mut dest = tokio::fs::File::create(&dest_path).await?;
    for part in &ordered {
        let mut src = tokio::fs::File::open(state.storage.upload_file(part.id)).await?;
        tokio::io::copy(&mut src, &mut dest).await?;
    }
    dest.flush().await?;
    dest.sync_data().await?;
    drop(dest);

    let upload: UploadRow = sqlx::query_as(&format!(
        "INSERT INTO uploads (id, user_id, parent_id, filename, length, upload_offset, is_partial)
         VALUES ($1, $2, $3, $4, $5, $5, FALSE) RETURNING {UPLOAD_COLS}"
    ))
    .bind(id)
    .bind(user.id)
    .bind(parent.node.id)
    .bind(&filename)
    .bind(total as i64)
    .fetch_one(&state.db)
    .await?;

    for part in &ordered {
        remove_path(&state.storage.upload_file(part.id)).await?;
    }
    sqlx::query("DELETE FROM uploads WHERE id = ANY($1)").bind(&parts).execute(&state.db).await?;

    let node_id = finalize(state, user, &upload).await?;
    Ok(tus_response(
        StatusCode::CREATED,
        &[
            ("location", format!("/api/uploads/{id}")),
            (tus::UPLOAD_OFFSET, total.to_string()),
            (NODE_ID_HEADER, node_id.to_string()),
        ],
    ))
}

pub async fn head(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Response> {
    let upload = load(&state, &user, id).await?;
    let mut extra = vec![
        (tus::UPLOAD_OFFSET, upload.upload_offset.to_string()),
        (tus::UPLOAD_LENGTH, upload.length.to_string()),
    ];
    if upload.is_partial {
        extra.push((tus::UPLOAD_CONCAT, "partial".into()));
    }
    Ok(tus_response(StatusCode::OK, &extra))
}

pub async fn patch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: Body,
) -> AppResult<Response> {
    if hdr(&headers, header::CONTENT_TYPE.as_str()).as_deref() != Some(fennoly_tus::OFFSET_CONTENT_TYPE) {
        return Ok(tus_response(StatusCode::UNSUPPORTED_MEDIA_TYPE, &[]));
    }
    let offset = hdr(&headers, tus::UPLOAD_OFFSET)
        .ok_or_else(|| AppError::bad("Upload-Offset is required"))
        .and_then(|v| fennoly_tus::parse_u64(&v).map_err(|e| AppError::bad(e.to_string())))?;

    let lock = state.upload_lock(id);
    let _guard = lock.lock().await;
    let mut upload = load(&state, &user, id).await?;
    if upload.completed_at.is_some() {
        let mut extra = vec![(tus::UPLOAD_OFFSET, upload.length.to_string())];
        if let Some(node) = upload.node_id {
            extra.push((NODE_ID_HEADER, node.to_string()));
        }
        return Ok(tus_response(StatusCode::NO_CONTENT, &extra));
    }
    if offset as i64 != upload.upload_offset {
        return Ok(tus_response(
            StatusCode::CONFLICT,
            &[(tus::UPLOAD_OFFSET, upload.upload_offset.to_string())],
        ));
    }
    upload.upload_offset = append(&state, &upload, body).await?;

    let mut extra = vec![(tus::UPLOAD_OFFSET, upload.upload_offset.to_string())];
    if !upload.is_partial && upload.upload_offset == upload.length {
        let node_id = finalize(&state, &user, &upload).await?;
        extra.push((NODE_ID_HEADER, node_id.to_string()));
        state.drop_upload_lock(id);
    }
    Ok(tus_response(StatusCode::NO_CONTENT, &extra))
}

pub async fn terminate(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Response> {
    let upload = load(&state, &user, id).await?;
    remove_path(&state.storage.upload_file(upload.id)).await?;
    sqlx::query("DELETE FROM uploads WHERE id = $1").bind(id).execute(&state.db).await?;
    state.drop_upload_lock(id);
    Ok(tus_response(StatusCode::NO_CONTENT, &[]))
}
