//! Compress tools. The web app calls `preview` while the user moves the
//! slider (it returns the real resulting size), then `apply` to keep it.

use axum::Json;
use axum::extract::State;
use fennoly_tools::image_tools::{self, CompressOptions};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::access;
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::model::NodeDto;
use crate::state::AppState;
use crate::storage::{move_path, remove_path};

pub async fn capabilities(_user: CurrentUser) -> Json<Value> {
    let caps = fennoly_tools::capabilities();
    Json(json!({ "image": caps.image, "pdf": caps.pdf }))
}

#[derive(Deserialize)]
pub struct PreviewRequest {
    node_id: Uuid,
    /// `image` or `pdf`
    tool: String,
    /// Image JPEG quality, 1-100.
    quality: Option<u8>,
    /// Image longest edge in pixels.
    max_dimension: Option<u32>,
    /// PDF strength, 0 (light) to 100 (smallest).
    strength: Option<u8>,
}

#[derive(Serialize)]
pub struct PreviewResponse {
    original_size: i64,
    result_size: u64,
    token: String,
}

const MAX_IMAGE_INPUT: i64 = 100 * 1024 * 1024;

pub async fn preview(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<PreviewRequest>,
) -> AppResult<Json<PreviewResponse>> {
    let r = access::resolve(&state, user.id, req.node_id).await?;
    if r.node.is_folder() {
        return Err(AppError::bad("choose a file"));
    }
    let mime = r.node.mime.clone().unwrap_or_default();
    let (ext, params) = match req.tool.as_str() {
        "image" => {
            if !image_tools::is_supported(&mime) {
                return Err(AppError::bad("this image type cannot be compressed"));
            }
            if r.node.size > MAX_IMAGE_INPUT {
                return Err(AppError::TooLarge("image is too large to compress".into()));
            }
            let q = req.quality.unwrap_or(75).clamp(1, 100);
            let max = req.max_dimension.filter(|m| *m >= 16);
            ("jpg", format!("q{q}-m{}", max.unwrap_or(0)))
        }
        "pdf" => {
            if mime != "application/pdf" {
                return Err(AppError::bad("this is not a PDF"));
            }
            let s = req.strength.unwrap_or(50).min(100);
            ("pdf", format!("s{s}"))
        }
        _ => return Err(AppError::bad("unknown tool")),
    };
    let key = blake3::hash(format!("{}|{}|{}", r.node.id, r.node.modified_at.timestamp(), params).as_bytes())
        .to_hex()[..24]
        .to_string();
    let token = format!("{}_{key}", r.node.id);
    let out = state.storage.tool_output(&token, ext);

    if tokio::fs::metadata(&out).await.is_err() {
        let input = r.path.clone();
        let tmp_out = out.with_extension(format!("{ext}.part"));
        match req.tool.as_str() {
            "image" => {
                let opts = CompressOptions {
                    quality: req.quality.unwrap_or(75).clamp(1, 100),
                    max_dimension: req.max_dimension.filter(|m| *m >= 16),
                };
                let bytes = tokio::fs::read(&input).await?;
                let result = tokio::task::spawn_blocking(move || image_tools::compress(&bytes, opts)).await??;
                tokio::fs::write(&tmp_out, result).await?;
            }
            _ => {
                let strength = req.strength.unwrap_or(50).min(100);
                let target = tmp_out.clone();
                tokio::task::spawn_blocking(move || fennoly_tools::pdf::compress(&input, &target, strength))
                    .await??;
            }
        }
        tokio::fs::rename(&tmp_out, &out).await?;
    }
    let result_size = tokio::fs::metadata(&out).await?.len();
    Ok(Json(PreviewResponse { original_size: r.node.size, result_size, token: format!("{token}.{ext}") }))
}

#[derive(Deserialize)]
pub struct ApplyRequest {
    node_id: Uuid,
    token: String,
    /// `copy` keeps the original and adds a new file; `replace` overwrites it.
    mode: String,
}

pub async fn apply(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<ApplyRequest>,
) -> AppResult<Json<NodeDto>> {
    let r = access::resolve(&state, user.id, req.node_id).await?;
    // A token is "<node id>_<hex key>.<jpg|pdf>", the name of a preview file.
    let valid_token = match req.token.split_once('.') {
        Some((stem, ext)) => {
            stem.starts_with(&format!("{}_", r.node.id))
                && stem.chars().all(|c| c.is_ascii_hexdigit() || matches!(c, '-' | '_'))
                && matches!(ext, "jpg" | "pdf")
        }
        None => false,
    };
    if !valid_token {
        return Err(AppError::bad("invalid token"));
    }
    let cached = state.storage.tools_dir().join(&req.token);
    if tokio::fs::metadata(&cached).await.is_err() {
        return Err(AppError::bad("the preview expired, please try again"));
    }
    let ext = req.token.rsplit('.').next().unwrap_or("bin");
    let (stem, old_ext) = fennoly_core::names::split_extension(&r.node.name);
    let same_ext = old_ext.trim_start_matches('.').eq_ignore_ascii_case(ext)
        || (ext == "jpg" && old_ext.eq_ignore_ascii_case(".jpeg"));
    let parent_id = r.node.parent_id.ok_or(AppError::NotFound)?;
    let parent = access::resolve(&state, user.id, parent_id).await?;
    let hash = blake3::hash(&tokio::fs::read(&cached).await?).to_hex().to_string();
    let size = tokio::fs::metadata(&cached).await?.len() as i64;

    let id = match req.mode.as_str() {
        "copy" => {
            parent.require_write()?;
            let new_ext = if same_ext { old_ext.to_string() } else { format!(".{ext}") };
            let wanted = format!("{stem} (compressed){new_ext}");
            let name = access::free_name(&state, parent_id, &wanted).await?;
            let mime = mime_guess::from_path(&name).first_or_octet_stream().essence_str().to_string();
            let mut tx = state.db.begin().await?;
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO nodes (space_id, parent_id, name, kind, size, mime, content_hash, created_by)
                 VALUES ($1, $2, $3, 'file', $4, $5, $6, $7) RETURNING id",
            )
            .bind(r.node.space_id)
            .bind(parent_id)
            .bind(&name)
            .bind(size)
            .bind(&mime)
            .bind(&hash)
            .bind(user.id)
            .fetch_one(&mut *tx)
            .await?;
            move_path(&cached, &parent.path.join(&name)).await?;
            tx.commit().await?;
            id
        }
        "replace" => {
            r.require_write()?;
            let name = if same_ext {
                r.node.name.clone()
            } else {
                access::free_name(&state, parent_id, &format!("{stem}.{ext}")).await?
            };
            let dest = parent.path.join(&name);
            let mime = mime_guess::from_path(&name).first_or_octet_stream().essence_str().to_string();
            move_path(&cached, &dest).await?;
            if dest != r.path {
                remove_path(&r.path).await?;
            }
            sqlx::query(
                "UPDATE nodes SET name = $1, size = $2, mime = $3, content_hash = $4, modified_at = now()
                 WHERE id = $5",
            )
            .bind(&name)
            .bind(size)
            .bind(&mime)
            .bind(&hash)
            .bind(r.node.id)
            .execute(&state.db)
            .await?;
            r.node.id
        }
        _ => return Err(AppError::bad("mode must be copy or replace")),
    };
    Ok(Json(super::nodes::view(&state, user.id, id).await?))
}
