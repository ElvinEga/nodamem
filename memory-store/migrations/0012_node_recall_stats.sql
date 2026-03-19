CREATE TABLE IF NOT EXISTS node_recall_stats (
    node_id TEXT PRIMARY KEY,
    recall_count INTEGER NOT NULL DEFAULT 0 CHECK (recall_count >= 0),
    last_recalled_at TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (node_id) REFERENCES nodes (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_node_recall_stats_recall_count
ON node_recall_stats (recall_count DESC, updated_at DESC);

CREATE TRIGGER IF NOT EXISTS trg_node_recall_stats_updated_at
AFTER UPDATE ON node_recall_stats
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE node_recall_stats
    SET updated_at = CURRENT_TIMESTAMP
    WHERE node_id = NEW.node_id;
END;
