//! Roles and permissions.

use serde::{Deserialize, Serialize};

/// A user's role on the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Role::Admin),
            "member" => Some(Role::Member),
            _ => None,
        }
    }
}

/// What a user may do with a file or folder. Ordered: a higher permission
/// includes everything a lower one allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// View, preview and download.
    Viewer,
    /// Also upload, create folders, rename, move and delete.
    Editor,
    /// Also share with others. Held by the owner of a personal space.
    Owner,
}

impl Permission {
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::Viewer => "viewer",
            Permission::Editor => "editor",
            Permission::Owner => "owner",
        }
    }

    /// Parses a permission that can be granted through a share.
    /// `owner` cannot be granted.
    pub fn parse_grantable(s: &str) -> Option<Self> {
        match s {
            "viewer" => Some(Permission::Viewer),
            "editor" => Some(Permission::Editor),
            _ => None,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Permission::Owner),
            other => Self::parse_grantable(other),
        }
    }

    pub fn can_write(self) -> bool {
        self >= Permission::Editor
    }

    pub fn can_share(self) -> bool {
        self == Permission::Owner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_matches_capability() {
        assert!(Permission::Owner > Permission::Editor);
        assert!(Permission::Editor > Permission::Viewer);
        assert!(!Permission::Viewer.can_write());
        assert!(Permission::Editor.can_write());
        assert!(!Permission::Editor.can_share());
        assert!(Permission::Owner.can_share());
    }

    #[test]
    fn owner_cannot_be_granted() {
        assert_eq!(Permission::parse_grantable("owner"), None);
        assert_eq!(Permission::parse("owner"), Some(Permission::Owner));
        assert_eq!(Permission::parse_grantable("editor"), Some(Permission::Editor));
    }

    #[test]
    fn role_round_trip() {
        for r in [Role::Admin, Role::Member] {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
    }
}
