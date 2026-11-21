//! Finding a node, checking what the current user may do with it, and
//! working out where it lives on disk.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use fennoly_core::Permission;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::model::{NODE_COLS, NodeRow};
use crate::state::AppState;
use crate::storage::Storage;

/// One step on the path from a node up to its space root.
#[derive(Debug, Clone, FromRow)]
pub struct AncRow {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub trashed_at: Option<DateTime<Utc>>,
    pub dir_name: String,
    pub space_kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Crumb {
    pub id: Uuid,
    pub name: String,
    pub is_root: bool,
    /// `personal`, `family`, or `shared` when the trail starts at a shared item.
    pub space: String,
}

/// A node the current user can reach.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub node: NodeRow,
    pub perm: Permission,
    /// The node first, then each ancestor up to the space root.
    pub chain: Vec<AncRow>,
    pub path: PathBuf,
}

impl Resolved {
    pub fn in_trash(&self) -> bool {
        self.chain.iter().any(|r| r.trashed_at.is_some())
    }

    pub fn space_kind(&self) -> &str {
        self.chain.last().map(|r| r.space_kind.as_str()).unwrap_or("personal")
    }

    pub fn require(&self, needed: Permission) -> AppResult<()> {
        if self.perm >= needed { Ok(()) } else { Err(AppError::Forbidden) }
    }

    pub fn require_write(&self) -> AppResult<()> {
        self.require(Permission::Editor)
    }
}

pub async fn chain(state: &AppState, id: Uuid) -> AppResult<Vec<AncRow>> {
    let rows: Vec<AncRow> = sqlx::query_as(
        "WITH RECURSIVE anc AS (
             SELECT id, parent_id, name, trashed_at, space_id, 0 AS depth FROM nodes WHERE id = $1
             UNION ALL
             SELECT n.id, n.parent_id, n.name, n.trashed_at, n.space_id, a.depth + 1
             FROM nodes n JOIN anc a ON n.id = a.parent_id
         )
         SELECT anc.id, anc.parent_id, anc.name, anc.trashed_at, s.dir_name, s.kind AS space_kind
         FROM anc JOIN spaces s ON s.id = anc.space_id
         ORDER BY anc.depth",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    if rows.is_empty() {
        return Err(AppError::NotFound);
    }
    Ok(rows)
}

/// Where a node is on disk, given its chain (node first, root last).
/// A node inside a trashed folder lives under that folder's trash entry.
pub fn disk_path(storage: &Storage, chain: &[AncRow]) -> PathBuf {
    let mut names = Vec::new();
    let mut base = None;
    for row in chain {
        if row.trashed_at.is_some() {
            base = Some(storage.trash_item(row.id));
            break;
        }
        if row.parent_id.is_none() {
            base = Some(storage.space_dir(&row.dir_name));
            break;
        }
        names.push(row.name.as_str());
    }
    let mut path = base.unwrap_or_else(|| storage.space_dir(&chain[chain.len() - 1].dir_name));
    for name in names.iter().rev() {
        path.push(name);
    }
    path
}

async fn permission(state: &AppState, user_id: Uuid, id: Uuid) -> AppResult<Option<Permission>> {
    let perm: Option<String> = sqlx::query_scalar("SELECT node_access($1, $2)")
        .bind(id)
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    Ok(perm.as_deref().and_then(Permission::parse))
}

async fn resolve_inner(state: &AppState, user_id: Uuid, id: Uuid, allow_trash: bool) -> AppResult<Resolved> {
    let node: NodeRow = sqlx::query_as(&format!("SELECT {NODE_COLS} FROM nodes n WHERE n.id = $1"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    // No access looks the same as "does not exist".
    let perm = permission(state, user_id, id).await?.ok_or(AppError::NotFound)?;
    let chain = chain(state, id).await?;
    let path = disk_path(&state.storage, &chain);
    let resolved = Resolved { node, perm, chain, path };
    if !allow_trash && resolved.in_trash() {
        return Err(AppError::NotFound);
    }
    Ok(resolved)
}

/// Finds a live (not trashed) node the user can access.
pub async fn resolve(state: &AppState, user_id: Uuid, id: Uuid) -> AppResult<Resolved> {
    resolve_inner(state, user_id, id, false).await
}

/// Like [`resolve`], but also finds trashed nodes (for restore and purge).
pub async fn resolve_with_trash(state: &AppState, user_id: Uuid, id: Uuid) -> AppResult<Resolved> {
    resolve_inner(state, user_id, id, true).await
}

/// Finds a node reached through a public link rooted at `root`: the node must
/// be the root or inside it, and nothing on the way may be in the trash.
pub async fn resolve_in_link(state: &AppState, root: Uuid, id: Uuid) -> AppResult<Resolved> {
    let node: NodeRow = sqlx::query_as(&format!("SELECT {NODE_COLS} FROM nodes n WHERE n.id = $1"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let chain = chain(state, id).await?;
    let inside = chain.iter().any(|c| c.id == root);
    let resolved = Resolved { path: disk_path(&state.storage, &chain), node, perm: Permission::Viewer, chain };
    if !inside || resolved.in_trash() {
        return Err(AppError::NotFound);
    }
    Ok(resolved)
}

/// May this user create public links for the node? The owner of a personal
/// item may; in the family space every editor may.
pub fn can_link(r: &Resolved) -> bool {
    r.perm == Permission::Owner || (r.space_kind() == "family" && r.perm.can_write())
}

/// The breadcrumb trail the user may see. People who reach a node through a
/// share only see the trail from the shared item down, never the owner's
/// folders above it.
pub async fn crumbs(state: &AppState, user_id: Uuid, r: &Resolved) -> AppResult<Vec<Crumb>> {
    let kind = r.space_kind().to_string();
    let own_space = r.perm == Permission::Owner || kind == "family";
    let top = if own_space {
        r.chain.len() - 1
    } else {
        let ids: Vec<Uuid> = r.chain.iter().map(|c| c.id).collect();
        let shared: Vec<Uuid> = sqlx::query_scalar(
            "SELECT node_id FROM shares WHERE user_id = $1 AND node_id = ANY($2)",
        )
        .bind(user_id)
        .bind(&ids)
        .fetch_all(&state.db)
        .await?;
        r.chain
            .iter()
            .rposition(|c| shared.contains(&c.id))
            .unwrap_or(0)
    };
    let space = if own_space { kind } else { "shared".to_string() };
    Ok(r.chain[..=top]
        .iter()
        .rev()
        .map(|c| Crumb {
            id: c.id,
            name: c.name.clone(),
            is_root: c.parent_id.is_none(),
            space: space.clone(),
        })
        .collect())
}

/// Lower-cased names of the live children of a folder, for conflict checks.
pub async fn sibling_names(state: &AppState, parent_id: Uuid) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT lower(name) FROM nodes WHERE parent_id = $1 AND trashed_at IS NULL",
    )
    .bind(parent_id)
    .fetch_all(&state.db)
    .await?)
}

/// A free name in `parent_id` based on `wanted` (adds " (1)" etc.).
pub async fn free_name(state: &AppState, parent_id: Uuid, wanted: &str) -> AppResult<String> {
    let taken = sibling_names(state, parent_id).await?;
    Ok(fennoly_core::unique_name(wanted, |n| taken.contains(&n.to_lowercase())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: Uuid, parent: Option<Uuid>, name: &str, trashed: bool) -> AncRow {
        AncRow {
            id,
            parent_id: parent,
            name: name.into(),
            trashed_at: trashed.then(Utc::now),
            dir_name: "users/noor".into(),
            space_kind: "personal".into(),
        }
    }

    #[test]
    fn path_of_live_node() {
        let storage = Storage::new(tempfile::tempdir().unwrap().keep()).unwrap();
        let (root, a, b) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let chain = [row(b, Some(a), "scan.pdf", false), row(a, Some(root), "Tax", false), row(root, None, "My files", false)];
        let p = disk_path(&storage, &chain);
        assert!(p.ends_with("spaces/users/noor/Tax/scan.pdf"), "{}", p.display());
    }

    #[test]
    fn path_inside_trashed_folder() {
        let storage = Storage::new(tempfile::tempdir().unwrap().keep()).unwrap();
        let (root, a, b) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let chain = [row(b, Some(a), "scan.pdf", false), row(a, Some(root), "Tax", true), row(root, None, "My files", false)];
        let p = disk_path(&storage, &chain);
        assert!(p.ends_with(format!("trash/{a}/scan.pdf")), "{}", p.display());
    }

    #[test]
    fn root_maps_to_space_dir() {
        let storage = Storage::new(tempfile::tempdir().unwrap().keep()).unwrap();
        let root = Uuid::new_v4();
        let p = disk_path(&storage, &[row(root, None, "My files", false)]);
        assert!(p.ends_with("spaces/users/noor"), "{}", p.display());
    }
}
