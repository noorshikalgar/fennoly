-- Fennoly initial schema.

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE users (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username       TEXT NOT NULL,
    display_name   TEXT NOT NULL,
    password_hash  TEXT NOT NULL,
    role           TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    locale         TEXT NOT NULL DEFAULT 'en',
    theme          TEXT NOT NULL DEFAULT 'system',
    avatar_color   TEXT NOT NULL DEFAULT '#6366f1',
    quota_bytes    BIGINT,
    disabled_at    TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_username_key ON users (lower(username));

CREATE TABLE sessions (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash    BYTEA NOT NULL UNIQUE,
    user_agent    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

-- A space is a top-level storage area: one personal space per user,
-- plus one family space everybody can use.
CREATE TABLE spaces (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind        TEXT NOT NULL CHECK (kind IN ('personal', 'family')),
    owner_id    UUID REFERENCES users (id) ON DELETE CASCADE,
    dir_name    TEXT NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((kind = 'personal') = (owner_id IS NOT NULL))
);
CREATE UNIQUE INDEX spaces_one_family ON spaces (kind) WHERE kind = 'family';
CREATE UNIQUE INDEX spaces_one_personal_per_user ON spaces (owner_id) WHERE kind = 'personal';

-- Files and folders. Each space has exactly one root folder (parent_id NULL).
-- On disk, a node lives at <space dir>/<names of its ancestors below the root>/<name>.
-- A trashed node moves to <data>/trash/<node id>; its descendants move with it.
CREATE TABLE nodes (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    space_id      UUID NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    parent_id     UUID REFERENCES nodes (id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL CHECK (kind IN ('folder', 'file')),
    size          BIGINT NOT NULL DEFAULT 0,
    mime          TEXT,
    content_hash  TEXT,
    created_by    UUID REFERENCES users (id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    trashed_at    TIMESTAMPTZ,
    trashed_by    UUID REFERENCES users (id) ON DELETE SET NULL
);
CREATE UNIQUE INDEX nodes_one_root_per_space ON nodes (space_id) WHERE parent_id IS NULL;
CREATE UNIQUE INDEX nodes_sibling_name ON nodes (parent_id, lower(name)) WHERE trashed_at IS NULL;
CREATE INDEX nodes_parent_idx ON nodes (parent_id);
CREATE INDEX nodes_space_modified_idx ON nodes (space_id, modified_at DESC);
CREATE INDEX nodes_trashed_idx ON nodes (trashed_at) WHERE trashed_at IS NOT NULL;
CREATE INDEX nodes_name_trgm ON nodes USING gin (lower(name) gin_trgm_ops);

CREATE TABLE shares (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id     UUID NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    permission  TEXT NOT NULL CHECK (permission IN ('viewer', 'editor')),
    created_by  UUID REFERENCES users (id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (node_id, user_id)
);
CREATE INDEX shares_user_idx ON shares (user_id);

CREATE TABLE stars (
    user_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    node_id     UUID NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, node_id)
);

-- tus uploads in progress. Partial uploads are parts of a parallel upload;
-- the final upload joins them.
CREATE TABLE uploads (
    id            UUID PRIMARY KEY,
    user_id       UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    parent_id     UUID REFERENCES nodes (id) ON DELETE CASCADE,
    filename      TEXT,
    mime          TEXT,
    length        BIGINT NOT NULL,
    upload_offset BIGINT NOT NULL DEFAULT 0,
    is_partial    BOOLEAN NOT NULL DEFAULT FALSE,
    node_id       UUID REFERENCES nodes (id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at  TIMESTAMPTZ
);
CREATE INDEX uploads_created_idx ON uploads (created_at);
