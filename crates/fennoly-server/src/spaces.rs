//! Creating users together with their storage spaces.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::error::AppResult;
use crate::storage::Storage;

pub const FAMILY_DIR: &str = "family";
pub const FAMILY_ROOT_NAME: &str = "Family";
pub const PERSONAL_ROOT_NAME: &str = "My files";

const AVATAR_COLORS: &[&str] = &[
    "#6366f1", "#ec4899", "#f59e0b", "#10b981", "#0ea5e9", "#8b5cf6", "#ef4444", "#14b8a6",
];

pub fn avatar_color(username: &str) -> &'static str {
    let h = blake3::hash(username.as_bytes());
    AVATAR_COLORS[h.as_bytes()[0] as usize % AVATAR_COLORS.len()]
}

async fn create_space(
    tx: &mut Transaction<'_, Postgres>,
    storage: &Storage,
    kind: &str,
    owner: Option<Uuid>,
    dir_name: &str,
    root_name: &str,
) -> AppResult<Uuid> {
    let space_id: Uuid = sqlx::query_scalar(
        "INSERT INTO spaces (kind, owner_id, dir_name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(kind)
    .bind(owner)
    .bind(dir_name)
    .fetch_one(&mut **tx)
    .await?;
    let root_id: Uuid = sqlx::query_scalar(
        "INSERT INTO nodes (space_id, parent_id, name, kind, created_by)
         VALUES ($1, NULL, $2, 'folder', $3) RETURNING id",
    )
    .bind(space_id)
    .bind(root_name)
    .bind(owner)
    .fetch_one(&mut **tx)
    .await?;
    tokio::fs::create_dir_all(storage.space_dir(dir_name)).await?;
    Ok(root_id)
}

/// Creates the family space if it does not exist yet.
pub async fn ensure_family_space(tx: &mut Transaction<'_, Postgres>, storage: &Storage) -> AppResult<()> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM spaces WHERE kind = 'family')")
        .fetch_one(&mut **tx)
        .await?;
    if !exists {
        create_space(tx, storage, "family", None, FAMILY_DIR, FAMILY_ROOT_NAME).await?;
    }
    Ok(())
}

pub struct NewUser<'a> {
    pub username: &'a str,
    pub display_name: &'a str,
    pub password_hash: &'a str,
    pub role: &'a str,
    pub locale: &'a str,
}

/// Inserts a user and creates their personal space. Returns the user id.
pub async fn create_user(
    tx: &mut Transaction<'_, Postgres>,
    storage: &Storage,
    new: NewUser<'_>,
) -> AppResult<Uuid> {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (username, display_name, password_hash, role, locale, avatar_color)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(new.username)
    .bind(new.display_name)
    .bind(new.password_hash)
    .bind(new.role)
    .bind(new.locale)
    .bind(avatar_color(new.username))
    .fetch_one(&mut **tx)
    .await?;
    let dir = format!("users/{}", new.username);
    create_space(tx, storage, "personal", Some(user_id), &dir, PERSONAL_ROOT_NAME).await?;
    Ok(user_id)
}

/// Bytes used by a user's personal space, trash included.
pub async fn used_bytes(db: &sqlx::PgPool, user_id: Uuid) -> AppResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(SUM(n.size), 0)::BIGINT FROM nodes n JOIN spaces s ON s.id = n.space_id
         WHERE s.owner_id = $1 AND n.kind = 'file'",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?)
}

/// Root folder ids of the user's personal space and the family space.
pub async fn roots(db: &sqlx::PgPool, user_id: Uuid) -> AppResult<(Option<Uuid>, Option<Uuid>)> {
    let home: Option<Uuid> = sqlx::query_scalar(
        "SELECT n.id FROM nodes n JOIN spaces s ON s.id = n.space_id
         WHERE s.owner_id = $1 AND s.kind = 'personal' AND n.parent_id IS NULL",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let family: Option<Uuid> = sqlx::query_scalar(
        "SELECT n.id FROM nodes n JOIN spaces s ON s.id = n.space_id
         WHERE s.kind = 'family' AND n.parent_id IS NULL",
    )
    .fetch_optional(db)
    .await?;
    Ok((home, family))
}
