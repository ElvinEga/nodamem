CREATE TABLE IF NOT EXISTS node_sources (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_ref TEXT NOT NULL,
    excerpt TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (node_id) REFERENCES nodes (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_node_sources_node_id ON node_sources (node_id);
CREATE INDEX IF NOT EXISTS idx_node_sources_source_kind ON node_sources (source_kind);

CREATE TRIGGER IF NOT EXISTS trg_node_sources_updated_at
AFTER UPDATE ON node_sources
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE node_sources
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;

