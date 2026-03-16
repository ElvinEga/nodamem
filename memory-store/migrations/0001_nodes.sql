CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    node_type TEXT NOT NULL CHECK (node_type IN (
        'episodic',
        'semantic',
        'lesson',
        'entity',
        'goal',
        'preference',
        'trait',
        'prediction',
        'prediction_error',
        'checkpoint',
        'imagined',
        'self_model'
    )),
    status TEXT NOT NULL CHECK (status IN (
        'candidate',
        'active',
        'reinforced',
        'contradicted',
        'archived',
        'pruned'
    )),
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    content TEXT,
    tags_json TEXT NOT NULL DEFAULT '[]',
    confidence REAL NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
    importance REAL NOT NULL DEFAULT 0.0 CHECK (importance >= 0.0),
    last_accessed_at TEXT,
    source_event_id TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_nodes_node_type ON nodes (node_type);
CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes (status);
CREATE INDEX IF NOT EXISTS idx_nodes_updated_at ON nodes (updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_nodes_last_accessed_at ON nodes (last_accessed_at DESC);

CREATE TRIGGER IF NOT EXISTS trg_nodes_updated_at
AFTER UPDATE ON nodes
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE nodes
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;

