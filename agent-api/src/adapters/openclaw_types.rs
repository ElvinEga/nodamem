use memory_core::{
    AdmissionDecision, Checkpoint, LessonType, NodeId, TraitType,
};
use memory_ingest::IngestEvent;
use memory_ingest::AdmissionContext;
use serde::{Deserialize, Serialize};

use crate::{
    GenerateImaginedScenariosRequest, GetNeighborsRequest, OutcomeRecordDto, ProposeLessonRequest,
    ProposeMemoryRequest, RecallContextRequest,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawRecallContextRequest {
    pub text: String,
    pub session_id: Option<String>,
    pub topic: Option<String>,
    pub nodes: Vec<memory_core::Node>,
    pub edges: Vec<memory_core::Edge>,
    pub lessons: Vec<memory_core::Lesson>,
    pub checkpoints: Vec<Checkpoint>,
    pub traits: Vec<memory_core::TraitState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawGetNeighborsRequest {
    pub node_id: NodeId,
    pub nodes: Vec<memory_core::Node>,
    pub edges: Vec<memory_core::Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawProposeMemoryRequest {
    pub event: IngestEvent,
    pub context: AdmissionContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawProposeLessonRequest {
    pub accepted_memories: Vec<memory_core::Node>,
    pub existing_lessons: Vec<memory_core::Lesson>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawRecordOutcomeRequest {
    pub existing_traits: Vec<memory_core::TraitState>,
    pub outcome: OutcomeRecordDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawGenerateImaginedScenariosRequest {
    pub planning_task: String,
    pub desired_scenarios: usize,
    pub context_packet: memory_core::MemoryPacket,
    pub active_goal_node_ids: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawNodeSummary {
    pub node_id: NodeId,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub importance: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawLessonSummary {
    pub lesson_id: memory_core::LessonId,
    pub lesson_type: LessonType,
    pub title: String,
    pub statement: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawTraitSummary {
    pub trait_type: TraitType,
    pub label: String,
    pub strength: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawImaginedScenarioSummary {
    pub scenario_id: memory_core::ScenarioId,
    pub title: String,
    pub premise: String,
    pub predicted_outcomes: Vec<String>,
    pub plausibility_score: f32,
    pub novelty_score: f32,
    pub usefulness_score: f32,
    pub hypothetical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawRecallContextResponse {
    pub summary: String,
    pub nodes: Vec<OpenClawNodeSummary>,
    pub lessons: Vec<OpenClawLessonSummary>,
    pub checkpoint_summary: Option<String>,
    pub trait_snapshot: Vec<OpenClawTraitSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawGetNeighborsResponse {
    pub node_id: NodeId,
    pub neighbors: Vec<OpenClawNodeSummary>,
    pub connection_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawProposeMemoryResponse {
    pub candidate_node_count: usize,
    pub candidate_lesson_count: usize,
    pub decisions: Vec<OpenClawAdmissionDecisionSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawAdmissionDecisionSummary {
    pub candidate_node_id: NodeId,
    pub action: memory_core::AdmissionAction,
    pub matched_node_id: Option<NodeId>,
    pub reason: String,
    pub total_score: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpenClawLessonProposalOutcome {
    CreateNew {
        lesson_title: String,
        source_memory_ids: Vec<NodeId>,
    },
    ReinforceExisting {
        lesson_title: String,
        evidence_node_ids: Vec<NodeId>,
    },
    RefineExisting {
        lesson_title: String,
        evidence_node_ids: Vec<NodeId>,
    },
    ContradictionHook {
        target_lesson_id: memory_core::LessonId,
        evidence_node_ids: Vec<NodeId>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawProposeLessonResponse {
    pub outcomes: Vec<OpenClawLessonProposalOutcome>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawRecordOutcomeResponse {
    pub updated_trait_count: usize,
    pub updates: Vec<OpenClawTraitUpdateSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawTraitUpdateSummary {
    pub trait_type: TraitType,
    pub previous_strength: f32,
    pub updated_strength: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawGenerateImaginedScenariosResponse {
    pub planning_task: String,
    pub scenarios: Vec<OpenClawImaginedScenarioSummary>,
}

impl From<OpenClawRecallContextRequest> for RecallContextRequest {
    fn from(value: OpenClawRecallContextRequest) -> Self {
        Self {
            text: value.text,
            session_id: value.session_id,
            topic: value.topic,
            nodes: value.nodes,
            edges: value.edges,
            lessons: value.lessons,
            checkpoints: value.checkpoints,
            traits: value.traits,
        }
    }
}

impl From<OpenClawGetNeighborsRequest> for GetNeighborsRequest {
    fn from(value: OpenClawGetNeighborsRequest) -> Self {
        Self {
            node_id: value.node_id,
            nodes: value.nodes,
            edges: value.edges,
        }
    }
}

impl From<OpenClawProposeMemoryRequest> for ProposeMemoryRequest {
    fn from(value: OpenClawProposeMemoryRequest) -> Self {
        Self {
            event: value.event,
            context: value.context,
        }
    }
}

impl From<OpenClawProposeLessonRequest> for ProposeLessonRequest {
    fn from(value: OpenClawProposeLessonRequest) -> Self {
        Self {
            accepted_memories: value.accepted_memories,
            existing_lessons: value.existing_lessons,
        }
    }
}

impl From<OpenClawRecordOutcomeRequest> for crate::RecordOutcomeRequest {
    fn from(value: OpenClawRecordOutcomeRequest) -> Self {
        Self {
            existing_traits: value.existing_traits,
            outcome: value.outcome,
        }
    }
}

impl From<OpenClawGenerateImaginedScenariosRequest> for GenerateImaginedScenariosRequest {
    fn from(value: OpenClawGenerateImaginedScenariosRequest) -> Self {
        Self {
            planning_task: value.planning_task,
            desired_scenarios: value.desired_scenarios,
            context_packet: value.context_packet,
            active_goal_node_ids: value.active_goal_node_ids,
        }
    }
}

impl From<memory_core::Node> for OpenClawNodeSummary {
    fn from(value: memory_core::Node) -> Self {
        Self {
            node_id: value.id,
            title: value.title,
            summary: value.summary,
            tags: value.tags,
            confidence: value.confidence,
            importance: value.importance,
        }
    }
}

impl From<memory_core::Lesson> for OpenClawLessonSummary {
    fn from(value: memory_core::Lesson) -> Self {
        Self {
            lesson_id: value.id,
            lesson_type: value.lesson_type,
            title: value.title,
            statement: value.statement,
            confidence: value.confidence,
        }
    }
}

impl From<memory_core::TraitState> for OpenClawTraitSummary {
    fn from(value: memory_core::TraitState) -> Self {
        Self {
            trait_type: value.trait_type,
            label: value.label,
            strength: value.strength,
            confidence: value.confidence,
        }
    }
}

impl From<memory_core::ImaginedScenario> for OpenClawImaginedScenarioSummary {
    fn from(value: memory_core::ImaginedScenario) -> Self {
        Self {
            scenario_id: value.id,
            title: value.title,
            premise: value.premise,
            predicted_outcomes: value.predicted_outcomes,
            plausibility_score: value.plausibility_score,
            novelty_score: value.novelty_score,
            usefulness_score: value.usefulness_score,
            hypothetical: true,
        }
    }
}

impl From<AdmissionDecision> for OpenClawAdmissionDecisionSummary {
    fn from(value: AdmissionDecision) -> Self {
        Self {
            candidate_node_id: value.candidate_node_id,
            action: value.action,
            matched_node_id: value.matched_node_id,
            reason: value.reason,
            total_score: value.score.total,
        }
    }
}

pub fn checkpoint_summary_text(checkpoint: Option<Checkpoint>) -> Option<String> {
    checkpoint.map(|entry| format!("{}: {}", entry.title, entry.summary))
}

pub fn compact_summary_line(
    node_count: usize,
    lesson_count: usize,
    has_checkpoint: bool,
    trait_count: usize,
) -> String {
    let checkpoint_text = if has_checkpoint {
        "with checkpoint context"
    } else {
        "without checkpoint context"
    };

    format!(
        "Recalled {node_count} verified nodes and {lesson_count} lessons {checkpoint_text}; trait snapshot includes {trait_count} entries."
    )
}
