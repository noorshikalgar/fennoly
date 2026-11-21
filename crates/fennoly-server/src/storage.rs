//! On-disk layout.
//!
//! ```text
//! <data>/spaces/family/...          the family space, as a plain folder tree
//! <data>/spaces/users/<name>/...    each personal space
//! <data>/trash/<node id>            trashed files and folders
//! <data>/tmp/uploads/<upload id>    uploads in progress
//! <data>/cache/thumbs/              generated thumbnails
//! <data>/cache/tools/               compress previews waiting to be kept
//! ```
//!
//! Files stay readable without Fennoly, so a family never loses access to
//! its documents even if the app is gone.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let root = root.into();
        for dir in ["spaces", "trash", "tmp/uploads", "cache/thumbs", "cache/tools"] {
            std::fs::create_dir_all(root.join(dir))?;
        }
        let root = std::fs::canonicalize(&root)?;
        Ok(Storage { root })
    }

    pub fn space_dir(&self, dir_name: &str) -> PathBuf {
        let mut p = self.root.join("spaces");
        for part in dir_name.split('/') {
            p.push(part);
        }
        p
    }

    pub fn trash_item(&self, node_id: Uuid) -> PathBuf {
        self.root.join("trash").join(node_id.to_string())
    }

    pub fn upload_file(&self, upload_id: Uuid) -> PathBuf {
        self.root.join("tmp").join("uploads").join(upload_id.to_string())
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn thumb(&self, node_id: Uuid, version: i64) -> PathBuf {
        self.root.join("cache").join("thumbs").join(format!("{node_id}-{version}.jpg"))
    }

    pub fn thumbs_dir(&self) -> PathBuf {
        self.root.join("cache").join("thumbs")
    }

    pub fn tool_output(&self, key: &str, ext: &str) -> PathBuf {
        self.root.join("cache").join("tools").join(format!("{key}.{ext}"))
    }

    pub fn tools_dir(&self) -> PathBuf {
        self.root.join("cache").join("tools")
    }
}

/// Moves a file or folder, creating the target's parent folder if needed.
pub async fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::rename(from, to).await
}

/// Deletes a file or a whole folder. Missing paths are not an error.
pub async fn remove_path(path: &Path) -> std::io::Result<()> {
    let result = match tokio::fs::metadata(path).await {
        Ok(meta) if meta.is_dir() => tokio::fs::remove_dir_all(path).await,
        Ok(_) => tokio::fs::remove_file(path).await,
        Err(e) => Err(e),
    };
    match result {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        other => other,
    }
}
