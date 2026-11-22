//! Serving file bytes: downloads and previews (with HTTP range support),
//! thumbnails, and zip downloads of several items.

use std::io::{Seek, SeekFrom, Write};
use std::path::{Path as FsPath, PathBuf};

use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio_util::io::ReaderStream;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::access::{self, Resolved};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::model::has_thumbnail;
use crate::state::AppState;

/// Only these are ever shown inline; everything else is downloaded. An
/// allow-list, because any HTML- or XML-like type (`*+xml`, XSLT, …) can
/// carry script.
fn safe_inline(mime: &str) -> bool {
    (mime.starts_with("image/") && mime != "image/svg+xml")
        || mime.starts_with("video/")
        || mime.starts_with("audio/")
        || matches!(mime, "application/pdf" | "text/plain")
}

fn encode_rfc5987(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn content_disposition(kind: &str, name: &str) -> HeaderValue {
    let ascii: String = name
        .chars()
        .map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' })
        .collect();
    let value = format!("{kind}; filename=\"{ascii}\"; filename*=UTF-8''{}", encode_rfc5987(name));
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

#[derive(Deserialize)]
pub struct ContentQuery {
    #[serde(default)]
    pub download: Option<String>,
}

pub async fn content(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ContentQuery>,
    req: Request,
) -> AppResult<Response> {
    let r = access::resolve(&state, user.id, id).await?;
    serve(&r, req, q.download.is_some()).await
}

/// Serves a file's bytes with range support and safe headers.
pub async fn serve(r: &Resolved, req: Request, download: bool) -> AppResult<Response> {
    if r.node.is_folder() {
        return Err(AppError::bad("folders are downloaded as zip"));
    }
    let mime = r.node.mime.clone().unwrap_or_else(|| "application/octet-stream".into());
    let risky = !safe_inline(&mime);
    let download = download || risky;

    let res = ServeFile::new(&r.path)
        .oneshot(req)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let mut res = res.map(Body::new);
    let headers = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&mime) {
        if !risky {
            headers.insert(header::CONTENT_TYPE, v);
        }
    }
    let kind = if download { "attachment" } else { "inline" };
    headers.insert(header::CONTENT_DISPOSITION, content_disposition(kind, &r.node.name));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    // PDFs need the browser's own viewer, which refuses sandboxed documents.
    if mime != "application/pdf" {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox; default-src 'none'; img-src 'self' data:; media-src 'self'; style-src 'unsafe-inline'"),
        );
    }
    Ok(res)
}

const MAX_THUMB_SOURCE: u64 = 64 * 1024 * 1024;

pub async fn thumbnail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Response> {
    let r = access::resolve(&state, user.id, id).await?;
    thumbnail_of(&state, &r).await
}

/// Image decoding and PDF compression use a lot of memory and CPU; a folder
/// of 200 photos must not start 200 of them at once.
pub static HEAVY_WORK: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(3);

/// A cached JPEG thumbnail for an image node.
pub async fn thumbnail_of(state: &AppState, r: &Resolved) -> AppResult<Response> {
    let id = r.node.id;
    if !has_thumbnail(&r.node) || r.node.size as u64 > MAX_THUMB_SOURCE {
        return Err(AppError::NotFound);
    }
    let cached = state.storage.thumb(id, r.node.modified_at.timestamp());
    let bytes = match tokio::fs::read(&cached).await {
        Ok(b) => b,
        Err(_) => {
            let _slot = HEAVY_WORK.acquire().await.map_err(|e| AppError::Internal(e.into()))?;
            let source = tokio::fs::read(&r.path).await?;
            let thumb = tokio::task::spawn_blocking(move || fennoly_tools::image_tools::thumbnail(&source, 360))
                .await??;
            let _ = tokio::fs::write(&cached, &thumb).await;
            thumb
        }
    };
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg")),
            (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400")),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct ZipQuery {
    pub ids: String,
}

/// File types that are already compressed; storing them is faster and
/// the zip comes out the same size.
fn already_compressed(path: &FsPath) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "avif" | "mp4" | "mov" | "mkv" | "webm"
            | "mp3" | "m4a" | "aac" | "ogg" | "zip" | "gz" | "7z" | "rar" | "pdf" | "docx" | "xlsx" | "pptx"
    )
}

fn add_to_zip(zip: &mut zip::ZipWriter<std::fs::File>, path: &FsPath, name: &str) -> anyhow::Result<()> {
    let meta = std::fs::metadata(path)?;
    if meta.is_dir() {
        zip.add_directory(format!("{name}/"), SimpleFileOptions::default())?;
        let mut entries: Vec<_> = std::fs::read_dir(path)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let child = entry.file_name().to_string_lossy().to_string();
            add_to_zip(zip, &entry.path(), &format!("{name}/{child}"))?;
        }
    } else {
        let method = if already_compressed(path) { CompressionMethod::Stored } else { CompressionMethod::Deflated };
        let opts = SimpleFileOptions::default()
            .compression_method(method)
            .large_file(meta.len() >= u32::MAX as u64);
        zip.start_file(name, opts)?;
        let mut f = std::fs::File::open(path)?;
        std::io::copy(&mut f, zip)?;
    }
    Ok(())
}

pub async fn zip(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ZipQuery>) -> AppResult<Response> {
    let ids = parse_ids(&q.ids)?;
    let mut items: Vec<(PathBuf, String)> = Vec::new();
    for id in &ids {
        let r = access::resolve(&state, user.id, *id).await?;
        items.push((r.path, r.node.name));
    }
    zip_response(&state, items).await
}

/// Parses a comma-separated list of ids (1 to 500).
pub fn parse_ids(raw: &str) -> AppResult<Vec<Uuid>> {
    let ids: Vec<Uuid> = raw
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().map_err(|_| AppError::bad("bad id")))
        .collect::<Result<_, _>>()?;
    if ids.is_empty() || ids.len() > 500 {
        return Err(AppError::bad("choose between 1 and 500 items"));
    }
    Ok(ids)
}

/// Builds a zip of files and folders and streams it.
pub async fn zip_response(state: &AppState, items: Vec<(PathBuf, String)>) -> AppResult<Response> {
    let filename = match items.as_slice() {
        [(path, name)] if path.is_dir() => format!("{name}.zip"),
        _ => format!("fennoly-{}.zip", chrono::Local::now().format("%Y-%m-%d")),
    };

    let tmp_dir = state.storage.tmp_dir();
    let file = tokio::task::spawn_blocking(move || -> anyhow::Result<std::fs::File> {
        let file = tempfile::tempfile_in(tmp_dir)?;
        let mut zip = zip::ZipWriter::new(file);
        let mut used = std::collections::HashSet::new();
        for (path, name) in items {
            let name = fennoly_core::unique_name(&name, |n| used.contains(&n.to_lowercase()));
            used.insert(name.to_lowercase());
            add_to_zip(&mut zip, &path, &name)?;
        }
        let mut file = zip.finish()?;
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        Ok(file)
    })
    .await?
    .map_err(AppError::Internal)?;

    let len = file.metadata()?.len();
    let stream = ReaderStream::new(tokio::fs::File::from_std(file));
    let mut res = Response::new(Body::from_stream(stream));
    let headers = res.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    headers.insert(header::CONTENT_DISPOSITION, content_disposition("attachment", &filename));
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition_handles_unicode() {
        let v = content_disposition("inline", "आधार \"card\".pdf");
        let s = v.to_str().unwrap();
        assert!(s.starts_with("inline; filename=\""));
        assert!(s.contains("filename*=UTF-8''%E0%A4%86"));
        assert!(!s.contains("\"card\""));
    }
}
