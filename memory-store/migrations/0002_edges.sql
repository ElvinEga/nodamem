CREATE TABLE IF NOT EXISTS edges (
    id TEXT PRIMARY KEY,
    edge_type TEXT NOT NULL CHECK (edge_type IN (
        'related_to',
        'derived_from',
        'supports',
        'contradicts',
        'same_topic',
        'same_project',
        'teaches',
        'strengthens',
        'weakens',
        'predicts',
        'corrected_by',
        'inspired_by',
        'part_of',
        'summarized_as',
        'applies_to'
    )),
    from_node_id TEXT NOT NULL,
    to_node_id TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (from_node_id) REFERENCES nodes (id) ON DELETE CASCADE,
    FOREIGN KEY (to_node_id) REFERENCES nodes (id) ON DELETE CASCADE,
    CHECK (from_node_id <> to_node_id)
);

CREATE INDEX IF NOT EXISTS idx_edges_from_node_id ON edges (from_node_id);
CREATE INDEX IF NOT EXISTS idx_edges_to_node_id ON edges (to_node_id);
CREATE INDEX IF NOT EXISTS idx_edges_edge_type ON edges (edge_type);
CREATE INDEX IF NOT EXISTS idx_edges_from_to ON edges (from_node_id, to_node_id);

CREATE TRIGGER IF NOT EXISTS trg_edges_updated_at
AFTER UPDATE ON edges
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE edges
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;

