//! Storage-to-domain mapping helpers for SQLite-backed records.

use chrono::{DateTime, NaiveDateTime, Utc};
use libsql::Row;
use memory_core::{
    Checkpoint, CheckpointId, Edge, EdgeId, EdgeType, ImaginationStatus, Lesson, LessonId,
    LessonType, MemoryStatus, Node, NodeId, NodeType, Timestamp, TraitId, TraitState, TraitType,
    WorkingMemoryEntry, WorkingMemoryId,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::error::StoreError;

pub fn map_node(row: &Row) -> Result<Node, StoreError> {
    Ok(Node {
        id: NodeId(parse_uuid(row.get::<String>(0)?, "nodes.id")?),
        node_type: parse_node_type(&row.get::<String>(1)?)?,
        status: parse_memory_status(&row.get::<String>(2)?)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        content: row.get(5)?,
        tags: parse_json(&row.get::<String>(6)?, "nodes.tags_json")?,
        confidence: row.get::<f64>(7)? as f32,
        importance: row.get::<f64>(8)? as f32,
        created_at: parse_timestamp(&row.get::<String>(9)?)?,
        updated_at: parse_timestamp(&row.get::<String>(10)?)?,
        last_accessed_at: parse_optional_timestamp(row.get::<Option<String>>(11)?)?,
        source_event_id: row.get(12)?,
    })
}

pub fn map_edge(row: &Row) -> Result<Edge, StoreError> {
    Ok(Edge {
        id: EdgeId(parse_uuid(row.get::<String>(0)?, "edges.id")?),
        edge_type: parse_edge_type(&row.get::<String>(1)?)?,
        from_node_id: NodeId(parse_uuid(row.get::<String>(2)?, "edges.from_node_id")?),
        to_node_id: NodeId(parse_uuid(row.get::<String>(3)?, "edges.to_node_id")?),
        weight: row.get::<f64>(4)? as f32,
        created_at: parse_timestamp(&row.get::<String>(5)?)?,
        updated_at: parse_timestamp(&row.get::<String>(6)?)?,
    })
}

pub fn map_lesson(
    row: &Row,
    supporting_node_ids: Vec<NodeId>,
    contradicting_node_ids: Vec<NodeId>,
) -> Result<Lesson, StoreError> {
    Ok(Lesson {
        id: LessonId(parse_uuid(row.get::<String>(0)?, "lessons.id")?),
        lesson_type: parse_lesson_type(&row.get::<String>(1)?)?,
        status: parse_memory_status(&row.get::<String>(2)?)?,
        title: row.get(3)?,
        statement: row.get(4)?,
        confidence: row.get::<f64>(5)? as f32,
        evidence_count: row.get::<i64>(6)? as u32,
        reinforcement_count: row.get::<i64>(7)? as u32,
        supporting_node_ids,
        contradicting_node_ids,
        created_at: parse_timestamp(&row.get::<String>(8)?)?,
        updated_at: parse_timestamp(&row.get::<String>(9)?)?,
    })
}

pub fn map_trait_state(row: &Row) -> Result<TraitState, StoreError> {
    Ok(TraitState {
        id: TraitId(parse_uuid(row.get::<String>(0)?, "trait_state.id")?),
        trait_type: parse_trait_type(&row.get::<String>(1)?)?,
        status: parse_memory_status(&row.get::<String>(2)?)?,
        label: row.get(3)?,
        description: row.get(4)?,
        strength: row.get::<f64>(5)? as f32,
        confidence: row.get::<f64>(6)? as f32,
        supporting_lesson_ids: parse_id_list(
            &row.get::<String>(7)?,
            "trait_state.supporting_lesson_ids_json",
            LessonId,
        )?,
        supporting_node_ids: parse_id_list(
            &row.get::<String>(8)?,
            "trait_state.supporting_node_ids_json",
            NodeId,
        )?,
        created_at: parse_timestamp(&row.get::<String>(9)?)?,
        updated_at: parse_timestamp(&row.get::<String>(10)?)?,
    })
}

pub fn map_checkpoint(row: &Row) -> Result<Checkpoint, StoreError> {
    Ok(Checkpoint {
        id: CheckpointId(parse_uuid(row.get::<String>(0)?, "checkpoints.id")?),
        status: parse_memory_status(&row.get::<String>(1)?)?,
        title: row.get(2)?,
        summary: row.get(3)?,
        node_ids: parse_id_list(&row.get::<String>(4)?, "checkpoints.node_ids_json", NodeId)?,
        lesson_ids: parse_id_list(
            &row.get::<String>(5)?,
            "checkpoints.lesson_ids_json",
            LessonId,
        )?,
        trait_ids: parse_id_list(
            &row.get::<String>(6)?,
            "checkpoints.trait_ids_json",
            TraitId,
        )?,
        created_at: parse_timestamp(&row.get::<String>(7)?)?,
        updated_at: parse_timestamp(&row.get::<String>(8)?)?,
    })
}

pub fn map_working_memory_entry(row: &Row) -> Result<WorkingMemoryEntry, StoreError> {
    Ok(WorkingMemoryEntry {
        id: WorkingMemoryId(parse_uuid(row.get::<String>(0)?, "working_memory.id")?),
        scope_key: row.get(1)?,
        session_id: row.get(2)?,
        task_ref: row.get(3)?,
        payload: parse_json(&row.get::<String>(4)?, "working_memory.payload_json")?,
        expires_at: parse_optional_timestamp(row.get::<Option<String>>(5)?)?,
        created_at: parse_timestamp(&row.get::<String>(6)?)?,
        updated_at: parse_timestamp(&row.get::<String>(7)?)?,
    })
}

pub fn format_node_type(value: NodeType) -> &'static str {
    match value {
        NodeType::Episodic => "episodic",
        NodeType::Semantic => "semantic",
        NodeType::Lesson => "lesson",
        NodeType::Entity => "entity",
        NodeType::Goal => "goal",
        NodeType::Preference => "preference",
        NodeType::Trait => "trait",
        NodeType::Prediction => "prediction",
        NodeType::PredictionError => "prediction_error",
        NodeType::Checkpoint => "checkpoint",
        NodeType::Imagined => "imagined",
        NodeType::SelfModel => "self_model",
    }
}

pub fn format_edge_type(value: EdgeType) -> &'static str {
    match value {
        EdgeType::RelatedTo => "related_to",
        EdgeType::DerivedFrom => "derived_from",
        EdgeType::Supports => "supports",
        EdgeType::Contradicts => "contradicts",
        EdgeType::SameTopic => "same_topic",
        EdgeType::SameProject => "same_project",
        EdgeType::Teaches => "teaches",
        EdgeType::Strengthens => "strengthens",
        EdgeType::Weakens => "weakens",
        EdgeType::Predicts => "predicts",
        EdgeType::CorrectedBy => "corrected_by",
        EdgeType::InspiredBy => "inspired_by",
        EdgeType::PartOf => "part_of",
        EdgeType::SummarizedAs => "summarized_as",
        EdgeType::AppliesTo => "applies_to",
    }
}

pub fn format_lesson_type(value: LessonType) -> &'static str {
    match value {
        LessonType::User => "user_lesson",
        LessonType::System => "system_lesson",
        LessonType::Task => "task_lesson",
        LessonType::Strategy => "strategy_lesson",
        LessonType::Domain => "domain_lesson",
        LessonType::Personality => "personality_lesson",
    }
}

pub fn format_trait_type(value: TraitType) -> &'static str {
    match value {
        TraitType::Curiosity => "curiosity",
        TraitType::Caution => "caution",
        TraitType::Verbosity => "verbosity",
        TraitType::NoveltySeeking => "novelty_seeking",
        TraitType::EvidenceReliance => "evidence_reliance",
        TraitType::Reliability => "reliability",
        TraitType::Practicality => "practicality",
        TraitType::Proactivity => "proactivity",
    }
}

pub fn format_memory_status(value: MemoryStatus) -> &'static str {
    match value {
        MemoryStatus::Candidate => "candidate",
        MemoryStatus::Active => "active",
        MemoryStatus::Reinforced => "reinforced",
        MemoryStatus::Contradicted => "contradicted",
        MemoryStatus::Archived => "archived",
        MemoryStatus::Pruned => "pruned",
    }
}

pub fn format_imagination_status(value: ImaginationStatus) -> &'static str {
    match value {
        ImaginationStatus::Proposed => "proposed",
        ImaginationStatus::Simulated => "simulated",
        ImaginationStatus::Reviewed => "reviewed",
        ImaginationStatus::AcceptedAsHypothesis => "accepted_as_hypothesis",
        ImaginationStatus::Rejected => "rejected",
    }
}

pub fn format_timestamp(value: Timestamp) -> String {
    value.to_rfc3339()
}

pub fn format_optional_timestamp(value: Option<Timestamp>) -> Option<String> {
    value.map(format_timestamp)
}

pub fn to_json<T>(value: &T) -> Result<String, StoreError>
where
    T: Serialize,
{
    Ok(serde_json::to_string(value)?)
}

pub fn parse_node_type(value: &str) -> Result<NodeType, StoreError> {
    match value {
        "episodic" => Ok(NodeType::Episodic),
        "semantic" => Ok(NodeType::Semantic),
        "lesson" => Ok(NodeType::Lesson),
        "entity" => Ok(NodeType::Entity),
        "goal" => Ok(NodeType::Goal),
        "preference" => Ok(NodeType::Preference),
        "trait" => Ok(NodeType::Trait),
        "prediction" => Ok(NodeType::Prediction),
        "prediction_error" => Ok(NodeType::PredictionError),
        "checkpoint" => Ok(NodeType::Checkpoint),
        "imagined" => Ok(NodeType::Imagined),
        "self_model" => Ok(NodeType::SelfModel),
        _ => Err(StoreError::InvalidValue {
            field: "node_type",
            value: value.to_owned(),
        }),
    }
}

pub fn parse_edge_type(value: &str) -> Result<EdgeType, StoreError> {
    match value {
        "related_to" => Ok(EdgeType::RelatedTo),
        "derived_from" => Ok(EdgeType::DerivedFrom),
        "supports" => Ok(EdgeType::Supports),
        "contradicts" => Ok(EdgeType::Contradicts),
        "same_topic" => Ok(EdgeType::SameTopic),
        "same_project" => Ok(EdgeType::SameProject),
        "teaches" => Ok(EdgeType::Teaches),
        "strengthens" => Ok(EdgeType::Strengthens),
        "weakens" => Ok(EdgeType::Weakens),
        "predicts" => Ok(EdgeType::Predicts),
        "corrected_by" => Ok(EdgeType::CorrectedBy),
        "inspired_by" => Ok(EdgeType::InspiredBy),
        "part_of" => Ok(EdgeType::PartOf),
        "summarized_as" => Ok(EdgeType::SummarizedAs),
        "applies_to" => Ok(EdgeType::AppliesTo),
        _ => Err(StoreError::InvalidValue {
            field: "edge_type",
            value: value.to_owned(),
        }),
    }
}

pub fn parse_lesson_type(value: &str) -> Result<LessonType, StoreError> {
    match value {
        "user_lesson" => Ok(LessonType::User),
        "system_lesson" => Ok(LessonType::System),
        "task_lesson" => Ok(LessonType::Task),
        "strategy_lesson" => Ok(LessonType::Strategy),
        "domain_lesson" => Ok(LessonType::Domain),
        "personality_lesson" => Ok(LessonType::Personality),
        _ => Err(StoreError::InvalidValue {
            field: "lesson_type",
            value: value.to_owned(),
        }),
    }
}

pub fn parse_trait_type(value: &str) -> Result<TraitType, StoreError> {
    match value {
        "curiosity" => Ok(TraitType::Curiosity),
        "caution" => Ok(TraitType::Caution),
        "verbosity" => Ok(TraitType::Verbosity),
        "novelty_seeking" => Ok(TraitType::NoveltySeeking),
        "evidence_reliance" => Ok(TraitType::EvidenceReliance),
        "reliability" => Ok(TraitType::Reliability),
        "practicality" => Ok(TraitType::Practicality),
        "proactivity" => Ok(TraitType::Proactivity),
        _ => Err(StoreError::InvalidValue {
            field: "trait_type",
            value: value.to_owned(),
        }),
    }
}

pub fn parse_memory_status(value: &str) -> Result<MemoryStatus, StoreError> {
    match value {
        "candidate" => Ok(MemoryStatus::Candidate),
        "active" => Ok(MemoryStatus::Active),
        "reinforced" => Ok(MemoryStatus::Reinforced),
        "contradicted" => Ok(MemoryStatus::Contradicted),
        "archived" => Ok(MemoryStatus::Archived),
        "pruned" => Ok(MemoryStatus::Pruned),
        _ => Err(StoreError::InvalidValue {
            field: "memory_status",
            value: value.to_owned(),
        }),
    }
}

pub fn parse_timestamp(value: &str) -> Result<Timestamp, StoreError> {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Ok(timestamp.with_timezone(&Utc));
    }

    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")?;
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

pub fn parse_optional_timestamp(value: Option<String>) -> Result<Option<Timestamp>, StoreError> {
    value.as_deref().map(parse_timestamp).transpose()
}

pub fn parse_json<T>(value: &str, field: &'static str) -> Result<T, StoreError>
where
    T: DeserializeOwned,
{
    serde_json::from_str(value).map_err(|_| StoreError::InvalidValue {
        field,
        value: value.to_owned(),
    })
}

pub fn parse_uuid(value: String, field: &'static str) -> Result<Uuid, StoreError> {
    Uuid::parse_str(&value).map_err(|_| StoreError::InvalidValue { field, value })
}

fn parse_id_list<T, F>(value: &str, field: &'static str, map: F) -> Result<Vec<T>, StoreError>
where
    F: Fn(Uuid) -> T,
{
    parse_json::<Vec<String>>(value, field)?
        .into_iter()
        .map(|item| parse_uuid(item, field).map(&map))
        .collect()
}

pub fn node_id_strings(ids: &[NodeId]) -> Vec<String> {
    ids.iter().map(|id| id.0.to_string()).collect()
}

pub fn lesson_id_strings(ids: &[LessonId]) -> Vec<String> {
    ids.iter().map(|id| id.0.to_string()).collect()
}

pub fn trait_id_strings(ids: &[TraitId]) -> Vec<String> {
    ids.iter().map(|id| id.0.to_string()).collect()
}

pub fn payload_to_json(value: &JsonValue) -> Result<String, StoreError> {
    Ok(serde_json::to_string(value)?)
}
