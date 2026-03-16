CREATE TABLE IF NOT EXISTS imagined_nodes (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN (
        'proposed',
        'simulated',
        'reviewed',
        'accepted_as_hypothesis',
        'rejected'
    )),
    title TEXT NOT NULL,
    premise TEXT NOT NULL,
    narrative TEXT NOT NULL,
    source_node_ids_json TEXT NOT NULL DEFAULT '[]',
    lesson_ids_json TEXT NOT NULL DEFAULT '[]',
    predicted_outcomes_json TEXT NOT NULL DEFAULT '[]',
    confidence REAL NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_imagined_nodes_status ON imagined_nodes (status);
CREATE INDEX IF NOT EXISTS idx_imagined_nodes_updated_at ON imagined_nodes (updated_at DESC);

CREATE TRIGGER IF NOT EXISTS trg_imagined_nodes_updated_at
AFTER UPDATE ON imagined_nodes
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE imagined_nodes
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;

