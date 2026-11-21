-- Chunked uploads: the client hashes each fixed-size chunk of a file, tells
-- the server the list of hashes, then sends the chunks the server is missing,
-- several at a time and in any order. Each chunk is checked against its hash
-- before it counts, so a resumed or retried upload can never assemble a
-- corrupted file.
CREATE TABLE chunked_uploads (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    parent_id     UUID NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    filename      TEXT NOT NULL,
    size          BIGINT NOT NULL,
    chunk_size    BIGINT NOT NULL,
    chunk_hashes  TEXT[] NOT NULL,
    received      BOOLEAN[] NOT NULL,
    -- Identifies "the same file going to the same folder", so adding the same
    -- file again resumes the earlier attempt instead of starting over.
    manifest      TEXT NOT NULL,
    node_id       UUID REFERENCES nodes (id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at  TIMESTAMPTZ
);
CREATE UNIQUE INDEX chunked_uploads_open_manifest
    ON chunked_uploads (user_id, manifest) WHERE completed_at IS NULL;
CREATE INDEX chunked_uploads_updated_idx ON chunked_uploads (updated_at);
