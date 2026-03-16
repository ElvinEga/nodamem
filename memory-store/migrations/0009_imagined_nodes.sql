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
    basis_source_node_ids_json TEXT NOT NULL DEFAULT '[]',
    basis_lesson_ids_json TEXT NOT NULL DEFAULT '[]',
    active_goal_node_ids_json TEXT NOT NULL DEFAULT '[]',
    trait_snapshot_json TEXT NOT NULL DEFAULT '[]',
    predicted_outcomes_json TEXT NOT NULL DEFAULT '[]',
    plausibility_score REAL NOT NULL DEFAULT 0.0 CHECK (plausibility_score >= 0.0 AND plausibility_score <= 1.0),
    novelty_score REAL NOT NULL DEFAULT 0.0 CHECK (novelty_score >= 0.0 AND novelty_score <= 1.0),
    usefulness_score REAL NOT NULL DEFAULT 0.0 CHECK (usefulness_score >= 0.0 AND usefulness_score <= 1.0),
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
