//! Database rows and the JSON shapes the API returns.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// Columns of `nodes`, for `SELECT {NODE_COLS} FROM nodes n`.
pub const NODE_COLS: &str = "n.id, n.space_id, n.parent_id, n.name, n.kind, n.size, n.mime, \
     n.created_by, n.created_at, n.modified_at, n.trashed_at";

#[derive(Debug, Clone, FromRow)]
pub struct NodeRow {
    pub id: Uuid,
    pub space_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub kind: String,
    pub size: i64,
    pub mime: Option<String>,
    #[allow(dead_code)] // kept for the activity view
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub trashed_at: Option<DateTime<Utc>>,
}

impl NodeRow {
    pub fn is_folder(&self) -> bool {
        self.kind == "folder"
    }
}

/// A node plus what the current user sees about it.
#[derive(Debug, Clone, FromRow)]
pub struct NodeView {
    #[sqlx(flatten)]
    pub node: NodeRow,
    pub starred: bool,
    pub permission: Option<String>,
    pub parent_name: Option<String>,
}

/// Extra columns for `NodeView` queries. Bind the user id as `$1`.
pub const VIEW_COLS: &str = "(EXISTS (SELECT 1 FROM stars st WHERE st.node_id = n.id AND st.user_id = $1)) AS starred, \
     node_access(n.id, $1) AS permission, \
     (SELECT p.name FROM nodes p WHERE p.id = n.parent_id) AS parent_name";

#[derive(Debug, Clone, Serialize)]
pub struct NodeDto {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub kind: String,
    pub size: i64,
    pub mime: Option<String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub trashed_at: Option<DateTime<Utc>>,
    pub starred: bool,
    pub permission: Option<String>,
    pub thumbnail: bool,
    pub is_root: bool,
    /// Name of the containing folder, shown in search and recent lists.
    pub parent_name: Option<String>,
}

pub fn has_thumbnail(node: &NodeRow) -> bool {
    !node.is_folder()
        && node
            .mime
            .as_deref()
            .is_some_and(fennoly_tools::image_tools::is_supported)
}

impl From<NodeView> for NodeDto {
    fn from(v: NodeView) -> Self {
        let thumbnail = has_thumbnail(&v.node);
        let n = v.node;
        NodeDto {
            id: n.id,
            is_root: n.parent_id.is_none(),
            parent_id: n.parent_id,
            name: n.name,
            kind: n.kind,
            size: n.size,
            mime: n.mime,
            created_at: n.created_at,
            modified_at: n.modified_at,
            trashed_at: n.trashed_at,
            starred: v.starred,
            permission: v.permission,
            thumbnail,
            parent_name: v.parent_name,
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct UserSummary {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_color: String,
}
