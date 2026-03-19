ALTER TABLE trait_events ADD COLUMN outcome_id TEXT;
ALTER TABLE trait_events ADD COLUMN trait_type TEXT;
ALTER TABLE trait_events ADD COLUMN previous_strength REAL NOT NULL DEFAULT 0.0;
ALTER TABLE trait_events ADD COLUMN updated_strength REAL NOT NULL DEFAULT 0.0;

CREATE INDEX IF NOT EXISTS idx_trait_events_outcome_id ON trait_events (outcome_id);
