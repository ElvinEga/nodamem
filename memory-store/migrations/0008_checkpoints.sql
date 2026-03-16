CREATE TABLE IF NOT EXISTS checkpoints (
    id TEXT PRIMARY KEY,
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
    window_started_at TEXT,
    window_ended_at TEXT,
    node_ids_json TEXT NOT NULL DEFAULT '[]',
    lesson_ids_json TEXT NOT NULL DEFAULT '[]',
    trait_ids_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_updated_at ON checkpoints (updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_checkpoints_window_started_at ON checkpoints (window_started_at DESC);

CREATE TRIGGER IF NOT EXISTS trg_checkpoints_updated_at
AFTER UPDATE ON checkpoints
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE checkpoints
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;

