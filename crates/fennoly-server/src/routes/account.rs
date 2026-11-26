//! Setup wizard, sign in and out, the user's own profile, and the user
//! directory used by the share dialog.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::USER_AGENT;
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::{self, CurrentUser};
use crate::error::{AppError, AppResult};
use crate::model::UserSummary;
use crate::spaces::{self, NewUser};
use crate::state::AppState;

pub const LOCALES: &[&str] = &["en", "hi", "mr"];
pub const THEMES: &[&str] = &["system", "light", "dark", "saffron"];

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn user_count(state: &AppState) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM users").fetch_one(&state.db).await?)
}

pub async fn setup_status(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let needs_setup = user_count(&state).await? == 0;
    Ok(Json(json!({ "needs_setup": needs_setup, "version": env!("CARGO_PKG_VERSION") })))
}

#[derive(Deserialize)]
pub struct SetupRequest {
    username: String,
    display_name: String,
    password: String,
    #[serde(default)]
    locale: Option<String>,
}

fn clean_locale(locale: Option<&str>) -> &'static str {
    locale
        .and_then(|l| LOCALES.iter().find(|x| **x == l).copied())
        .unwrap_or("en")
}

fn clean_display_name(raw: &str) -> AppResult<String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(AppError::bad("name must be 1-64 characters"));
    }
    Ok(name.to_string())
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers.get(USER_AGENT).and_then(|v| v.to_str().ok()).map(String::from)
}

/// First-run setup: creates the admin account and the family space.
pub async fn setup(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<SetupRequest>,
) -> AppResult<(CookieJar, Json<MeResponse>)> {
    let username = auth::normalize_username(&req.username)?;
    let display_name = clean_display_name(&req.display_name)?;
    auth::check_password_strength(&req.password)?;
    let hash = auth::hash_password(req.password).await?;

    let mut tx = state.db.begin().await?;
    // Serialise concurrent setup attempts; only the first one may succeed.
    sqlx::query("LOCK TABLE users IN EXCLUSIVE MODE").execute(&mut *tx).await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users").fetch_one(&mut *tx).await?;
    if count > 0 {
        return Err(AppError::Conflict("setup is already complete".into()));
    }
    spaces::ensure_family_space(&mut tx, &state.storage).await?;
    let user_id = spaces::create_user(
        &mut tx,
        &state.storage,
        NewUser {
            username: &username,
            display_name: &display_name,
            password_hash: &hash,
            role: "admin",
            locale: clean_locale(req.locale.as_deref()),
        },
    )
    .await?;
    tx.commit().await?;

    let cookie = auth::start_session(&state, user_id, user_agent(&headers)).await?;
    let me = me_for(&state, user_id).await?;
    Ok((jar.add(cookie), Json(me)))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> AppResult<(CookieJar, Json<MeResponse>)> {
    let username = req.username.trim().to_lowercase();
    if state.login_locked(&username) {
        return Err(AppError::TooManyRequests);
    }
    let row: Option<(Uuid, String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        "SELECT id, password_hash, disabled_at FROM users WHERE lower(username) = $1",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?;

    // Verify even for unknown users so timing does not reveal who exists.
    let (user_id, hash, disabled) = match row {
        Some(r) => (Some(r.0), r.1, r.2.is_some()),
        None => (None, "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$ZmFrZWhhc2hmYWtlaGFzaA".into(), false),
    };
    let ok = auth::verify_password(req.password, hash).await?;
    match user_id {
        Some(id) if ok && !disabled => {
            state.clear_login_failures(&username);
            let cookie = auth::start_session(&state, id, user_agent(&headers)).await?;
            let me = me_for(&state, id).await?;
            Ok((jar.add(cookie), Json(me)))
        }
        _ => {
            state.record_login_failure(&username);
            Err(AppError::BadRequest("wrong username or password".into()))
        }
    }
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> AppResult<(CookieJar, Json<Value>)> {
    auth::end_session(&state, &jar).await?;
    Ok((jar.add(auth::clear_cookie(&state)), Json(json!({ "ok": true }))))
}

#[derive(Serialize)]
pub struct MeResponse {
    #[serde(flatten)]
    user: CurrentUser,
    used_bytes: i64,
    home_id: Option<Uuid>,
    family_id: Option<Uuid>,
    /// The welcome tour has been seen (or skipped).
    tour_seen: bool,
    /// The getting-started checklist was put away.
    checklist_hidden: bool,
}

async fn me_for(state: &AppState, user_id: Uuid) -> AppResult<MeResponse> {
    let user: CurrentUser = sqlx::query_as(
        "SELECT id, username, display_name, role, locale, theme, avatar_color, quota_bytes,
                '00000000-0000-0000-0000-000000000000'::uuid AS session_id
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;
    let used_bytes = spaces::used_bytes(&state.db, user_id).await?;
    let (home_id, family_id) = spaces::roots(&state.db, user_id).await?;
    let (tour_seen, checklist_hidden): (bool, bool) =
        sqlx::query_as("SELECT tour_seen_at IS NOT NULL, checklist_hidden_at IS NOT NULL FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&state.db)
            .await?;
    Ok(MeResponse { user, used_bytes, home_id, family_id, tour_seen, checklist_hidden })
}

pub async fn me(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<MeResponse>> {
    Ok(Json(me_for(&state, user.id).await?))
}

#[derive(Deserialize)]
pub struct UpdateMe {
    display_name: Option<String>,
    locale: Option<String>,
    theme: Option<String>,
    /// true: the tour is done; false: show it again.
    tour_seen: Option<bool>,
    checklist_hidden: Option<bool>,
}

pub async fn update_me(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<UpdateMe>,
) -> AppResult<Json<MeResponse>> {
    if let Some(name) = req.display_name {
        let name = clean_display_name(&name)?;
        sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
            .bind(name)
            .bind(user.id)
            .execute(&state.db)
            .await?;
    }
    if let Some(locale) = req.locale {
        if !LOCALES.contains(&locale.as_str()) {
            return Err(AppError::bad("unsupported language"));
        }
        sqlx::query("UPDATE users SET locale = $1 WHERE id = $2")
            .bind(locale)
            .bind(user.id)
            .execute(&state.db)
            .await?;
    }
    if let Some(theme) = req.theme {
        if !THEMES.contains(&theme.as_str()) {
            return Err(AppError::bad("unknown theme"));
        }
        sqlx::query("UPDATE users SET theme = $1 WHERE id = $2")
            .bind(theme)
            .bind(user.id)
            .execute(&state.db)
            .await?;
    }
    if let Some(seen) = req.tour_seen {
        sqlx::query("UPDATE users SET tour_seen_at = CASE WHEN $1 THEN now() END WHERE id = $2")
            .bind(seen)
            .bind(user.id)
            .execute(&state.db)
            .await?;
    }
    if let Some(hidden) = req.checklist_hidden {
        sqlx::query("UPDATE users SET checklist_hidden_at = CASE WHEN $1 THEN now() END WHERE id = $2")
            .bind(hidden)
            .bind(user.id)
            .execute(&state.db)
            .await?;
    }
    Ok(Json(me_for(&state, user.id).await?))
}

#[derive(Serialize)]
pub struct Progress {
    members: i64,
    family_items: i64,
    shares: i64,
    backups: i64,
}

/// How far the family has got, for the getting-started checklist.
pub async fn progress(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Progress>> {
    let (members, family_items, shares, backups): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM users WHERE disabled_at IS NULL),
                (SELECT count(*) FROM nodes n JOIN spaces s ON s.id = n.space_id
                  WHERE s.kind = 'family' AND n.parent_id IS NOT NULL AND n.trashed_at IS NULL),
                (SELECT count(*) FROM shares WHERE created_by = $1)
                  + (SELECT count(*) FROM links WHERE created_by = $1),
                (SELECT count(*) FROM backup_targets)",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(Progress { members, family_items, shares, backups }))
}

#[derive(Deserialize)]
pub struct ChangePassword {
    current: String,
    new: String,
}

/// Changes the password and signs out every other device.
pub async fn change_password(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<ChangePassword>,
) -> AppResult<Json<Value>> {
    auth::check_password_strength(&req.new)?;
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_one(&state.db)
        .await?;
    if !auth::verify_password(req.current, hash).await? {
        return Err(AppError::bad("current password is wrong"));
    }
    let new_hash = auth::hash_password(req.new).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(new_hash)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND id <> $2")
        .bind(user.id)
        .bind(user.session_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Everyone on this server, for the share dialog.
pub async fn directory(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<Vec<UserSummary>>> {
    let users = sqlx::query_as(
        "SELECT id, username, display_name, avatar_color FROM users
         WHERE disabled_at IS NULL ORDER BY display_name",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(users))
}
