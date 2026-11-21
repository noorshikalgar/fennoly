//! Admin: manage the family's accounts.

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::{self, AdminUser};
use crate::error::{AppError, AppResult};
use crate::spaces::{self, NewUser};
use crate::state::AppState;
use crate::storage::remove_path;

#[derive(Serialize, FromRow)]
pub struct AdminUserRow {
    id: Uuid,
    username: String,
    display_name: String,
    role: String,
    avatar_color: String,
    quota_bytes: Option<i64>,
    used_bytes: i64,
    disabled: bool,
    created_at: DateTime<Utc>,
    last_seen_at: Option<DateTime<Utc>>,
}

async fn fetch_users(state: &AppState, only: Option<Uuid>) -> AppResult<Vec<AdminUserRow>> {
    Ok(sqlx::query_as(
        "SELECT u.id, u.username, u.display_name, u.role, u.avatar_color, u.quota_bytes,
                COALESCE((SELECT SUM(n.size) FROM nodes n JOIN spaces s ON s.id = n.space_id
                          WHERE s.owner_id = u.id AND n.kind = 'file'), 0)::BIGINT AS used_bytes,
                (u.disabled_at IS NOT NULL) AS disabled,
                u.created_at,
                (SELECT max(last_seen_at) FROM sessions WHERE user_id = u.id) AS last_seen_at
         FROM users u
         WHERE $1::uuid IS NULL OR u.id = $1
         ORDER BY u.created_at",
    )
    .bind(only)
    .fetch_all(&state.db)
    .await?)
}

pub async fn list(State(state): State<AppState>, _admin: AdminUser) -> AppResult<Json<Vec<AdminUserRow>>> {
    Ok(Json(fetch_users(&state, None).await?))
}

#[derive(Deserialize)]
pub struct CreateUser {
    username: String,
    display_name: String,
    password: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    locale: Option<String>,
}

fn clean_role(role: Option<&str>) -> AppResult<&'static str> {
    match role.unwrap_or("member") {
        "member" => Ok("member"),
        "admin" => Ok("admin"),
        _ => Err(AppError::bad("role must be admin or member")),
    }
}

pub async fn create(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<CreateUser>,
) -> AppResult<Json<AdminUserRow>> {
    let username = auth::normalize_username(&req.username)?;
    let display_name = req.display_name.trim();
    if display_name.is_empty() || display_name.chars().count() > 64 {
        return Err(AppError::bad("name must be 1-64 characters"));
    }
    auth::check_password_strength(&req.password)?;
    let role = clean_role(req.role.as_deref())?;
    let locale = req
        .locale
        .as_deref()
        .filter(|l| super::account::LOCALES.contains(l))
        .unwrap_or("en");
    let hash = auth::hash_password(req.password).await?;

    let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE lower(username) = $1)")
        .bind(&username)
        .fetch_one(&state.db)
        .await?;
    if taken {
        return Err(AppError::Conflict("this username is taken".into()));
    }
    let mut tx = state.db.begin().await?;
    let id = spaces::create_user(
        &mut tx,
        &state.storage,
        NewUser { username: &username, display_name, password_hash: &hash, role, locale },
    )
    .await?;
    tx.commit().await?;
    let row = fetch_users(&state, Some(id)).await?.pop().ok_or(AppError::NotFound)?;
    Ok(Json(row))
}

#[derive(Deserialize)]
pub struct UpdateUser {
    display_name: Option<String>,
    role: Option<String>,
    disabled: Option<bool>,
    /// `null` removes the quota; a number sets it in bytes.
    #[serde(default, with = "double_option")]
    quota_bytes: Option<Option<i64>>,
    /// Sets a new password and signs the user out everywhere.
    password: Option<String>,
}

mod double_option {
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
        Ok(Some(Option::deserialize(d)?))
    }
}

async fn active_admin_count(state: &AppState) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM users WHERE role = 'admin' AND disabled_at IS NULL")
        .fetch_one(&state.db)
        .await?)
}

pub async fn update(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUser>,
) -> AppResult<Json<AdminUserRow>> {
    let current_role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let demoting = req.role.as_deref() == Some("member") && current_role == "admin";
    let disabling = req.disabled == Some(true);
    if id == admin.id && (demoting || disabling) {
        return Err(AppError::bad("you cannot demote or disable your own account"));
    }
    if (demoting || disabling) && current_role == "admin" && active_admin_count(&state).await? <= 1 {
        return Err(AppError::bad("the server needs at least one admin"));
    }

    let mut tx = state.db.begin().await?;
    if let Some(name) = req.display_name.as_deref().map(str::trim) {
        if name.is_empty() || name.chars().count() > 64 {
            return Err(AppError::bad("name must be 1-64 characters"));
        }
        sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
            .bind(name)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(role) = req.role.as_deref() {
        sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
            .bind(clean_role(Some(role))?)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(disabled) = req.disabled {
        sqlx::query("UPDATE users SET disabled_at = CASE WHEN $1 THEN now() ELSE NULL END WHERE id = $2")
            .bind(disabled)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if disabled {
            sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(id).execute(&mut *tx).await?;
        }
    }
    if let Some(quota) = req.quota_bytes {
        if quota.is_some_and(|q| q < 0) {
            return Err(AppError::bad("quota cannot be negative"));
        }
        sqlx::query("UPDATE users SET quota_bytes = $1 WHERE id = $2")
            .bind(quota)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(password) = req.password {
        auth::check_password_strength(&password)?;
        let hash = auth::hash_password(password).await?;
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(hash)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    let row = fetch_users(&state, Some(id)).await?.pop().ok_or(AppError::NotFound)?;
    Ok(Json(row))
}

/// Deletes a user and everything in their personal space, trash included.
pub async fn remove(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    if id == admin.id {
        return Err(AppError::bad("you cannot delete your own account"));
    }
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if role == "admin" && active_admin_count(&state).await? <= 1 {
        return Err(AppError::bad("the server needs at least one admin"));
    }
    let dir: Option<String> = sqlx::query_scalar("SELECT dir_name FROM spaces WHERE owner_id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    let trashed: Vec<Uuid> = sqlx::query_scalar(
        "SELECT n.id FROM nodes n JOIN spaces s ON s.id = n.space_id
         WHERE s.owner_id = $1 AND n.trashed_at IS NOT NULL",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    sqlx::query("DELETE FROM users WHERE id = $1").bind(id).execute(&state.db).await?;
    if let Some(dir) = dir {
        remove_path(&state.storage.space_dir(&dir)).await?;
    }
    for node in trashed {
        remove_path(&state.storage.trash_item(node)).await?;
    }
    Ok(Json(json!({ "ok": true })))
}
