//! Sharing files and folders with other people on the server.

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use fennoly_core::Permission;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use uuid::Uuid;

use crate::access;
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::model::{NODE_COLS, NodeDto, NodeView, VIEW_COLS};
use crate::state::AppState;

#[derive(Serialize, FromRow)]
pub struct ShareRow {
    id: Uuid,
    node_id: Uuid,
    permission: String,
    created_at: DateTime<Utc>,
    user_id: Uuid,
    username: String,
    display_name: String,
    avatar_color: String,
}

async fn shares_of(state: &AppState, node_id: Uuid) -> AppResult<Vec<ShareRow>> {
    Ok(sqlx::query_as(
        "SELECT sh.id, sh.node_id, sh.permission, sh.created_at,
                u.id AS user_id, u.username, u.display_name, u.avatar_color
         FROM shares sh JOIN users u ON u.id = sh.user_id
         WHERE sh.node_id = $1 ORDER BY u.display_name",
    )
    .bind(node_id)
    .fetch_all(&state.db)
    .await?)
}

pub async fn list(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Vec<ShareRow>>> {
    let r = access::resolve(&state, user.id, id).await?;
    r.require(Permission::Owner)?;
    Ok(Json(shares_of(&state, id).await?))
}

#[derive(Deserialize)]
pub struct CreateShare {
    user_id: Uuid,
    permission: String,
}

fn grantable(p: &str) -> AppResult<Permission> {
    Permission::parse_grantable(p).ok_or_else(|| AppError::bad("permission must be viewer or editor"))
}

/// Shares a node with a user, or changes the permission of an existing share.
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateShare>,
) -> AppResult<Json<Vec<ShareRow>>> {
    let r = access::resolve(&state, user.id, id).await?;
    r.require(Permission::Owner)?;
    if r.space_kind() == "family" {
        return Err(AppError::bad("everyone already has access to the family space"));
    }
    if req.user_id == user.id {
        return Err(AppError::bad("you cannot share with yourself"));
    }
    let perm = grantable(&req.permission)?;
    let active: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND disabled_at IS NULL)")
        .bind(req.user_id)
        .fetch_one(&state.db)
        .await?;
    if !active {
        return Err(AppError::NotFound);
    }
    sqlx::query(
        "INSERT INTO shares (node_id, user_id, permission, created_by) VALUES ($1, $2, $3, $4)
         ON CONFLICT (node_id, user_id) DO UPDATE SET permission = EXCLUDED.permission",
    )
    .bind(id)
    .bind(req.user_id)
    .bind(perm.as_str())
    .bind(user.id)
    .execute(&state.db)
    .await?;
    Ok(Json(shares_of(&state, id).await?))
}

#[derive(Deserialize)]
pub struct UpdateShare {
    permission: String,
}

async fn share_node(state: &AppState, share_id: Uuid) -> AppResult<(Uuid, Uuid)> {
    sqlx::query_as("SELECT node_id, user_id FROM shares WHERE id = $1")
        .bind(share_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(share_id): Path<Uuid>,
    Json(req): Json<UpdateShare>,
) -> AppResult<Json<Vec<ShareRow>>> {
    let (node_id, _) = share_node(&state, share_id).await?;
    access::resolve(&state, user.id, node_id).await?.require(Permission::Owner)?;
    sqlx::query("UPDATE shares SET permission = $1 WHERE id = $2")
        .bind(grantable(&req.permission)?.as_str())
        .bind(share_id)
        .execute(&state.db)
        .await?;
    Ok(Json(shares_of(&state, node_id).await?))
}

/// Removes a share. The owner can remove any share; the person it was
/// shared with can remove it from their own list.
pub async fn remove(State(state): State<AppState>, user: CurrentUser, Path(share_id): Path<Uuid>) -> AppResult<Json<Value>> {
    let (node_id, recipient) = share_node(&state, share_id).await?;
    if recipient != user.id {
        access::resolve(&state, user.id, node_id).await?.require(Permission::Owner)?;
    }
    sqlx::query("DELETE FROM shares WHERE id = $1").bind(share_id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize)]
pub struct SharedItem {
    #[serde(flatten)]
    node: Option<NodeDto>,
    share_id: Uuid,
    shared_by: String,
    shared_by_color: String,
    shared_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct SharedRow {
    #[sqlx(flatten)]
    view: NodeView,
    share_id: Uuid,
    shared_by: String,
    shared_by_color: String,
    shared_at: DateTime<Utc>,
}

/// Items other people shared with the current user.
pub async fn shared_with_me(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<SharedItem>>> {
    let rows: Vec<SharedRow> = sqlx::query_as(&format!(
        "SELECT {NODE_COLS}, {VIEW_COLS},
                sh.id AS share_id, COALESCE(o.display_name, '') AS shared_by,
                COALESCE(o.avatar_color, '#6366f1') AS shared_by_color,
                sh.created_at AS shared_at
         FROM shares sh
         JOIN nodes n ON n.id = sh.node_id
         LEFT JOIN users o ON o.id = sh.created_by
         WHERE sh.user_id = $1 AND node_live(n.id)
         ORDER BY sh.created_at DESC"
    ))
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| SharedItem {
                node: Some(r.view.into()),
                share_id: r.share_id,
                shared_by: r.shared_by,
                shared_by_color: r.shared_by_color,
                shared_at: r.shared_at,
            })
            .collect(),
    ))
}
