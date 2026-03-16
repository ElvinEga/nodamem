CREATE TABLE IF NOT EXISTS trait_events (
    id TEXT PRIMARY KEY,
    trait_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    delta REAL NOT NULL DEFAULT 0.0,
    reason TEXT,
    lesson_id TEXT,
    node_id TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (trait_id) REFERENCES trait_state (id) ON DELETE CASCADE,
    FOREIGN KEY (lesson_id) REFERENCES lessons (id) ON DELETE SET NULL,
    FOREIGN KEY (node_id) REFERENCES nodes (id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_trait_events_trait_id ON trait_events (trait_id);
CREATE INDEX IF NOT EXISTS idx_trait_events_created_at ON trait_events (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_trait_events_lesson_id ON trait_events (lesson_id);
CREATE INDEX IF NOT EXISTS idx_trait_events_node_id ON trait_events (node_id);

CREATE TRIGGER IF NOT EXISTS trg_trait_events_updated_at
AFTER UPDATE ON trait_events
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE trait_events
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;

