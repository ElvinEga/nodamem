# Nodamem

Nodamem is a minimal Rust workspace for a local-first AI memory engine.

## Crates

- `memory-core`: Shared domain types and core abstractions.
- `memory-store`: Storage layer interfaces and local persistence entry points.
- `memory-ingest`: Ingestion pipeline scaffolding for incoming memory events.
- `memory-retrieval`: Retrieval pipeline scaffolding for querying memory.
- `memory-lessons`: Learned patterns and distilled lessons scaffolding.
- `memory-personality`: Personality state and preference scaffolding.
- `memory-imagination`: Generative or speculative memory scaffolding.
- `memory-sleep`: Background consolidation and maintenance scaffolding.
- `agent-api`: High-level API surface for agents using the memory engine.

Everything is intentionally minimal and compiling; business logic is not implemented yet.

## Debugging

Structured `tracing` instrumentation is available across ingestion, retrieval, lessons, personality, consolidation, and store audit inspection.

Developer notes:

- Audit stored provenance with `StoreRepository::inspect_node_audit` and `StoreRepository::inspect_lesson_audit`.
- Enable `tracing` subscribers in the binary or tests to inspect admission decisions, retrieval scoring, reinforcement, trait updates, and consolidation changes.
- See [docs/debugging-graph-behavior.md](/home/snakeos/Development/rust/nodamem/docs/debugging-graph-behavior.md) for the recommended debugging workflow.
