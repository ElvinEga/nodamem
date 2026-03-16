# Debugging Graph Behavior

Nodamem uses structured `tracing` events at the main graph decision points so developers can inspect behavior without adding temporary `println!` calls.

## What is logged

- `memory-ingest`: candidate extraction summaries and admission decisions with score components, duplicate similarity, chosen action, and reason.
- `memory-retrieval`: retrieval source sizes plus the top ranked node scoring inputs for each recall.
- `memory-lessons`: new lesson creation, refinement, contradiction hooks, and reinforcement decisions.
- `memory-personality`: validated outcome-driven trait updates with the applied signal and before/after strengths.
- `memory-sleep`: checkpoint creation, duplicate merges, edge decay, archival, lesson reinforcement, and reconsolidation changes.
- `memory-store`: audit trail construction for nodes and lessons.

The intent is useful diagnostics with bounded volume:

- admission decisions log once per candidate node
- retrieval scoring logs only the top ranked nodes
- consolidation logs only when a job changes state
- audit inspection logs only when explicitly requested

## Enabling tracing

Add a subscriber in the binary, test harness, or integration tool that drives Nodamem:

```rust
use tracing_subscriber::{EnvFilter, fmt};

fn init_tracing() {
    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("memory_ingest=info,memory_retrieval=debug,memory_lessons=info,memory_personality=info,memory_sleep=info,memory_store=debug")),
        )
        .with_target(true)
        .compact()
        .try_init();
}
```

Recommended defaults:

- use `info` for ingest, lessons, personality, and sleep when diagnosing behavior changes
- use `debug` for retrieval when investigating ranking
- use `debug` for store audit inspection

## Auditing why a record exists

`StoreRepository` exposes two audit-oriented inspection methods:

- `inspect_node_audit(node_id)`
- `inspect_lesson_audit(lesson_id)`

These return store-backed provenance views that summarize:

- graph links
- source event ids where available
- lesson evidence links
- trait dependencies
- checkpoint inclusion

Use them when a node or lesson seems surprising and you need a human-readable explanation without manually querying every table.

## Practical workflow

1. Reproduce the behavior with tracing enabled.
2. Check `memory-ingest` admission logs to confirm whether the node was created, merged, attached, or rejected.
3. Check `memory-retrieval` scoring logs to see why a node ranked highly or was omitted.
4. Check `memory-lessons` and `memory-personality` logs if the behavior came from reinforcement or trait drift.
5. Run `inspect_node_audit` or `inspect_lesson_audit` to confirm the stored provenance matches the runtime logs.
6. For long-term drift, inspect `memory-sleep` job reports to see whether consolidation altered confidence, edges, or lesson strength.
