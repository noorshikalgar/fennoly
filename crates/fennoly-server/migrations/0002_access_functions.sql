-- Access rules live in one place so every list (folder, search, recent,
-- starred) agrees on what a user may see.

-- The permission a user has on a node, or NULL for no access:
--   'owner'  - the node is in the user's personal space
--   'editor' - the node is in the family space, or an ancestor is shared as editor
--   'viewer' - an ancestor (or the node) is shared as viewer
CREATE FUNCTION node_access(nid UUID, uid UUID) RETURNS TEXT
LANGUAGE sql STABLE AS $$
    WITH RECURSIVE anc AS (
        SELECT id, parent_id FROM nodes WHERE id = nid
        UNION ALL
        SELECT n.id, n.parent_id FROM nodes n JOIN anc a ON n.id = a.parent_id
    )
    SELECT CASE
        WHEN s.kind = 'personal' AND s.owner_id = uid THEN 'owner'
        WHEN s.kind = 'family' THEN 'editor'
        ELSE (
            SELECT CASE
                WHEN bool_or(sh.permission = 'editor') THEN 'editor'
                WHEN count(*) > 0 THEN 'viewer'
            END
            FROM shares sh
            WHERE sh.user_id = uid AND sh.node_id IN (SELECT id FROM anc)
        )
    END
    FROM nodes x JOIN spaces s ON s.id = x.space_id
    WHERE x.id = nid
$$;

-- TRUE when neither the node nor any ancestor is in the trash.
CREATE FUNCTION node_live(nid UUID) RETURNS BOOLEAN
LANGUAGE sql STABLE AS $$
    WITH RECURSIVE anc AS (
        SELECT id, parent_id, trashed_at FROM nodes WHERE id = nid
        UNION ALL
        SELECT n.id, n.parent_id, n.trashed_at FROM nodes n JOIN anc a ON n.id = a.parent_id
    )
    SELECT NOT EXISTS (SELECT 1 FROM anc WHERE trashed_at IS NOT NULL)
$$;
