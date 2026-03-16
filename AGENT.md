# AGENT.md

> Project: **Nodamem**

## Purpose

This document defines how AI agents interact with **Nodamem**, the local-first memory system.

The memory system is not a chat log and not a flat key-value store. It is a connected memory graph inspired by human memory and tools like Obsidian:

* memories are stored as nodes
* relationships are stored as typed edges
* lessons are extracted from experiences
* personality develops from repeated validated lessons
* imagination is generated from connected memory clusters
* weak isolated memories are usually not kept

Agents do **not** write directly to the database. All interaction happens through a controlled memory engine.

---

## Core Principles

### 1. The memory system owns truth

Agents may propose memories, lessons, or imagined scenarios, but they do not commit them directly.

The memory engine validates:

* whether a candidate is worth saving
* whether it connects to existing graph structure
* whether it duplicates existing knowledge
* whether it contradicts validated lessons
* whether it should strengthen an existing node instead of creating a new one

### 2. Connected memory only

A memory should usually be saved only if:

* it connects to existing nodes, or
* it is important enough to become a new root node

Low-value isolated noise should remain in short-term working memory or be discarded.

### 3. Separate reality, lessons, traits, and imagination

The system must strictly separate:

* **episodic memory**: what happened
* **semantic/lesson memory**: what was learned
* **personality traits**: how the agent tends to behave
* **imagined nodes**: hypothetical scenarios, simulations, forecasts

Imagined nodes must never be treated as verified facts.

### 4. Memory is dynamic

Memory is not frozen forever.

The system supports:

* reconsolidation on recall
* strengthening from repeated evidence
* weakening from contradictions
* decay of weak edges
* pruning or archiving of stale isolated nodes

### 5. The agent reads memory packets, not raw database state

Agents should receive curated context packets, not full graph dumps.

A memory packet should contain only the most relevant items for the current task.

---

## System Roles

### Memory Engine

The memory engine is the governor and source of memory integrity.

It is responsible for:

* memory admission
* node and edge creation
* duplicate detection
* lesson validation
* checkpoint generation
* memory retrieval
* graph neighborhood expansion
* trait updates
* prediction error handling
* decay and pruning
* imagination generation
* provenance tracking

### AI Agent

The agent is a consumer and proposer.

It is responsible for:

* asking for relevant context
* using that context to reason
* proposing candidate memories and lessons
* reporting outcomes and prediction errors
* optionally requesting imagined scenarios for planning

The agent does not directly mutate durable memory state.

---

## Memory Model

### Node Types

The graph supports these first-class node types:

* `episodic` — specific events or interactions
* `semantic` — generalized knowledge
* `lesson` — distilled reusable learning
* `entity` — person, project, tool, topic, place, concept
* `goal` — desired future state
* `preference` — stable preference or style
* `trait` — personality tendency
* `prediction` — expected outcome or hypothesis
* `prediction_error` — gap between expected and actual result
* `checkpoint` — summary of a time window or cluster
* `imagined` — hypothetical future, simulation, counterfactual, forecast
* `self_model` — summary of the agent’s longer-term identity and strengths

### Edge Types

The graph supports typed relationships:

* `related_to`
* `derived_from`
* `supports`
* `contradicts`
* `same_topic`
* `same_project`
* `teaches`
* `strengthens`
* `weakens`
* `predicts`
* `corrected_by`
* `inspired_by`
* `part_of`
* `summarized_as`
* `applies_to`

---

## Agent Interaction Model

Agents interact with the memory system through a small structured API.

### Required Read Operations

#### `recall_context`

Returns the most relevant memory packet for the current task.

Input:

* current user message or task
* optional session id
* optional topic or project id
* optional desired scope

Output:

* top relevant nodes
* linked lessons
* active goals
* current preference signals
* small personality snapshot
* optional checkpoint summary
* optional nearby connected nodes

This is the default read operation used before reasoning.

#### `get_neighbors`

Returns nearby graph nodes for a given node or memory cluster.

Use this when the agent needs more local graph structure around an important concept.

#### `get_working_set`

Returns temporary active state for the current session or task.

This includes:

* currently pinned nodes
* recent task state
* unresolved subgoals
* recent candidate ideas

### Required Write-Proposal Operations

#### `propose_memory`

The agent proposes a memory candidate.

The proposal should include:

* candidate type
* short summary
* source event or source messages
* suggested connected nodes
* importance estimate
* confidence estimate
* optional entities

The engine decides whether to:

* store it as a new node
* merge into an existing node
* attach as evidence to an existing node
* discard it

#### `propose_lesson`

The agent proposes a lesson derived from one or more memories.

The proposal should include:

* lesson content
* lesson type
* supporting source node ids
* confidence estimate
* whether it strengthens or contradicts existing lessons

The engine validates the lesson before storing or reinforcing it.

#### `record_outcome`

Used after a response, action, or task.

This lets the agent report:

* success or failure
* usefulness
* user acceptance or rejection
* prediction correctness
* surprising outcome

This feeds reinforcement learning inside the memory engine.

#### `pin_working_set`

Marks nodes as temporarily important for the current task.

Pinned working memory is not automatically durable memory.

### Optional Higher-Level Operation

#### `generate_imagined_scenarios`

Requests hypothetical scenarios based on a memory cluster, goal, or active topic.

The engine returns:

* candidate imagined nodes
* basis nodes that inspired them
* plausibility score
* novelty score
* usefulness score
* confidence or uncertainty

Imagined outputs are always marked hypothetical.

---

## Recommended Agent Loop

### Before responding

1. Read the user message or task.
2. Call `recall_context`.
3. Use the returned packet to reason.
4. Optionally call `get_neighbors` if the topic needs more local graph expansion.
5. Optionally call `generate_imagined_scenarios` if planning or brainstorming would help.

### During reasoning

The agent should:

* prefer validated lessons over raw noisy events
* use checkpoint summaries when available
* avoid overloading itself with too many nodes
* treat imagined content as hypothesis, not fact

### After responding

The agent should:

* propose durable memories only when useful
* propose lessons if reusable meaning emerged
* record expected outcome when appropriate

### After feedback or completion

The system should:

* call `record_outcome`
* update lesson confidence
* update edge strengths
* log prediction errors
* update personality traits slowly

---

## Memory Packets

Agents should never receive the full graph by default.

A memory packet should be small and task-oriented.

### Default packet contents

A good packet normally contains:

* 3 to 5 core nodes
* 2 to 3 linked neighbor nodes
* 1 to 2 validated lessons
* 1 checkpoint summary
* 1 personality snapshot
* active goals or preferences if relevant

### Retrieval priorities

The engine should rank by a hybrid score using:

* semantic similarity
* graph relationship strength
* importance
* recency
* confidence
* centrality or hub relevance

Approximate ranking idea:

```text
score = semantic_similarity * 0.35
      + edge_strength * 0.20
      + importance * 0.15
      + recency * 0.10
      + confidence * 0.10
      + centrality * 0.10
```

---

## Memory Admission Rules

A candidate memory should usually be accepted only if:

* it is connected to existing nodes, or
* it is highly important and deserves a new root node, or
* it bridges two previously separate important clusters, or
* it strengthens an existing lesson or preference significantly

A candidate memory should usually be rejected or deferred if:

* it is isolated and low-value
* it is redundant with an existing node
* it is temporary noise
* it does not change future behavior or understanding

### Example admission score

```text
admission_score =
  connectedness * 0.35 +
  usefulness * 0.25 +
  recurrence * 0.15 +
  novelty * 0.15 +
  importance * 0.10
```

Hard rule:

```text
must_have_connection OR must_be_high_importance
```

---

## Lessons

Each important memory may produce one or more lessons.

### Lesson purpose

A lesson captures what the system learned from an experience.

Example:

* memory: user preferred practical implementation advice over theory
* lesson: for this user, concrete implementation guidance is usually more effective than abstract framing

### Lesson types

Suggested lesson categories:

* `user_lesson`
* `system_lesson`
* `task_lesson`
* `domain_lesson`
* `strategy_lesson`
* `personality_lesson`

### Lesson validation

Every lesson should track:

* confidence
* evidence count
* supporting source nodes
* contradiction signals
* last confirmed time

Repeated supporting evidence strengthens the lesson. Contradictory evidence weakens or refines it.

---

## Personality

Personality is not roleplay. It is a slow-moving behavioral profile derived from repeated validated lessons and outcomes.

### Trait examples

Possible trait dimensions:

* curiosity
* caution
* verbosity
* novelty_seeking
* evidence_reliance
* abstraction_preference
* proactivity
* empathy_style
* practicality

### Update rule

Traits should update slowly, not abruptly.

Approximate idea:

```text
new_trait = old_trait * 0.95 + observed_signal * 0.05
```

Traits should be updated from:

* repeated lesson reinforcement
* success or failure outcomes
* prediction errors
* user acceptance patterns

Agents must not mutate trait values directly.

---

## Prediction and Prediction Error

The system should support expectation and correction.

### Prediction nodes

Before or during planning, the agent or engine may create prediction nodes such as:

* likely user preference
* expected task outcome
* expected usefulness of a strategy
* likely follow-up need

### Prediction error nodes

After results arrive, the gap between expected and actual should be stored as a prediction error.

This helps the system learn:

* what strategies work
* what assumptions failed
* what lessons should be updated
* how personality should shift

---

## Imagination

Imagination is grounded scenario synthesis, not random invention.

### Inputs

Imagination is built from:

* connected memory clusters
* lessons
* active goals
* current personality snapshot

### Outputs

The engine may generate:

* hypothetical future scenarios
* possible next user needs
* alternative plans
* counterfactuals
* feature ideas

All imagined content must include:

* basis source nodes
* plausibility score
* novelty score
* usefulness score
* uncertainty
* hypothetical status

### Safety rule

Imagined nodes must never automatically become facts.

They may influence planning, but only validated real outcomes can upgrade them into stronger knowledge.

---

## Working Memory

Working memory is temporary state for current reasoning.

It may include:

* current task focus
* current node neighborhood
* active subgoals
* temporary hypotheses
* recent tool outputs
* pinned context

Working memory should expire or be replaced over time.

Durable memory should only be created through the admission pipeline.

---

## Consolidation and Sleep

The system should run periodic offline consolidation jobs.

### Consolidation responsibilities

* replay recent salient nodes
* extract or reinforce lessons
* create checkpoint summaries
* merge near-duplicate nodes
* strengthen repeated edges
* weaken unused edges
* prune weak isolated nodes
* refresh self-model summaries
* generate a limited number of imagined scenarios

This is the main place where the system becomes more coherent over time.

---

## Forgetting and Decay

A healthy graph must not grow forever without structure.

### Decay rules

* weak edges decay over time
* stale isolated nodes may be archived or deleted
* repeated nodes may merge into summaries
* low-salience leaves may disappear unless later reinforced

### Preserve meaning, not clutter

The engine should prefer:

* summary checkpoints
* strong lessons
* strong entities
* useful preferences
* dense meaningful clusters

over large volumes of weak raw episodes

---

## Multi-Agent Use

If multiple agents use the system later, the recommended model is:

### Shared long-term graph

Shared:

* entities
* goals
* lessons
* checkpoints
* strong semantic knowledge
* project graph

### Agent-local working memory

Private per agent:

* temporary hypotheses
* active task context
* local reasoning artifacts
* short-lived strategy choices

### Permissions

Different agents may have different write rights, for example:

* research agent proposes evidence nodes
* planner agent proposes goal or strategy nodes
* social agent proposes preference or tone lessons
* consolidation agent manages sleep jobs and checkpointing

All writes still pass through validation.

---

## What Agents Must Never Do

Agents must never:

* write directly to database tables
* mark imagined nodes as verified facts
* update personality traits directly
* create arbitrary edge explosions
* bypass provenance requirements
* delete durable memory without engine policy
* rewrite validated lessons without evidence

---

## Storage and Deployment Choice

The system will start with **Turso Database (Embedded)** as the primary storage model.

### Why Turso Database (Embedded) is the default

The Nodamem system is a structured local-first graph database, not just a file workspace.

It needs:

* relational storage for nodes, edges, lessons, traits, checkpoints, and predictions
* vector search for semantic recall
* local-first reads and writes
* optional synchronization later
* strong support for a Rust runtime

Turso Database (Embedded) is the right default because it supports local-first database behavior while preserving a path to sync with Turso Cloud later.

### Why AgentFS is not the core memory layer

AgentFS is useful for sandboxed agent workspaces and filesystem-like agent operations.

It is a good future companion when agents need:

* isolated workspaces
* copy-on-write file/session behavior
* auditable file changes
* shared agent sessions around files and tools

However, AgentFS is not the primary choice for the long-term semantic memory graph.

The memory graph should remain in Turso Database (Embedded).

### Recommended split

Use:

* **Turso Database (Embedded)** for the Nodamem engine and graph store
* **AgentFS later** only if the product needs agent file sandboxes or collaborative workspace sessions

### Local-first storage principle

The first version should work fully with local embedded storage.

Possible later additions:

* sync to Turso Cloud
* backup and restore
* multi-device memory replication
* remote shared graphs

The first implementation should not depend on cloud connectivity.

---

## Rust Implementation Guidance

The memory system is expected to be implemented in Rust.

### Recommended storage choice in Rust

Use Rust with **Turso Database (Embedded)** as the persistence layer.

This means:

* local embedded database in the app runtime
* memory graph tables stored in Turso Database
* vector search done through Turso Database capabilities
* optional sync support later without redesigning the memory model
* current Rust integration via the `libsql` crate/tooling

### Recommended module layout

* `memory-core`
* `memory-store`
* `memory-retrieval`
* `memory-ingest`
* `memory-lessons`
* `memory-personality`
* `memory-imagination`
* `memory-sleep`
* `agent-api`

### Strong typing recommendations

Use enums and structs to separate memory classes clearly.

Example categories:

* node type
* edge relation type
* confidence state
* verification status
* lesson type
* trait type
* imagination status

This prevents accidental mixing of real, inferred, and imagined content.

---

## Minimum Viable Agent API

The first version should support at least these operations:

* `recall_context`
* `get_neighbors`
* `get_working_set`
* `propose_memory`
* `propose_lesson`
* `record_outcome`
* `pin_working_set`
* `generate_imagined_scenarios`

This is enough to support:

* normal agent interaction
* memory growth
* lesson formation
* personality learning
* imagination-assisted planning

---

## Build Order

### Phase 1

* node and edge graph
* memory admission rules
* hybrid retrieval
* working memory packets

### Phase 2

* lessons
* checkpoint summaries
* contradiction handling

### Phase 3

* forgetting and decay
* duplicate merging
* reconsolidation on recall

### Phase 4

* personality traits
* prediction and prediction error
* reinforcement from outcomes

### Phase 5

* imagination generation
* dream/sleep cycle
* self-model summaries

---

## Final Goal

The goal is to build **Nodamem**, a local-first memory engine where:

* agents remember only meaningful connected things
* experiences become lessons
* lessons shape personality
* memory can reorganize over time
* hypothetical imagination grows from real connected experience
* the user can later explore the graph visually like an Obsidian-style knowledge network

The agent should feel consistent, adaptive, and increasingly insightful without becoming opaque or uncontrollable.
