//! Public links: share a file or folder with anyone who has the URL,
//! optionally with a password, an expiry date and download turned off.
//!
//! Management (`/api/nodes/{id}/links`, `/api/links/{id}`) needs a signed-in
//! user who may link the item. Public access (`/api/public/{token}/...`)
//! needs no account; a password-protected link is unlocked once, which sets
//! a cookie scoped to that link's URL.

use axum::Json;
use axum::extract::{Path, Query, Request, State};
use axum::response::Response;
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use super::content::{self, ContentQuery, ZipQuery};
use crate::access::{self, Resolved};
use crate::auth::{self, CurrentUser};
use crate::error::{AppError, AppResult};
use crate::model::has_thumbnail;
use crate::state::AppState;

const LINK_COOKIE: &str = "fennoly_link";

#[derive(Debug, Clone, FromRow)]
pub struct LinkRow {
    pub id: Uuid,
    pub node_id: Uuid,
    pub token: String,
    pub created_by: Option<Uuid>,
    pub allow_download: bool,
    pub password_hash: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub view_count: i64,
    pub download_count: i64,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

const LINK_COLS: &str = "id, node_id, token, created_by, allow_download, password_hash, expires_at, \
     view_count, download_count, last_accessed_at, created_at";

#[derive(Debug, Clone, Serialize)]
pub struct LinkDto {
    pub id: Uuid,
    pub node_id: Uuid,
    pub token: String,
    pub allow_download: bool,
    pub has_password: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub expired: bool,
    pub view_count: i64,
    pub download_count: i64,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<LinkRow> for LinkDto {
    fn from(l: LinkRow) -> Self {
        LinkDto {
            expired: l.expires_at.is_some_and(|e| e <= Utc::now()),
            id: l.id,
            node_id: l.node_id,
            token: l.token,
            allow_download: l.allow_download,
            has_password: l.password_hash.is_some(),
            expires_at: l.expires_at,
            view_count: l.view_count,
            download_count: l.download_count,
            last_accessed_at: l.last_accessed_at,
            created_at: l.created_at,
        }
    }
}

pub async fn links_of(state: &AppState, node_id: Uuid) -> AppResult<Vec<LinkDto>> {
    let rows: Vec<LinkRow> = sqlx::query_as(&format!(
        "SELECT {LINK_COLS} FROM links WHERE node_id = $1 ORDER BY created_at DESC"
    ))
    .bind(node_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

async fn linkable(state: &AppState, user: &CurrentUser, node_id: Uuid) -> AppResult<Resolved> {
    let r = access::resolve(state, user.id, node_id).await?;
    if !access::can_link(&r) {
        return Err(AppError::Forbidden);
    }
    Ok(r)
}

// ---- management -----------------------------------------------------------

pub async fn list(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Vec<LinkDto>>> {
    linkable(&state, &user, id).await?;
    Ok(Json(links_of(&state, id).await?))
}

#[derive(Deserialize)]
pub struct CreateLink {
    #[serde(default = "yes")]
    allow_download: bool,
    password: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

fn yes() -> bool {
    true
}

async fn hash_link_password(password: Option<String>) -> AppResult<Option<String>> {
    match password.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
        Some(p) if p.chars().count() < 4 => Err(AppError::bad("link password must be at least 4 characters")),
        Some(p) => Ok(Some(auth::hash_password(p).await?)),
        None => Ok(None),
    }
}

fn check_expiry(expires_at: Option<DateTime<Utc>>) -> AppResult<()> {
    if expires_at.is_some_and(|e| e <= Utc::now()) {
        return Err(AppError::bad("the expiry date must be in the future"));
    }
    Ok(())
}

pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateLink>,
) -> AppResult<Json<LinkDto>> {
    linkable(&state, &user, id).await?;
    check_expiry(req.expires_at)?;
    let password_hash = hash_link_password(req.password).await?;
    let mut bytes = [0u8; 18];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let row: LinkRow = sqlx::query_as(&format!(
        "INSERT INTO links (node_id, token, created_by, allow_download, password_hash, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {LINK_COLS}"
    ))
    .bind(id)
    .bind(&token)
    .bind(user.id)
    .bind(req.allow_download)
    .bind(password_hash)
    .bind(req.expires_at)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row.into()))
}

#[derive(Deserialize)]
pub struct UpdateLink {
    allow_download: Option<bool>,
    /// A new password; an empty string removes the password.
    password: Option<String>,
    /// `null` removes the expiry.
    #[serde(default, with = "double_option")]
    expires_at: Option<Option<DateTime<Utc>>>,
}

mod double_option {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<DateTime<Utc>>>, D::Error> {
        Ok(Some(Option::deserialize(d)?))
    }
}

async fn managed_link(state: &AppState, user: &CurrentUser, link_id: Uuid) -> AppResult<LinkRow> {
    let link: LinkRow = sqlx::query_as(&format!("SELECT {LINK_COLS} FROM links WHERE id = $1"))
        .bind(link_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    // The creator can always manage their link; otherwise you must be able to link the item.
    if link.created_by != Some(user.id) {
        linkable(state, user, link.node_id).await?;
    }
    Ok(link)
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(link_id): Path<Uuid>,
    Json(req): Json<UpdateLink>,
) -> AppResult<Json<LinkDto>> {
    let link = managed_link(&state, &user, link_id).await?;
    let allow_download = req.allow_download.unwrap_or(link.allow_download);
    let password_hash = match req.password {
        Some(p) => hash_link_password(Some(p)).await?,
        None => link.password_hash,
    };
    let expires_at = match req.expires_at {
        Some(e) => {
            check_expiry(e)?;
            e
        }
        None => link.expires_at,
    };
    let row: LinkRow = sqlx::query_as(&format!(
        "UPDATE links SET allow_download = $1, password_hash = $2, expires_at = $3
         WHERE id = $4 RETURNING {LINK_COLS}"
    ))
    .bind(allow_download)
    .bind(password_hash)
    .bind(expires_at)
    .bind(link_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row.into()))
}

pub async fn remove(State(state): State<AppState>, user: CurrentUser, Path(link_id): Path<Uuid>) -> AppResult<Json<Value>> {
    managed_link(&state, &user, link_id).await?;
    sqlx::query("DELETE FROM links WHERE id = $1").bind(link_id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---- public access ---------------------------------------------------------

/// Proof that this browser knows the link's password. It changes when the
/// password changes, so old unlocks stop working.
fn grant_for(link: &LinkRow) -> Option<String> {
    link.password_hash.as_ref().map(|hash| {
        let mut h = blake3::Hasher::new();
        h.update(link.token.as_bytes());
        h.update(b"|");
        h.update(hash.as_bytes());
        h.finalize().to_hex().to_string()
    })
}

async fn find_link(state: &AppState, token: &str) -> AppResult<LinkRow> {
    let link: LinkRow = sqlx::query_as(&format!("SELECT {LINK_COLS} FROM links WHERE token = $1"))
        .bind(token)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if link.expires_at.is_some_and(|e| e <= Utc::now()) {
        return Err(AppError::Expired);
    }
    Ok(link)
}

/// Finds a link and checks that it is unlocked for this browser.
async fn open_link(state: &AppState, token: &str, jar: &CookieJar) -> AppResult<LinkRow> {
    let link = find_link(state, token).await?;
    if let Some(grant) = grant_for(&link) {
        let ok = jar.get(LINK_COOKIE).is_some_and(|c| c.value() == grant);
        if !ok {
            return Err(AppError::PasswordRequired);
        }
    }
    Ok(link)
}

#[derive(Serialize)]
pub struct PublicNode {
    id: Uuid,
    name: String,
    kind: String,
    size: i64,
    mime: Option<String>,
    modified_at: DateTime<Utc>,
    thumbnail: bool,
}

impl From<&Resolved> for PublicNode {
    fn from(r: &Resolved) -> Self {
        PublicNode {
            id: r.node.id,
            name: r.node.name.clone(),
            kind: r.node.kind.clone(),
            size: r.node.size,
            mime: r.node.mime.clone(),
            modified_at: r.node.modified_at,
            thumbnail: has_thumbnail(&r.node),
        }
    }
}

#[derive(Serialize)]
pub struct PublicInfo {
    root: PublicNode,
    allow_download: bool,
    shared_by: String,
    expires_at: Option<DateTime<Utc>>,
}

pub async fn info(State(state): State<AppState>, jar: CookieJar, Path(token): Path<String>) -> AppResult<Json<PublicInfo>> {
    let link = open_link(&state, &token, &jar).await?;
    let root = access::resolve_in_link(&state, link.node_id, link.node_id).await?;
    let shared_by: Option<String> = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(link.created_by)
        .fetch_optional(&state.db)
        .await?;
    sqlx::query("UPDATE links SET view_count = view_count + 1, last_accessed_at = now() WHERE id = $1")
        .bind(link.id)
        .execute(&state.db)
        .await?;
    Ok(Json(PublicInfo {
        root: (&root).into(),
        allow_download: link.allow_download,
        shared_by: shared_by.unwrap_or_default(),
        expires_at: link.expires_at,
    }))
}

#[derive(Deserialize)]
pub struct Unlock {
    password: String,
}

pub async fn unlock(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token): Path<String>,
    Json(req): Json<Unlock>,
) -> AppResult<(CookieJar, Json<Value>)> {
    let key = format!("link:{token}");
    if state.login_locked(&key) {
        return Err(AppError::TooManyRequests);
    }
    let link = find_link(&state, &token).await?;
    let (Some(hash), Some(grant)) = (link.password_hash.clone(), grant_for(&link)) else {
        return Ok((jar, Json(json!({ "ok": true }))));
    };
    if !auth::verify_password(req.password, hash).await? {
        state.record_login_failure(&key);
        return Err(AppError::BadRequest("wrong password".into()));
    }
    state.clear_login_failures(&key);
    let cookie = Cookie::build((LINK_COOKIE, grant))
        .path(format!("/api/public/{token}"))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.cfg.secure_cookies)
        .max_age(time::Duration::days(1))
        .build();
    Ok((jar.add(cookie), Json(json!({ "ok": true }))))
}

#[derive(Serialize)]
pub struct PublicCrumb {
    id: Uuid,
    name: String,
}

#[derive(Serialize)]
pub struct PublicFolder {
    node: PublicNode,
    crumbs: Vec<PublicCrumb>,
    children: Vec<PublicNode>,
}

pub async fn folder(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((token, id)): Path<(String, Uuid)>,
) -> AppResult<Json<PublicFolder>> {
    let link = open_link(&state, &token, &jar).await?;
    let r = access::resolve_in_link(&state, link.node_id, id).await?;
    if !r.node.is_folder() {
        return Err(AppError::bad("not a folder"));
    }
    // Trail from the shared folder down to this one; never above it.
    let top = r.chain.iter().position(|c| c.id == link.node_id).unwrap_or(0);
    let crumbs = r.chain[..=top]
        .iter()
        .rev()
        .map(|c| PublicCrumb { id: c.id, name: c.name.clone() })
        .collect();
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM nodes WHERE parent_id = $1 AND trashed_at IS NULL
         ORDER BY (kind = 'folder') DESC, lower(name)",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let mut children = Vec::with_capacity(ids.len());
    for child in ids {
        let c = access::resolve_in_link(&state, link.node_id, child).await?;
        children.push(PublicNode::from(&c));
    }
    Ok(Json(PublicFolder { node: (&r).into(), crumbs, children }))
}

pub async fn file_content(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((token, id)): Path<(String, Uuid)>,
    Query(q): Query<ContentQuery>,
    req: Request,
) -> AppResult<Response> {
    let link = open_link(&state, &token, &jar).await?;
    let download = q.download.is_some();
    if download && !link.allow_download {
        return Err(AppError::Forbidden);
    }
    let r = access::resolve_in_link(&state, link.node_id, id).await?;
    if download {
        sqlx::query("UPDATE links SET download_count = download_count + 1, last_accessed_at = now() WHERE id = $1")
            .bind(link.id)
            .execute(&state.db)
            .await?;
    }
    content::serve(&r, req, download).await
}

pub async fn file_thumbnail(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((token, id)): Path<(String, Uuid)>,
) -> AppResult<Response> {
    let link = open_link(&state, &token, &jar).await?;
    let r = access::resolve_in_link(&state, link.node_id, id).await?;
    content::thumbnail_of(&state, &r).await
}

pub async fn zip(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token): Path<String>,
    Query(q): Query<ZipQuery>,
) -> AppResult<Response> {
    let link = open_link(&state, &token, &jar).await?;
    if !link.allow_download {
        return Err(AppError::Forbidden);
    }
    let mut items = Vec::new();
    for id in content::parse_ids(&q.ids)? {
        let r = access::resolve_in_link(&state, link.node_id, id).await?;
        items.push((r.path, r.node.name));
    }
    sqlx::query("UPDATE links SET download_count = download_count + 1, last_accessed_at = now() WHERE id = $1")
        .bind(link.id)
        .execute(&state.db)
        .await?;
    content::zip_response(&state, items).await
}
