-- Backups: destinations ("targets") and the history of runs.
CREATE TABLE backup_targets (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name           TEXT NOT NULL,
    -- local: a folder or USB drive; smb: a Windows/NAS share; sftp; s3 (incl. Backblaze B2)
    kind           TEXT NOT NULL CHECK (kind IN ('local', 'smb', 'sftp', 's3')),
    -- Non-secret settings (path, host, share, bucket, …) as JSON.
    settings       JSONB NOT NULL DEFAULT '{}',
    -- Secret settings (passwords, access keys), encrypted with the server key.
    secrets        TEXT,
    encrypted      BOOLEAN NOT NULL DEFAULT TRUE,
    -- The repository password (the "recovery key"), encrypted with the server key.
    repo_password  TEXT,
    -- "manual", "daily 02:00" or "weekly sun 02:00" (server local time).
    schedule       TEXT NOT NULL DEFAULT 'daily 02:00',
    keep_daily     INTEGER NOT NULL DEFAULT 7 CHECK (keep_daily BETWEEN 0 AND 365),
    keep_weekly    INTEGER NOT NULL DEFAULT 4 CHECK (keep_weekly BETWEEN 0 AND 520),
    keep_monthly   INTEGER NOT NULL DEFAULT 12 CHECK (keep_monthly BETWEEN 0 AND 240),
    include_trash  BOOLEAN NOT NULL DEFAULT FALSE,
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    initialized    BOOLEAN NOT NULL DEFAULT FALSE,
    next_run_at    TIMESTAMPTZ,
    created_by     UUID REFERENCES users (id) ON DELETE SET NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE backup_runs (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_id      UUID NOT NULL REFERENCES backup_targets (id) ON DELETE CASCADE,
    kind           TEXT NOT NULL CHECK (kind IN ('backup', 'check', 'restore')),
    status         TEXT NOT NULL CHECK (status IN ('running', 'success', 'failed')),
    trigger        TEXT NOT NULL DEFAULT 'manual' CHECK (trigger IN ('manual', 'schedule')),
    started_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at    TIMESTAMPTZ,
    snapshot_id    TEXT,
    files_new      BIGINT,
    files_changed  BIGINT,
    bytes_added    BIGINT,
    total_files    BIGINT,
    total_bytes    BIGINT,
    error          TEXT,
    started_by     UUID REFERENCES users (id) ON DELETE SET NULL
);
CREATE INDEX backup_runs_target_idx ON backup_runs (target_id, started_at DESC);
