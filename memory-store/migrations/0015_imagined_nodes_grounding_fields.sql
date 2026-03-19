ALTER TABLE imagined_nodes ADD COLUMN kind TEXT NOT NULL DEFAULT 'future_need_prediction';
ALTER TABLE imagined_nodes ADD COLUMN self_model_snapshot_json TEXT;
