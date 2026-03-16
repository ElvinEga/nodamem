//! Core domain model for the Nodamem local-first memory graph.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Shared timestamp type used across persisted memory graph records.
pub type Timestamp = DateTime<Utc>;

/// Stable identifier for a graph node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub Uuid);

/// Stable identifier for a graph edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EdgeId(pub Uuid);

/// Stable identifier for a distilled lesson.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LessonId(pub Uuid);

/// Stable identifier for a personality trait state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TraitId(pub Uuid);

/// Stable identifier for a checkpoint summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CheckpointId(pub Uuid);

/// Stable identifier for a hypothetical imagined scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScenarioId(pub Uuid);

/// Stable identifier for a retrieved memory packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryPacketId(pub Uuid);

/// Graph node category used across verified memory, entities, goals, and governed imagined nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Episodic,
    Semantic,
    Lesson,
    Entity,
    Goal,
    Preference,
    Trait,
    Prediction,
    PredictionError,
    Checkpoint,
    Imagined,
    SelfModel,
}

/// Typed relationship connecting nodes in the memory graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    RelatedTo,
    DerivedFrom,
    Supports,
    Contradicts,
    SameTopic,
    SameProject,
    Teaches,
    Strengthens,
    Weakens,
    Predicts,
    CorrectedBy,
    InspiredBy,
    PartOf,
    SummarizedAs,
    AppliesTo,
}

/// Category for a distilled lesson, separate from raw memories and trait tendencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonType {
    Factual,
    Procedural,
    Strategic,
    Behavioral,
    Preference,
    Safety,
}

/// Category for a longer-lived personality tendency, kept separate from lessons and memories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraitType {
    CommunicationStyle,
    PlanningStyle,
    RiskTolerance,
    Creativity,
    Reliability,
    Curiosity,
    Collaboration,
}

/// Lifecycle state for verified or candidate memory records in the durable graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Candidate,
    Active,
    Reinforced,
    Contradicted,
    Archived,
    Pruned,
}

/// Validation state for hypothetical content so it cannot be confused with verified memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImaginationStatus {
    Proposed,
    Simulated,
    Reviewed,
    AcceptedAsHypothesis,
    Rejected,
}

/// Durable graph node for memory, entities, goals, or other governed knowledge records.
///
/// Even though `NodeType` includes `Imagined`, callers should expose imagined content through
/// [`ImaginedScenario`] and [`MemoryPacket::imagined_scenarios`] so verified recall remains
/// clearly separated from hypothetical reasoning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub node_type: NodeType,
    pub status: MemoryStatus,
    pub title: String,
    pub summary: String,
    pub content: Option<String>,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub importance: f32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub last_accessed_at: Option<Timestamp>,
    pub source_event_id: Option<String>,
}

/// Typed directed edge between two graph nodes with a strength value and provenance timestamp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub edge_type: EdgeType,
    pub from_node_id: NodeId,
    pub to_node_id: NodeId,
    pub weight: f32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Distilled reusable learning derived from one or more supporting memories.
///
/// Lessons remain separate from personality traits so the system can distinguish what was learned
/// from how the agent tends to behave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lesson {
    pub id: LessonId,
    pub lesson_type: LessonType,
    pub status: MemoryStatus,
    pub title: String,
    pub statement: String,
    pub confidence: f32,
    pub reinforcement_count: u32,
    pub supporting_node_ids: Vec<NodeId>,
    pub contradicting_node_ids: Vec<NodeId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Current state of a longer-term personality tendency inferred from repeated validated evidence.
///
/// Trait state is intentionally not stored as a lesson or generic memory record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitState {
    pub id: TraitId,
    pub trait_type: TraitType,
    pub status: MemoryStatus,
    pub label: String,
    pub description: String,
    pub strength: f32,
    pub confidence: f32,
    pub supporting_lesson_ids: Vec<LessonId>,
    pub supporting_node_ids: Vec<NodeId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Summary node capturing the state of a time window, topic cluster, or task period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: CheckpointId,
    pub status: MemoryStatus,
    pub title: String,
    pub summary: String,
    pub node_ids: Vec<NodeId>,
    pub lesson_ids: Vec<LessonId>,
    pub trait_ids: Vec<TraitId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Hypothetical scenario used for planning, forecasting, or counterfactual exploration.
///
/// This is intentionally distinct from verified memory nodes. It may reference real nodes, but its
/// contents must not be treated as established fact without later validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImaginedScenario {
    pub id: ScenarioId,
    pub status: ImaginationStatus,
    pub title: String,
    pub premise: String,
    pub narrative: String,
    pub source_node_ids: Vec<NodeId>,
    pub lesson_ids: Vec<LessonId>,
    pub predicted_outcomes: Vec<String>,
    pub confidence: f32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Curated context packet returned to agents for a task or recall request.
///
/// Verified graph context and imagined scenarios are separated into different fields so downstream
/// reasoning can preserve the truth boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryPacket {
    pub id: MemoryPacketId,
    pub request_id: Option<String>,
    pub created_at: Timestamp,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub lessons: Vec<Lesson>,
    pub traits: Vec<TraitState>,
    pub checkpoints: Vec<Checkpoint>,
    pub imagined_scenarios: Vec<ImaginedScenario>,
}

/// Marker type preserved for lightweight crate wiring and simple composition tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoreMarker;
