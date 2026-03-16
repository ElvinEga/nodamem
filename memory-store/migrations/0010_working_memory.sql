CREATE TABLE IF NOT EXISTS working_memory (
    id TEXT PRIMARY KEY,
    scope_key TEXT NOT NULL UNIQUE,
    session_id TEXT,
    task_ref TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}',
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_working_memory_session_id ON working_memory (session_id);
CREATE INDEX IF NOT EXISTS idx_working_memory_task_ref ON working_memory (task_ref);
CREATE INDEX IF NOT EXISTS idx_working_memory_expires_at ON working_memory (expires_at);
CREATE INDEX IF NOT EXISTS idx_working_memory_updated_at ON working_memory (updated_at DESC);

CREATE TRIGGER IF NOT EXISTS trg_working_memory_updated_at
AFTER UPDATE ON working_memory
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE working_memory
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = NEW.id;
END;

