CREATE TABLE IF NOT EXISTS node_action_events (
    id TEXT PRIMARY KEY NOT NULL,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL CHECK (event_type IN ('archived', 'unarchived')),
    reason TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_node_action_events_node_id
    ON node_action_events(node_id, created_at DESC);
