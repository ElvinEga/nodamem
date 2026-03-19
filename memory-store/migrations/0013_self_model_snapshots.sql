CREATE TABLE IF NOT EXISTS self_model_snapshots (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL,
    recurring_strengths_json TEXT NOT NULL,
    user_interaction_preferences_json TEXT NOT NULL,
    behavioral_tendencies_json TEXT NOT NULL,
    active_domains_json TEXT NOT NULL,
    supporting_lesson_ids_json TEXT NOT NULL,
    supporting_trait_ids_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_self_model_snapshots_version
    ON self_model_snapshots (version);

CREATE INDEX IF NOT EXISTS idx_self_model_snapshots_updated_at
    ON self_model_snapshots (updated_at DESC);
