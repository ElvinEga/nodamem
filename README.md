# Nodamem

Nodamem is a Rust workspace for a local-first AI memory engine.

The project is organized around a memory graph with explicit boundaries between:

- verified memory
- distilled lessons
- trait state
- imagined scenarios
- background consolidation

The storage default is embedded local libSQL. Optional Turso Cloud sync hooks exist at the
storage boundary, but the core memory logic remains local-first and storage-agnostic.

## Crates

- `memory-core`: Shared domain types and core abstractions.
- `memory-store`: Storage layer interfaces and local persistence entry points.
- `memory-ingest`: Ingestion pipeline scaffolding for incoming memory events.
- `memory-retrieval`: Retrieval pipeline scaffolding for querying memory.
- `memory-lessons`: Learned patterns and distilled lessons scaffolding.
- `memory-personality`: Personality state and preference scaffolding.
- `memory-imagination`: Generative or speculative memory scaffolding.
- `memory-sleep`: Background consolidation and maintenance scaffolding.
- `agent-api`: High-level API surface, DTOs, HTTP transport, and adapter layers for agents using the memory engine.
- `nodamem-app`: Runnable binary crate for starting Nodamem.

The workspace is intentionally small, but the current codebase includes:

- deterministic ingestion and admission scaffolding
- retrieval packet construction
- lesson reinforcement and contradiction handling
- trait updates from validated outcomes
- hypothetical imagination kept separate from verified memory
- consolidation jobs
- structured tracing and audit inspection support
- internal agent service methods for memory operations
- HTTP JSON endpoints for agent runtimes
- OpenClaw adapter modules with compact tool-facing outputs
- OpenClaw runtime instructions and usage examples

## Agent Integration

Nodamem exposes an agent-facing integration layer in `agent-api` while keeping the core memory
engine independent of any specific runtime.

Current integration features:

- internal service methods for:
  - `recall_context`
  - `get_neighbors`
  - `propose_memory`
  - `propose_lesson`
  - `record_outcome`
  - `generate_imagined_scenarios`
- serde request and response DTOs
- axum-based HTTP JSON routes for the same operations
- adapter layers for runtime-specific integrations such as OpenClaw

### HTTP API

Available JSON endpoints:

- `POST /recall-context`
- `POST /get-neighbors`
- `POST /propose-memory`
- `POST /propose-lesson`
- `POST /record-outcome`
- `POST /generate-imagined-scenarios`

The transport layer delegates into service methods and does not expose raw database tables.

### OpenClaw Support

OpenClaw-specific code is isolated under `agent-api/src/adapters`.

Files:

- `agent-api/src/adapters/openclaw.rs`
- `agent-api/src/adapters/openclaw_types.rs`
- `agent-api/src/adapters/openclaw_tools.rs`
- `agent-api/src/adapters/openclaw_instructions.md`
- `agent-api/src/adapters/openclaw_examples.md`

OpenClaw support includes:

- compact tool-facing schemas
- tool descriptions for when and when not to call each tool
- guidance for system-prompt/runtime instructions
- realistic usage examples
- compact adapter outputs that keep imagined scenarios labeled hypothetical

## Debugging

Structured `tracing` instrumentation is available across ingestion, retrieval, lessons, personality, consolidation, and store audit inspection.

Developer notes:

- Audit stored provenance with `StoreRepository::inspect_node_audit` and `StoreRepository::inspect_lesson_audit`.
- Enable `tracing` subscribers in the binary or tests to inspect admission decisions, retrieval scoring, reinforcement, trait updates, and consolidation changes.
- See [docs/debugging-graph-behavior.md](/home/snakeos/Development/rust/nodamem/docs/debugging-graph-behavior.md) for the recommended debugging workflow.

## Storage

Nodamem defaults to embedded local storage through `memory-store`.

- Local-first mode is the default and does not require network access.
- Optional Turso Cloud sync hooks are available through `NODAMEM_TURSO_SYNC_ENABLED`, `NODAMEM_TURSO_SYNC_REQUIRED`, `TURSO_DATABASE_URL`, `TURSO_AUTH_TOKEN`, and `NODAMEM_TURSO_READ_YOUR_WRITES`.
- If sync is requested but incomplete, Nodamem stays in offline/local-only mode.
- If sync is enabled and configured but remote initialization fails, Nodamem falls back to local-only mode unless `NODAMEM_TURSO_SYNC_REQUIRED=true`.
- Local bootstrap migrations run only for the local-only backend. Synced mode assumes schema management is handled outside the embedded bootstrap path.
- See [docs/turso-sync.md](/home/snakeos/Development/rust/nodamem/docs/turso-sync.md) for the storage and sync behavior.

### Local-only default

With no Turso environment variables set, Nodamem opens:

- local embedded database path: `data/nodamem.db`
- no network dependency
- local schema bootstrap

### Optional sync configuration

Environment variables:

- `NODAMEM_DB_PATH`: override the local embedded database path
- `NODAMEM_TURSO_SYNC_ENABLED`: enable the synced database path
- `NODAMEM_TURSO_SYNC_REQUIRED`: fail startup if sync cannot initialize
- `TURSO_DATABASE_URL`: Turso database URL
- `TURSO_AUTH_TOKEN`: Turso auth token
- `NODAMEM_TURSO_READ_YOUR_WRITES`: optional libsql synced-database setting, default `true`

## Running

Start the binary crate:

```bash
cargo run -p nodamem-app
```

Current startup behavior:

- prints `Nodamem starting...`
- loads store configuration from environment
- opens the local-first store
- reports the resolved startup mode and database path

## Testing

Check the full workspace:

```bash
cargo check --workspace
```

Run all tests:

```bash
cargo test
```

Run a single crate:

```bash
cargo test -p agent-api
cargo test -p memory-store
```

Useful verification commands:

```bash
cargo run -p nodamem-app
cargo check -p agent-api
```

## Build

Build a debug binary:

```bash
cargo build -p nodamem-app
```

The binary will be at:

```bash
target/debug/nodamem-app
```

## Release

Build a release binary:

```bash
cargo build --release -p nodamem-app
```

The release binary will be at:

```bash
target/release/nodamem-app
```

Run the release binary:

```bash
./target/release/nodamem-app
```

Notes:

- the current binary is a startup executable, not a long-running background daemon
- if background execution is needed later, `nodamem-app` will need a server loop, worker loop, or process-manager deployment model
