CREATE TABLE IF NOT EXISTS lesson_sources (
    id TEXT PRIMARY KEY,
    lesson_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    source_role TEXT NOT NULL CHECK (source_role IN ('supporting', 'contradicting')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (lesson_id) REFERENCES lessons (id) ON DELETE CASCADE,
    FOREIGN KEY (node_id) REFERENCES nodes (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_lesson_sources_lesson_id ON lesson_sources (lesson_id);
CREATE INDEX IF NOT EXISTS idx_lesson_sources_node_id ON lesson_sources (node_id);
CREATE INDEX IF NOT EXISTS idx_lesson_sources_source_role ON lesson_sources (source_role);

CREATE TRIGGER IF NOT EXISTS trg_lesson_sources_updated_at
AFTER UPDATE ON lesson_sources
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE lesson_sources
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;

