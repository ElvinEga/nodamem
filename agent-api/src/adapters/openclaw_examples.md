# OpenClaw Usage Examples

## Continue a prior architecture discussion

Situation:
The user says, "Continue the architecture discussion from yesterday about migration strategy."

Recommended action:

1. Call `recall_context`.
2. Use the returned verified summaries in the answer.
3. If one recalled node looks central and nearby context is needed, call `get_neighbors`.

## User reveals a stable preference

Situation:
The user says, "I want design docs to always include rollout notes and rollback steps."

Recommended action:

1. Answer the user directly.
2. After answering, call `propose_memory` because this is durable preference-like information that could matter later.

## Agent observes a reusable pattern

Situation:
Across several tasks, explicit rollout notes consistently reduce planning confusion.

Recommended action:

1. Call `propose_lesson`.
2. Let Nodamem decide whether to create, reinforce, refine, or contradiction-hook the lesson.

## Task succeeds or fails

Situation:
The agent completes a planning task successfully and the user accepts the result.

Recommended action:

1. Call `record_outcome` with success, usefulness, prediction correctness, and user acceptance signals.

If the task fails or the user rejects the result:

1. Still call `record_outcome`.
2. Include the failure and rejection signals so trait updates remain grounded in validated outcomes.

## Planning needs speculative help

Situation:
The agent needs possible next-step options for release planning and wants grounded hypotheticals.

Recommended action:

1. Call `recall_context`.
2. Then call `generate_imagined_scenarios`.
3. Present the result as hypothetical planning support, not verified memory.
