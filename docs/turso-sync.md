# Optional Turso Sync

Nodamem remains local-first by default.

The storage layer always opens an embedded local database unless sync is explicitly enabled with a complete Turso configuration. This keeps offline and local-only behavior unchanged for normal development and single-device use.

## Environment variables

- `NODAMEM_DB_PATH`: local embedded database path. Default: `data/nodamem.db`
- `NODAMEM_TURSO_SYNC_ENABLED`: enables the future sync path when set to `true`
- `TURSO_DATABASE_URL`: remote Turso database URL
- `TURSO_AUTH_TOKEN`: remote Turso auth token
- `NODAMEM_TURSO_READ_YOUR_WRITES`: optional libsql synced-database setting. Default: `true`

## Behavior

- If `NODAMEM_TURSO_SYNC_ENABLED` is not set or is `false`, Nodamem opens a local embedded database.
- If sync is enabled but `TURSO_DATABASE_URL` or `TURSO_AUTH_TOKEN` is missing, Nodamem logs a warning and stays in local-only mode.
- If all sync settings are present, the storage bootstrap uses the current `libsql::Builder::new_synced_database(...)` integration point.

## Design boundary

Sync is intentionally isolated inside `memory-store`.

- memory graph logic does not branch on sync mode
- ingestion, retrieval, lessons, personality, imagination, and consolidation stay storage-agnostic
- future sync behavior can evolve without changing the core memory subsystems

## Notes

This is a configuration hook for future Turso Cloud sync support, not a change to Nodamem’s local-first default. Embedded local storage is still the primary mode and the safe fallback mode.
