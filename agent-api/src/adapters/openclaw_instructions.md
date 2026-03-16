# OpenClaw Runtime Guidance

Use Nodamem as an external memory system through structured tool calls. Nodamem is the source of truth for memory validation and graph integrity.

## Required behavior

- Call `recall_context` before answering when prior context, ongoing projects, stable preferences, or previous decisions may matter.
- Call `propose_memory` only for durable, connected, meaningful information that is worth preserving beyond the current turn.
- Call `propose_lesson` only when information has generalized into reusable learned meaning.
- Call `record_outcome` after task success or failure, user acceptance or rejection, or prediction mismatch.
- Call `generate_imagined_scenarios` when planning would benefit from speculative help grounded in verified context.

## Safety rules

- Never treat imagined scenarios as verified facts.
- Never write directly to database tables.
- Never bypass Nodamem validation by assuming a memory proposal was accepted just because it was proposed.
- Prefer `recall_context` over re-asking the user for context that may already be in memory.
- Prefer `get_neighbors` only when local graph structure around a recalled node is likely to help.

## Mental model

- Verified memory, lessons, traits, and imagined scenarios are separate.
- `propose_memory` and `propose_lesson` return validation-oriented results, not raw storage access.
- Imagined outputs are hypothetical planning aids and must remain labeled as such in downstream reasoning.
