//! Repository layer for persisting and loading Nodamem graph records.

use libsql::{params, Connection};
use memory_core::{
    Checkpoint, Edge, ImaginedScenario, Lesson, LessonId, Node, NodeId, ScenarioId, TraitId,
    TraitState, WorkingMemoryEntry,
};
use uuid::Uuid;

use crate::error::StoreError;
use crate::mapper::{
    format_edge_type, format_lesson_type, format_memory_status, format_node_type,
    format_imagination_status, format_optional_timestamp, format_timestamp, format_trait_type,
    lesson_id_strings, map_checkpoint, map_edge, map_imagined_scenario, map_lesson, map_node,
    map_trait_state, map_working_memory_entry, node_id_strings, payload_to_json, to_json,
    trait_id_strings,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LessonSourceRole {
    Supporting,
    Contradicting,
}

impl LessonSourceRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Supporting => "supporting",
            Self::Contradicting => "contradicting",
        }
    }
}

#[derive(Debug)]
pub struct StoreRepository<'a> {
    connection: &'a Connection,
}

impl<'a> StoreRepository<'a> {
    #[must_use]
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub async fn insert_node(&self, node: &Node) -> Result<Node, StoreError> {
        self.connection
            .execute(
                "INSERT INTO nodes (
                    id, node_type, status, title, summary, content, tags_json, confidence,
                    importance, last_accessed_at, source_event_id, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    node.id.0.to_string(),
                    format_node_type(node.node_type),
                    format_memory_status(node.status),
                    node.title.clone(),
                    node.summary.clone(),
                    node.content.clone(),
                    to_json(&node.tags)?,
                    f64::from(node.confidence),
                    f64::from(node.importance),
                    format_optional_timestamp(node.last_accessed_at),
                    node.source_event_id.clone(),
                    format_timestamp(node.created_at),
                    format_timestamp(node.updated_at),
                ],
            )
            .await?;

        self.get_node_by_id(node.id)
            .await?
            .ok_or_else(|| missing_row("node", node.id.0))
    }

    pub async fn get_node_by_id(&self, node_id: NodeId) -> Result<Option<Node>, StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT id, node_type, status, title, summary, content, tags_json, confidence,
                        importance, created_at, updated_at, last_accessed_at, source_event_id
                 FROM nodes
                 WHERE id = ?1",
                params![node_id.0.to_string()],
            )
            .await?;

        rows.next().await?.map(|row| map_node(&row)).transpose()
    }

    pub async fn update_node(&self, node: &Node) -> Result<Option<Node>, StoreError> {
        self.connection
            .execute(
                "UPDATE nodes
                 SET node_type = ?2,
                     status = ?3,
                     title = ?4,
                     summary = ?5,
                     content = ?6,
                     tags_json = ?7,
                     confidence = ?8,
                     importance = ?9,
                     last_accessed_at = ?10,
                     source_event_id = ?11
                 WHERE id = ?1",
                params![
                    node.id.0.to_string(),
                    format_node_type(node.node_type),
                    format_memory_status(node.status),
                    node.title.clone(),
                    node.summary.clone(),
                    node.content.clone(),
                    to_json(&node.tags)?,
                    f64::from(node.confidence),
                    f64::from(node.importance),
                    format_optional_timestamp(node.last_accessed_at),
                    node.source_event_id.clone(),
                ],
            )
            .await?;

        self.get_node_by_id(node.id).await
    }

    pub async fn insert_edge(&self, edge: &Edge) -> Result<Edge, StoreError> {
        self.connection
            .execute(
                "INSERT INTO edges (
                    id, edge_type, from_node_id, to_node_id, weight, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    edge.id.0.to_string(),
                    format_edge_type(edge.edge_type),
                    edge.from_node_id.0.to_string(),
                    edge.to_node_id.0.to_string(),
                    f64::from(edge.weight),
                    format_timestamp(edge.created_at),
                    format_timestamp(edge.updated_at),
                ],
            )
            .await?;

        let mut rows = self
            .connection
            .query(
                "SELECT id, edge_type, from_node_id, to_node_id, weight, created_at, updated_at
                 FROM edges
                 WHERE id = ?1",
                params![edge.id.0.to_string()],
            )
            .await?;

        rows.next()
            .await?
            .map(|row| map_edge(&row))
            .transpose()?
            .ok_or_else(|| missing_row("edge", edge.id.0))
    }

    pub async fn get_neighbors(&self, node_id: NodeId) -> Result<Vec<Node>, StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT DISTINCT n.id, n.node_type, n.status, n.title, n.summary, n.content,
                        n.tags_json, n.confidence, n.importance, n.created_at, n.updated_at,
                        n.last_accessed_at, n.source_event_id
                 FROM edges e
                 JOIN nodes n
                   ON (
                        (e.from_node_id = ?1 AND e.to_node_id = n.id)
                     OR (e.to_node_id = ?1 AND e.from_node_id = n.id)
                   )
                 ORDER BY n.updated_at DESC",
                params![node_id.0.to_string()],
            )
            .await?;

        let mut nodes = Vec::new();
        while let Some(row) = rows.next().await? {
            nodes.push(map_node(&row)?);
        }

        Ok(nodes)
    }

    pub async fn upsert_lesson(&self, lesson: &Lesson) -> Result<Lesson, StoreError> {
        self.connection
            .execute(
                "INSERT INTO lessons (
                    id, lesson_type, status, title, statement, confidence, evidence_count,
                    reinforcement_count, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(id) DO UPDATE SET
                    lesson_type = excluded.lesson_type,
                    status = excluded.status,
                    title = excluded.title,
                    statement = excluded.statement,
                    confidence = excluded.confidence,
                    evidence_count = excluded.evidence_count,
                    reinforcement_count = excluded.reinforcement_count",
                params![
                    lesson.id.0.to_string(),
                    format_lesson_type(lesson.lesson_type),
                    format_memory_status(lesson.status),
                    lesson.title.clone(),
                    lesson.statement.clone(),
                    f64::from(lesson.confidence),
                    i64::from(lesson.evidence_count),
                    i64::from(lesson.reinforcement_count),
                    format_timestamp(lesson.created_at),
                    format_timestamp(lesson.updated_at),
                ],
            )
            .await?;

        self.load_lesson(lesson.id)
            .await?
            .ok_or_else(|| missing_row("lesson", lesson.id.0))
    }

    pub async fn attach_lesson_source(
        &self,
        lesson_id: LessonId,
        node_id: NodeId,
        source_role: LessonSourceRole,
    ) -> Result<(), StoreError> {
        self.connection
            .execute(
                "INSERT INTO lesson_sources (id, lesson_id, node_id, source_role)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    lesson_id.0.to_string(),
                    node_id.0.to_string(),
                    source_role.as_str(),
                ],
            )
            .await?;

        Ok(())
    }

    pub async fn load_trait_state(
        &self,
        trait_id: TraitId,
    ) -> Result<Option<TraitState>, StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT id, trait_type, status, label, description, strength, confidence,
                        supporting_lesson_ids_json, supporting_node_ids_json, created_at, updated_at
                 FROM trait_state
                 WHERE id = ?1",
                params![trait_id.0.to_string()],
            )
            .await?;

        rows.next()
            .await?
            .map(|row| map_trait_state(&row))
            .transpose()
    }

    pub async fn save_trait_state(
        &self,
        trait_state: &TraitState,
    ) -> Result<TraitState, StoreError> {
        self.connection
            .execute(
                "INSERT INTO trait_state (
                    id, trait_type, status, label, description, strength, confidence,
                    supporting_lesson_ids_json, supporting_node_ids_json, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                    trait_type = excluded.trait_type,
                    status = excluded.status,
                    label = excluded.label,
                    description = excluded.description,
                    strength = excluded.strength,
                    confidence = excluded.confidence,
                    supporting_lesson_ids_json = excluded.supporting_lesson_ids_json,
                    supporting_node_ids_json = excluded.supporting_node_ids_json",
                params![
                    trait_state.id.0.to_string(),
                    format_trait_type(trait_state.trait_type),
                    format_memory_status(trait_state.status),
                    trait_state.label.clone(),
                    trait_state.description.clone(),
                    f64::from(trait_state.strength),
                    f64::from(trait_state.confidence),
                    to_json(&lesson_id_strings(&trait_state.supporting_lesson_ids))?,
                    to_json(&node_id_strings(&trait_state.supporting_node_ids))?,
                    format_timestamp(trait_state.created_at),
                    format_timestamp(trait_state.updated_at),
                ],
            )
            .await?;

        self.load_trait_state(trait_state.id)
            .await?
            .ok_or_else(|| missing_row("trait_state", trait_state.id.0))
    }

    pub async fn create_checkpoint(
        &self,
        checkpoint: &Checkpoint,
    ) -> Result<Checkpoint, StoreError> {
        self.connection
            .execute(
                "INSERT INTO checkpoints (
                    id, status, title, summary, node_ids_json, lesson_ids_json, trait_ids_json,
                    created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    checkpoint.id.0.to_string(),
                    format_memory_status(checkpoint.status),
                    checkpoint.title.clone(),
                    checkpoint.summary.clone(),
                    to_json(&node_id_strings(&checkpoint.node_ids))?,
                    to_json(&lesson_id_strings(&checkpoint.lesson_ids))?,
                    to_json(&trait_id_strings(&checkpoint.trait_ids))?,
                    format_timestamp(checkpoint.created_at),
                    format_timestamp(checkpoint.updated_at),
                ],
            )
            .await?;

        let mut rows = self
            .connection
            .query(
                "SELECT id, status, title, summary, node_ids_json, lesson_ids_json, trait_ids_json,
                        created_at, updated_at
                 FROM checkpoints
                 WHERE id = ?1",
                params![checkpoint.id.0.to_string()],
            )
            .await?;

        rows.next()
            .await?
            .map(|row| map_checkpoint(&row))
            .transpose()?
            .ok_or_else(|| missing_row("checkpoint", checkpoint.id.0))
    }

    pub async fn load_recent_checkpoints(&self, limit: u32) -> Result<Vec<Checkpoint>, StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT id, status, title, summary, node_ids_json, lesson_ids_json, trait_ids_json,
                        created_at, updated_at
                 FROM checkpoints
                 ORDER BY updated_at DESC
                 LIMIT ?1",
                params![i64::from(limit)],
            )
            .await?;

        let mut checkpoints = Vec::new();
        while let Some(row) = rows.next().await? {
            checkpoints.push(map_checkpoint(&row)?);
        }

        Ok(checkpoints)
    }

    pub async fn upsert_imagined_scenario(
        &self,
        scenario: &ImaginedScenario,
    ) -> Result<ImaginedScenario, StoreError> {
        self.connection
            .execute(
                "INSERT INTO imagined_nodes (
                    id, status, title, premise, narrative, basis_source_node_ids_json,
                    basis_lesson_ids_json, active_goal_node_ids_json, trait_snapshot_json,
                    predicted_outcomes_json, plausibility_score, novelty_score, usefulness_score,
                    created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                 ON CONFLICT(id) DO UPDATE SET
                    status = excluded.status,
                    title = excluded.title,
                    premise = excluded.premise,
                    narrative = excluded.narrative,
                    basis_source_node_ids_json = excluded.basis_source_node_ids_json,
                    basis_lesson_ids_json = excluded.basis_lesson_ids_json,
                    active_goal_node_ids_json = excluded.active_goal_node_ids_json,
                    trait_snapshot_json = excluded.trait_snapshot_json,
                    predicted_outcomes_json = excluded.predicted_outcomes_json,
                    plausibility_score = excluded.plausibility_score,
                    novelty_score = excluded.novelty_score,
                    usefulness_score = excluded.usefulness_score",
                params![
                    scenario.id.0.to_string(),
                    format_imagination_status(scenario.status),
                    scenario.title.clone(),
                    scenario.premise.clone(),
                    scenario.narrative.clone(),
                    to_json(&node_id_strings(&scenario.basis_source_node_ids))?,
                    to_json(&lesson_id_strings(&scenario.basis_lesson_ids))?,
                    to_json(&node_id_strings(&scenario.active_goal_node_ids))?,
                    to_json(&scenario.trait_snapshot)?,
                    to_json(&scenario.predicted_outcomes)?,
                    f64::from(scenario.plausibility_score),
                    f64::from(scenario.novelty_score),
                    f64::from(scenario.usefulness_score),
                    format_timestamp(scenario.created_at),
                    format_timestamp(scenario.updated_at),
                ],
            )
            .await?;

        self.load_imagined_scenario(scenario.id)
            .await?
            .ok_or_else(|| missing_row("imagined_nodes", scenario.id.0))
    }

    pub async fn load_imagined_scenario(
        &self,
        scenario_id: ScenarioId,
    ) -> Result<Option<ImaginedScenario>, StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT id, status, title, premise, narrative, basis_source_node_ids_json,
                        basis_lesson_ids_json, active_goal_node_ids_json, trait_snapshot_json,
                        predicted_outcomes_json, plausibility_score, novelty_score, usefulness_score,
                        created_at, updated_at
                 FROM imagined_nodes
                 WHERE id = ?1",
                params![scenario_id.0.to_string()],
            )
            .await?;

        rows.next()
            .await?
            .map(|row| map_imagined_scenario(&row))
            .transpose()
    }

    pub async fn list_imagined_scenarios(
        &self,
        limit: u32,
    ) -> Result<Vec<ImaginedScenario>, StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT id, status, title, premise, narrative, basis_source_node_ids_json,
                        basis_lesson_ids_json, active_goal_node_ids_json, trait_snapshot_json,
                        predicted_outcomes_json, plausibility_score, novelty_score, usefulness_score,
                        created_at, updated_at
                 FROM imagined_nodes
                 ORDER BY updated_at DESC
                 LIMIT ?1",
                params![i64::from(limit)],
            )
            .await?;

        let mut scenarios = Vec::new();
        while let Some(row) = rows.next().await? {
            scenarios.push(map_imagined_scenario(&row)?);
        }

        Ok(scenarios)
    }

    pub async fn upsert_working_memory_entry(
        &self,
        entry: &WorkingMemoryEntry,
    ) -> Result<WorkingMemoryEntry, StoreError> {
        self.connection
            .execute(
                "INSERT INTO working_memory (
                    id, scope_key, session_id, task_ref, payload_json, expires_at, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(scope_key) DO UPDATE SET
                    id = excluded.id,
                    session_id = excluded.session_id,
                    task_ref = excluded.task_ref,
                    payload_json = excluded.payload_json,
                    expires_at = excluded.expires_at",
                params![
                    entry.id.0.to_string(),
                    entry.scope_key.clone(),
                    entry.session_id.clone(),
                    entry.task_ref.clone(),
                    payload_to_json(&entry.payload)?,
                    format_optional_timestamp(entry.expires_at),
                    format_timestamp(entry.created_at),
                    format_timestamp(entry.updated_at),
                ],
            )
            .await?;

        self.get_working_memory_entry(&entry.scope_key)
            .await?
            .ok_or_else(|| StoreError::InvalidValue {
                field: "working_memory.scope_key",
                value: entry.scope_key.clone(),
            })
    }

    pub async fn get_working_memory_entry(
        &self,
        scope_key: &str,
    ) -> Result<Option<WorkingMemoryEntry>, StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT id, scope_key, session_id, task_ref, payload_json, expires_at, created_at, updated_at
                 FROM working_memory
                 WHERE scope_key = ?1",
                params![scope_key],
            )
            .await?;

        rows.next()
            .await?
            .map(|row| map_working_memory_entry(&row))
            .transpose()
    }

    pub async fn list_working_memory_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<WorkingMemoryEntry>, StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT id, scope_key, session_id, task_ref, payload_json, expires_at, created_at, updated_at
                 FROM working_memory
                 WHERE session_id = ?1
                 ORDER BY updated_at DESC",
                params![session_id],
            )
            .await?;

        let mut entries = Vec::new();
        while let Some(row) = rows.next().await? {
            entries.push(map_working_memory_entry(&row)?);
        }

        Ok(entries)
    }

    pub async fn delete_working_memory_entry(&self, scope_key: &str) -> Result<bool, StoreError> {
        let deleted = self
            .connection
            .execute(
                "DELETE FROM working_memory WHERE scope_key = ?1",
                params![scope_key],
            )
            .await?;

        Ok(deleted > 0)
    }

    async fn load_lesson(&self, lesson_id: LessonId) -> Result<Option<Lesson>, StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT id, lesson_type, status, title, statement, confidence, evidence_count,
                        reinforcement_count, created_at, updated_at
                 FROM lessons
                 WHERE id = ?1",
                params![lesson_id.0.to_string()],
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let (supporting_node_ids, contradicting_node_ids) =
            self.load_lesson_sources(lesson_id).await?;

        Ok(Some(map_lesson(
            &row,
            supporting_node_ids,
            contradicting_node_ids,
        )?))
    }

    async fn load_lesson_sources(
        &self,
        lesson_id: LessonId,
    ) -> Result<(Vec<NodeId>, Vec<NodeId>), StoreError> {
        let mut rows = self
            .connection
            .query(
                "SELECT node_id, source_role
                 FROM lesson_sources
                 WHERE lesson_id = ?1",
                params![lesson_id.0.to_string()],
            )
            .await?;

        let mut supporting = Vec::new();
        let mut contradicting = Vec::new();

        while let Some(row) = rows.next().await? {
            let node_id = NodeId(crate::mapper::parse_uuid(
                row.get::<String>(0)?,
                "lesson_sources.node_id",
            )?);
            let source_role: String = row.get(1)?;

            match source_role.as_str() {
                "supporting" => supporting.push(node_id),
                "contradicting" => contradicting.push(node_id),
                _ => {
                    return Err(StoreError::InvalidValue {
                        field: "lesson_sources.source_role",
                        value: source_role,
                    });
                }
            }
        }

        Ok((supporting, contradicting))
    }
}

fn missing_row(kind: &'static str, id: Uuid) -> StoreError {
    StoreError::InvalidValue {
        field: kind,
        value: id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use memory_core::{
        Checkpoint, CheckpointId, Edge, EdgeId, EdgeType, ImaginedScenario, ImaginationStatus,
        Lesson, LessonId, LessonType, MemoryStatus, Node, NodeId, NodeType, ScenarioId, TraitId,
        TraitState, TraitType, WorkingMemoryEntry, WorkingMemoryId,
    };
    use serde_json::json;
    use uuid::Uuid;

    use super::{LessonSourceRole, StoreRepository};
    use crate::config::StoreConfig;
    use crate::db::StoreRuntime;

    #[tokio::test]
    async fn persists_nodes_edges_and_neighbors() {
        let runtime = open_test_runtime().await;
        let repository = StoreRepository::new(&runtime.connection);

        let node_a = sample_node("alpha", NodeType::Episodic);
        let node_b = sample_node("beta", NodeType::Semantic);

        let saved_a = repository
            .insert_node(&node_a)
            .await
            .expect("node insert should work");
        let saved_b = repository
            .insert_node(&node_b)
            .await
            .expect("node insert should work");

        repository
            .insert_edge(&Edge {
                id: EdgeId(Uuid::new_v4()),
                edge_type: EdgeType::RelatedTo,
                from_node_id: saved_a.id,
                to_node_id: saved_b.id,
                weight: 0.8,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .expect("edge insert should work");

        let loaded = repository
            .get_node_by_id(saved_a.id)
            .await
            .expect("node load should work")
            .expect("node should exist");

        assert_eq!(loaded.title, "alpha");

        let neighbors = repository
            .get_neighbors(saved_a.id)
            .await
            .expect("neighbor query should work");

        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].id, saved_b.id);
    }

    #[tokio::test]
    async fn persists_lessons_traits_checkpoints_and_working_memory() {
        let runtime = open_test_runtime().await;
        let repository = StoreRepository::new(&runtime.connection);

        let source_node = repository
            .insert_node(&sample_node("source", NodeType::Episodic))
            .await
            .expect("source node insert should work");

        let lesson = Lesson {
            id: LessonId(Uuid::new_v4()),
            lesson_type: LessonType::Strategy,
            status: MemoryStatus::Active,
            title: "use structure".to_owned(),
            statement: "Structured memory retrieval improves consistency.".to_owned(),
            confidence: 0.8,
            evidence_count: 1,
            reinforcement_count: 2,
            supporting_node_ids: vec![],
            contradicting_node_ids: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let saved_lesson = repository
            .upsert_lesson(&lesson)
            .await
            .expect("lesson upsert should work");
        repository
            .attach_lesson_source(
                saved_lesson.id,
                source_node.id,
                LessonSourceRole::Supporting,
            )
            .await
            .expect("lesson source attach should work");

        let updated_lesson = repository
            .upsert_lesson(&saved_lesson)
            .await
            .expect("lesson reload should work");
        assert_eq!(updated_lesson.supporting_node_ids, vec![source_node.id]);

        let trait_state = TraitState {
            id: TraitId(Uuid::new_v4()),
            trait_type: TraitType::Practicality,
            status: MemoryStatus::Active,
            label: "Practical".to_owned(),
            description: "Prefers useful and workable outcomes.".to_owned(),
            strength: 0.9,
            confidence: 0.7,
            supporting_lesson_ids: vec![saved_lesson.id],
            supporting_node_ids: vec![source_node.id],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let saved_trait = repository
            .save_trait_state(&trait_state)
            .await
            .expect("trait save should work");
        let loaded_trait = repository
            .load_trait_state(saved_trait.id)
            .await
            .expect("trait load should work")
            .expect("trait should exist");
        assert_eq!(loaded_trait.label, "Practical");

        repository
            .create_checkpoint(&Checkpoint {
                id: CheckpointId(Uuid::new_v4()),
                status: MemoryStatus::Active,
                title: "recent work".to_owned(),
                summary: "Recent memory checkpoint.".to_owned(),
                node_ids: vec![source_node.id],
                lesson_ids: vec![saved_lesson.id],
                trait_ids: vec![saved_trait.id],
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .expect("checkpoint create should work");

        let checkpoints = repository
            .load_recent_checkpoints(10)
            .await
            .expect("recent checkpoint load should work");
        assert_eq!(checkpoints.len(), 1);

        let working_memory = WorkingMemoryEntry {
            id: WorkingMemoryId(Uuid::new_v4()),
            scope_key: "session:demo".to_owned(),
            session_id: Some("session-1".to_owned()),
            task_ref: Some("task-1".to_owned()),
            payload: json!({ "pinned_nodes": [source_node.id.0.to_string()] }),
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        repository
            .upsert_working_memory_entry(&working_memory)
            .await
            .expect("working memory save should work");

        let loaded_working_memory = repository
            .get_working_memory_entry("session:demo")
            .await
            .expect("working memory load should work")
            .expect("working memory should exist");
        assert_eq!(loaded_working_memory.scope_key, "session:demo");

        let session_entries = repository
            .list_working_memory_for_session("session-1")
            .await
            .expect("working memory list should work");
        assert_eq!(session_entries.len(), 1);

        let deleted = repository
            .delete_working_memory_entry("session:demo")
            .await
            .expect("working memory delete should work");
        assert!(deleted);
    }

    #[tokio::test]
    async fn persists_imagined_scenarios_separately_from_verified_nodes() {
        let runtime = open_test_runtime().await;
        let repository = StoreRepository::new(&runtime.connection);

        let basis_node = repository
            .insert_node(&sample_node("basis", NodeType::Semantic))
            .await
            .expect("basis node insert should work");
        let goal_node = repository
            .insert_node(&sample_node("goal", NodeType::Goal))
            .await
            .expect("goal node insert should work");

        let scenario = ImaginedScenario {
            id: ScenarioId(Uuid::new_v4()),
            status: ImaginationStatus::Proposed,
            title: "Hypothetical plan".to_owned(),
            premise: "If the agent reuses the basis memory cluster, planning may accelerate."
                .to_owned(),
            narrative: "This scenario is hypothetical and should not be treated as verified memory."
                .to_owned(),
            basis_source_node_ids: vec![basis_node.id],
            basis_lesson_ids: Vec::new(),
            active_goal_node_ids: vec![goal_node.id],
            trait_snapshot: vec![TraitState {
                id: TraitId(Uuid::new_v4()),
                trait_type: TraitType::Practicality,
                status: MemoryStatus::Active,
                label: "Practical".to_owned(),
                description: "Optimizes for useful outcomes.".to_owned(),
                strength: 0.8,
                confidence: 0.7,
                supporting_lesson_ids: Vec::new(),
                supporting_node_ids: vec![basis_node.id],
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            predicted_outcomes: vec!["Planning finishes with fewer revisions.".to_owned()],
            plausibility_score: 0.74,
            novelty_score: 0.51,
            usefulness_score: 0.82,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let saved = repository
            .upsert_imagined_scenario(&scenario)
            .await
            .expect("imagined scenario upsert should work");
        let listed = repository
            .list_imagined_scenarios(10)
            .await
            .expect("imagined scenario list should work");

        assert_eq!(saved.status, ImaginationStatus::Proposed);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].basis_source_node_ids, vec![basis_node.id]);
        assert!(
            repository
                .get_node_by_id(NodeId(saved.id.0))
                .await
                .expect("verified node lookup should work")
                .is_none(),
            "imagined scenarios must not appear in verified node storage"
        );
    }

    async fn open_test_runtime() -> StoreRuntime {
        let tempdir = std::env::temp_dir().join(format!("nodamem-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tempdir).expect("temporary directory should be created");
        StoreRuntime::open(StoreConfig {
            local_database_path: tempdir.join("nodamem.db"),
            ..StoreConfig::default()
        })
        .await
        .expect("test database should open")
    }

    fn sample_node(title: &str, node_type: NodeType) -> Node {
        Node {
            id: NodeId(Uuid::new_v4()),
            node_type,
            status: MemoryStatus::Active,
            title: title.to_owned(),
            summary: format!("{title} summary"),
            content: Some(format!("{title} content")),
            tags: vec!["test".to_owned()],
            confidence: 0.9,
            importance: 0.7,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            source_event_id: Some("event-1".to_owned()),
        }
    }
}
