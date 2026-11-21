-- Public links: anyone with the URL can open the item, optionally
-- protected by a password and an expiry date, with or without download.
CREATE TABLE links (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id           UUID NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    token             TEXT NOT NULL UNIQUE,
    created_by        UUID REFERENCES users (id) ON DELETE CASCADE,
    allow_download    BOOLEAN NOT NULL DEFAULT TRUE,
    password_hash     TEXT,
    expires_at        TIMESTAMPTZ,
    view_count        BIGINT NOT NULL DEFAULT 0,
    download_count    BIGINT NOT NULL DEFAULT 0,
    last_accessed_at  TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX links_node_idx ON links (node_id);
CREATE INDEX links_creator_idx ON links (created_by);
