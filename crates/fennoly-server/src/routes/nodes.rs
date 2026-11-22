//! Browsing, creating, renaming, moving and trashing files and folders.

use axum::Json;
use axum::extract::{Path, Query, State};
use fennoly_core::Permission;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::access::{self, Crumb};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::model::{NODE_COLS, NodeDto, NodeView, VIEW_COLS};
use crate::state::AppState;
use crate::storage::move_path;

/// One node as the current user sees it.
pub async fn view(state: &AppState, user_id: Uuid, id: Uuid) -> AppResult<NodeDto> {
    let v: NodeView = sqlx::query_as(&format!(
        "SELECT {NODE_COLS}, {VIEW_COLS} FROM nodes n WHERE n.id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(v.into())
}

async fn list(state: &AppState, user_id: Uuid, where_order_limit: &str, arg: Option<&str>) -> AppResult<Vec<NodeDto>> {
    let sql = format!("SELECT {NODE_COLS}, {VIEW_COLS} FROM nodes n {where_order_limit}");
    let mut q = sqlx::query_as::<_, NodeView>(&sql).bind(user_id);
    if let Some(a) = arg {
        q = q.bind(a);
    }
    Ok(q.fetch_all(&state.db).await?.into_iter().map(Into::into).collect())
}

#[derive(Serialize)]
pub struct NodeResponse {
    node: NodeDto,
    crumbs: Vec<Crumb>,
}

pub async fn get_node(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<NodeResponse>> {
    let r = access::resolve(&state, user.id, id).await?;
    let crumbs = access::crumbs(&state, user.id, &r).await?;
    let node = view(&state, user.id, id).await?;
    Ok(Json(NodeResponse { node, crumbs }))
}

#[derive(Deserialize)]
pub struct ListQuery {
    sort: Option<String>,
    order: Option<String>,
}

pub async fn children(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<NodeDto>>> {
    let r = access::resolve(&state, user.id, id).await?;
    if !r.node.is_folder() {
        return Err(AppError::bad("not a folder"));
    }
    let column = match q.sort.as_deref() {
        Some("modified") => "n.modified_at",
        Some("size") => "n.size",
        _ => "lower(n.name)",
    };
    let dir = if q.order.as_deref() == Some("desc") { "DESC" } else { "ASC" };
    let sql = format!(
        "SELECT {NODE_COLS}, {VIEW_COLS} FROM nodes n
         WHERE n.parent_id = $2 AND n.trashed_at IS NULL
         ORDER BY (n.kind = 'folder') DESC, {column} {dir}, lower(n.name)"
    );
    let rows: Vec<NodeView> = sqlx::query_as(&sql).bind(user.id).bind(id).fetch_all(&state.db).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize)]
pub struct CreateFolder {
    name: String,
}

pub async fn create_folder(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(parent_id): Path<Uuid>,
    Json(req): Json<CreateFolder>,
) -> AppResult<Json<NodeDto>> {
    let id = create_folder_node(&state, &user, parent_id, &req.name).await?;
    Ok(Json(view(&state, user.id, id).await?))
}

/// Creates a folder named `name` in `parent_id` (which `user` must be able to
/// write to) and returns its id. Fails with Conflict if the name is taken.
pub(crate) async fn create_folder_node(state: &AppState, user: &CurrentUser, parent_id: Uuid, name: &str) -> AppResult<Uuid> {
    let parent = access::resolve(state, user.id, parent_id).await?;
    parent.require_write()?;
    if !parent.node.is_folder() {
        return Err(AppError::bad("not a folder"));
    }
    let name = fennoly_core::validate_name(name)?;
    if access::sibling_names(state, parent_id).await?.contains(&name.to_lowercase()) {
        return Err(AppError::Conflict("an item with this name already exists".into()));
    }
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO nodes (space_id, parent_id, name, kind, created_by)
         VALUES ($1, $2, $3, 'folder', $4) RETURNING id",
    )
    .bind(parent.node.space_id)
    .bind(parent_id)
    .bind(&name)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    tokio::fs::create_dir_all(parent.path.join(&name)).await?;
    tx.commit().await?;
    Ok(id)
}

#[derive(Deserialize)]
pub struct UpdateNode {
    name: Option<String>,
    parent_id: Option<Uuid>,
}

/// Renames and/or moves a node.
pub async fn update_node(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateNode>,
) -> AppResult<Json<NodeDto>> {
    let r = access::resolve(&state, user.id, id).await?;
    r.require_write()?;
    let current_parent = r.node.parent_id.ok_or_else(|| AppError::bad("the top folder cannot be changed"))?;
    let new_name = match req.name.as_deref() {
        Some(n) => fennoly_core::validate_name(n)?,
        None => r.node.name.clone(),
    };
    let target_id = req.parent_id.unwrap_or(current_parent);
    let target = access::resolve(&state, user.id, target_id).await?;
    target.require_write()?;
    if !target.node.is_folder() {
        return Err(AppError::bad("you can only move items into a folder"));
    }
    if target.chain.iter().any(|c| c.id == id) {
        return Err(AppError::bad("a folder cannot be moved into itself"));
    }
    if target.node.space_id != r.node.space_id {
        // Taking an item out of someone's space needs more than edit rights:
        // a share editor could otherwise move the owner's files into their own.
        if r.perm != Permission::Owner && r.space_kind() != "family" {
            return Err(AppError::Forbidden);
        }
        let bytes: i64 = sqlx::query_scalar(
            "WITH RECURSIVE sub AS (
                 SELECT id, kind, size FROM nodes WHERE id = $1
                 UNION ALL
                 SELECT n.id, n.kind, n.size FROM nodes n JOIN sub s ON n.parent_id = s.id WHERE n.trashed_at IS NULL
             )
             SELECT COALESCE(SUM(size) FILTER (WHERE kind = 'file'), 0)::BIGINT FROM sub",
        )
        .bind(id)
        .fetch_one(&state.db)
        .await?;
        super::uploads::check_quota(&state, &target, bytes.max(0) as u64).await?;
    }

    let same_place = target_id == current_parent;
    let taken = access::sibling_names(&state, target_id).await?;
    let clash = taken.contains(&new_name.to_lowercase())
        && !(same_place && new_name.to_lowercase() == r.node.name.to_lowercase());
    if clash {
        return Err(AppError::Conflict("an item with this name already exists".into()));
    }

    let new_path = target.path.join(&new_name);
    if new_path == r.path {
        return Ok(Json(view(&state, user.id, id).await?));
    }
    move_path(&r.path, &new_path).await?;

    let db_result = async {
        let mut tx = state.db.begin().await?;
        sqlx::query("UPDATE nodes SET name = $1, parent_id = $2 WHERE id = $3")
            .bind(&new_name)
            .bind(target_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if target.node.space_id != r.node.space_id {
            sqlx::query(
                "WITH RECURSIVE sub AS (
                     SELECT id FROM nodes WHERE id = $1
                     UNION ALL
                     SELECT n.id FROM nodes n JOIN sub s ON n.parent_id = s.id
                 )
                 UPDATE nodes SET space_id = $2 WHERE id IN (SELECT id FROM sub)",
            )
            .bind(id)
            .bind(target.node.space_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok::<_, AppError>(())
    }
    .await;
    if let Err(e) = db_result {
        // Put the file back so disk and database stay in step.
        let _ = move_path(&new_path, &r.path).await;
        return Err(e);
    }
    Ok(Json(view(&state, user.id, id).await?))
}

/// Moves a node to the trash.
pub async fn trash_node(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let r = access::resolve(&state, user.id, id).await?;
    r.require_write()?;
    if r.node.parent_id.is_none() {
        return Err(AppError::bad("the top folder cannot be deleted"));
    }
    let dest = state.storage.trash_item(id);
    move_path(&r.path, &dest).await?;
    let result = sqlx::query("UPDATE nodes SET trashed_at = now(), trashed_by = $1 WHERE id = $2")
        .bind(user.id)
        .bind(id)
        .execute(&state.db)
        .await;
    if let Err(e) = result {
        let _ = move_path(&dest, &r.path).await;
        return Err(e.into());
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn star(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    access::resolve(&state, user.id, id).await?;
    sqlx::query("INSERT INTO stars (user_id, node_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(user.id)
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn unstar(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM stars WHERE user_id = $1 AND node_id = $2")
        .bind(user.id)
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn recent(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<NodeDto>>> {
    Ok(Json(
        list(
            &state,
            user.id,
            "WHERE n.kind = 'file' AND n.trashed_at IS NULL
               AND node_access(n.id, $1) IS NOT NULL AND node_live(n.id)
             ORDER BY n.modified_at DESC LIMIT 40",
            None,
        )
        .await?,
    ))
}

pub async fn starred(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<NodeDto>>> {
    Ok(Json(
        list(
            &state,
            user.id,
            "JOIN stars s ON s.node_id = n.id AND s.user_id = $1
             WHERE node_access(n.id, $1) IS NOT NULL AND node_live(n.id)
             ORDER BY (n.kind = 'folder') DESC, lower(n.name)",
            None,
        )
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
}

pub async fn search(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Vec<NodeDto>>> {
    let term = q.q.trim().to_lowercase();
    if term.is_empty() {
        return Ok(Json(vec![]));
    }
    let escaped = term.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    Ok(Json(
        list(
            &state,
            user.id,
            "WHERE n.parent_id IS NOT NULL AND n.trashed_at IS NULL
               AND lower(n.name) LIKE '%' || $2 || '%'
               AND node_access(n.id, $1) IS NOT NULL AND node_live(n.id)
             ORDER BY (lower(n.name) LIKE $2 || '%') DESC, n.modified_at DESC
             LIMIT 60",
            Some(&escaped),
        )
        .await?,
    ))
}
