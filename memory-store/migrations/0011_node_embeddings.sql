CREATE TABLE IF NOT EXISTS node_embeddings (
    node_id TEXT PRIMARY KEY,
    embedding_model TEXT NOT NULL,
    embedding_dimensions INTEGER NOT NULL CHECK (embedding_dimensions > 0),
    embedding BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (node_id) REFERENCES nodes (id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_node_embeddings_model ON node_embeddings (embedding_model);
CREATE INDEX IF NOT EXISTS idx_node_embeddings_updated_at ON node_embeddings (updated_at DESC);

CREATE TRIGGER IF NOT EXISTS trg_node_embeddings_updated_at
AFTER UPDATE ON node_embeddings
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE node_embeddings
    SET updated_at = CURRENT_TIMESTAMP
    WHERE node_id = NEW.node_id;
END;
