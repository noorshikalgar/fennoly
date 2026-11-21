//! The trash: list, restore, delete forever.

use axum::Json;
use axum::extract::{Path, State};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::access;
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::model::{NODE_COLS, NodeDto, NodeView, VIEW_COLS};
use crate::state::AppState;
use crate::storage::{move_path, remove_path};

/// Trash entries this user may see: everything trashed in their own space,
/// what they trashed anywhere, and (for admins) the family space's trash.
const VISIBLE: &str = "n.trashed_at IS NOT NULL AND (
        n.trashed_by = $1
        OR EXISTS (SELECT 1 FROM spaces s WHERE s.id = n.space_id
                   AND (s.owner_id = $1 OR (s.kind = 'family' AND $2)))
    )";

async fn visible_ids(state: &AppState, user: &CurrentUser, only: Option<Uuid>) -> AppResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(&format!(
        "SELECT n.id FROM nodes n WHERE {VISIBLE} AND ($3::uuid IS NULL OR n.id = $3)"
    ))
    .bind(user.id)
    .bind(user.is_admin())
    .bind(only)
    .fetch_all(&state.db)
    .await?)
}

pub async fn list(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<NodeDto>>> {
    let rows: Vec<NodeView> = sqlx::query_as(&format!(
        "SELECT {NODE_COLS}, {VIEW_COLS} FROM nodes n WHERE {VISIBLE} ORDER BY n.trashed_at DESC"
    ))
    .bind(user.id)
    .bind(user.is_admin())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

async fn require_visible(state: &AppState, user: &CurrentUser, id: Uuid) -> AppResult<()> {
    if visible_ids(state, user, Some(id)).await?.is_empty() {
        Err(AppError::NotFound)
    } else {
        Ok(())
    }
}

pub async fn restore(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<NodeDto>> {
    require_visible(&state, &user, id).await?;
    let node = access::resolve_with_trash(&state, user.id, id).await?;

    // Go back to the old folder if it still exists; otherwise to the top folder.
    let parent_live: bool = match node.node.parent_id {
        Some(p) => sqlx::query_scalar("SELECT node_live($1)").bind(p).fetch_one(&state.db).await?,
        None => false,
    };
    let target_id = match node.node.parent_id {
        Some(p) if parent_live => p,
        _ => sqlx::query_scalar("SELECT id FROM nodes WHERE space_id = $1 AND parent_id IS NULL")
            .bind(node.node.space_id)
            .fetch_one(&state.db)
            .await?,
    };
    let target = access::resolve_with_trash(&state, user.id, target_id).await?;
    let name = access::free_name(&state, target_id, &node.node.name).await?;
    let from = state.storage.trash_item(id);
    let to = target.path.join(&name);
    move_path(&from, &to).await?;
    let result = sqlx::query(
        "UPDATE nodes SET trashed_at = NULL, trashed_by = NULL, parent_id = $1, name = $2 WHERE id = $3",
    )
    .bind(target_id)
    .bind(&name)
    .bind(id)
    .execute(&state.db)
    .await;
    if let Err(e) = result {
        let _ = move_path(&to, &from).await;
        return Err(e.into());
    }
    Ok(Json(super::nodes::view(&state, user.id, id).await?))
}

/// Deletes a trashed node for good, including anything trashed inside it.
pub async fn purge_node(state: &AppState, id: Uuid) -> AppResult<()> {
    let trashed_inside: Vec<Uuid> = sqlx::query_scalar(
        "WITH RECURSIVE sub AS (
             SELECT id, trashed_at FROM nodes WHERE id = $1
             UNION ALL
             SELECT n.id, n.trashed_at FROM nodes n JOIN sub s ON n.parent_id = s.id
         )
         SELECT id FROM sub WHERE trashed_at IS NOT NULL",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    sqlx::query("DELETE FROM nodes WHERE id = $1").bind(id).execute(&state.db).await?;
    for node in trashed_inside {
        remove_path(&state.storage.trash_item(node)).await?;
    }
    Ok(())
}

pub async fn purge(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    require_visible(&state, &user, id).await?;
    purge_node(&state, id).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn empty(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let ids = visible_ids(&state, &user, None).await?;
    let count = ids.len();
    for id in ids {
        // A nested entry may already be gone with its parent.
        match purge_node(&state, id).await {
            Ok(()) | Err(AppError::NotFound) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(Json(json!({ "ok": true, "deleted": count })))
}
