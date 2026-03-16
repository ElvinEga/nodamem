CREATE TABLE IF NOT EXISTS trait_state (
    id TEXT PRIMARY KEY,
    trait_type TEXT NOT NULL CHECK (trait_type IN (
        'communication_style',
        'planning_style',
        'risk_tolerance',
        'creativity',
        'reliability',
        'curiosity',
        'collaboration'
    )),
    status TEXT NOT NULL CHECK (status IN (
        'candidate',
        'active',
        'reinforced',
        'contradicted',
        'archived',
        'pruned'
    )),
    label TEXT NOT NULL,
    description TEXT NOT NULL,
    strength REAL NOT NULL DEFAULT 0.0,
    confidence REAL NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
    supporting_lesson_ids_json TEXT NOT NULL DEFAULT '[]',
    supporting_node_ids_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_trait_state_trait_type ON trait_state (trait_type);
CREATE INDEX IF NOT EXISTS idx_trait_state_updated_at ON trait_state (updated_at DESC);

CREATE TRIGGER IF NOT EXISTS trg_trait_state_updated_at
AFTER UPDATE ON trait_state
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE trait_state
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;

