CREATE TABLE IF NOT EXISTS lessons (
    id TEXT PRIMARY KEY,
    lesson_type TEXT NOT NULL CHECK (lesson_type IN (
        'user_lesson',
        'system_lesson',
        'task_lesson',
        'strategy_lesson',
        'domain_lesson',
        'personality_lesson'
    )),
    status TEXT NOT NULL CHECK (status IN (
        'candidate',
        'active',
        'reinforced',
        'contradicted',
        'archived',
        'pruned'
    )),
    title TEXT NOT NULL,
    statement TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),
    evidence_count INTEGER NOT NULL DEFAULT 0 CHECK (evidence_count >= 0),
    reinforcement_count INTEGER NOT NULL DEFAULT 0 CHECK (reinforcement_count >= 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_lessons_lesson_type ON lessons (lesson_type);
CREATE INDEX IF NOT EXISTS idx_lessons_status ON lessons (status);
CREATE INDEX IF NOT EXISTS idx_lessons_updated_at ON lessons (updated_at DESC);

CREATE TRIGGER IF NOT EXISTS trg_lessons_updated_at
AFTER UPDATE ON lessons
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE lessons
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;
