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

