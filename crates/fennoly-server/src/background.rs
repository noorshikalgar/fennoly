//! Housekeeping that runs every hour: empty old trash, drop abandoned
//! uploads, expired sessions and stale tool previews.

use std::time::{Duration, SystemTime};

use uuid::Uuid;

use crate::routes::purge_node;
use crate::state::AppState;
use crate::storage::remove_path;

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(e) = run_once(&state).await {
                tracing::warn!(error = ?e, "housekeeping failed");
            }
        }
    });
}

async fn remove_old_files(dir: std::path::PathBuf, max_age: Duration) -> anyhow::Result<()> {
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let old = entry
            .metadata()
            .await?
            .modified()
            .ok()
            .and_then(|m| SystemTime::now().duration_since(m).ok())
            .is_some_and(|age| age > max_age);
        if old {
            remove_path(&entry.path()).await?;
        }
    }
    Ok(())
}

pub async fn run_once(state: &AppState) -> anyhow::Result<()> {
    let expired: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM nodes WHERE trashed_at < now() - make_interval(days => $1)",
    )
    .bind(state.cfg.trash_days as i32)
    .fetch_all(&state.db)
    .await?;
    for id in expired {
        if let Err(e) = purge_node(state, id).await {
            tracing::warn!(%id, error = %e, "could not purge trashed item");
        }
    }

    let stale: Vec<Uuid> = sqlx::query_scalar(
        "DELETE FROM uploads WHERE created_at < now() - interval '1 day' RETURNING id",
    )
    .fetch_all(&state.db)
    .await?;
    for id in stale {
        remove_path(&state.storage.upload_file(id)).await?;
    }

    // Interrupted chunked uploads are kept 3 days so they can be resumed.
    let abandoned: Vec<Uuid> = sqlx::query_scalar(
        "DELETE FROM chunked_uploads
         WHERE (completed_at IS NULL AND updated_at < now() - interval '3 days')
            OR (completed_at IS NOT NULL AND completed_at < now() - interval '1 day')
         RETURNING id",
    )
    .fetch_all(&state.db)
    .await?;
    for id in abandoned {
        remove_path(&state.storage.chunked_file(id)).await?;
    }

    sqlx::query("DELETE FROM sessions WHERE expires_at < now()").execute(&state.db).await?;
    remove_old_files(state.storage.tools_dir(), Duration::from_secs(3600)).await?;
    remove_old_files(state.storage.thumbs_dir(), Duration::from_secs(30 * 24 * 3600)).await?;
    Ok(())
}
