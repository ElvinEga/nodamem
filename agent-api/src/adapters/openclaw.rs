use std::sync::Arc;

use crate::{
    AgentApiError, AgentMemoryService, GenerateImaginedScenariosResponse, GetNeighborsResponse,
    ProposeLessonResponse, ProposeMemoryResponse, RecallContextResponse, RecordOutcomeResponse,
};

use super::openclaw_tools::{openclaw_tool_descriptions, OpenClawToolDescription};
use super::openclaw_types::{
    checkpoint_summary_text, compact_summary_line, OpenClawAdmissionDecisionSummary,
    OpenClawGenerateImaginedScenariosRequest, OpenClawGenerateImaginedScenariosResponse,
    OpenClawGetNeighborsRequest, OpenClawGetNeighborsResponse, OpenClawImaginedScenarioSummary,
    OpenClawLessonProposalOutcome, OpenClawLessonSummary, OpenClawNodeSummary,
    OpenClawProposeLessonRequest, OpenClawProposeLessonResponse, OpenClawProposeMemoryRequest,
    OpenClawProposeMemoryResponse, OpenClawRecallContextRequest, OpenClawRecallContextResponse,
    OpenClawRecordOutcomeRequest, OpenClawRecordOutcomeResponse, OpenClawTraitSummary,
    OpenClawTraitUpdateSummary,
};
use crate::LessonOutcomeDto;

#[derive(Debug, Clone)]
pub struct OpenClawAdapter<S> {
    service: Arc<S>,
}

impl<S> OpenClawAdapter<S>
where
    S: AgentMemoryService,
{
    #[must_use]
    pub fn new(service: Arc<S>) -> Self {
        Self { service }
    }

    pub fn recall_context(
        &self,
        request: OpenClawRecallContextRequest,
    ) -> Result<OpenClawRecallContextResponse, AgentApiError> {
        let response = self.service.recall_context(&request.into())?;
        Ok(compact_recall_context(response))
    }

    pub fn get_neighbors(
        &self,
        request: OpenClawGetNeighborsRequest,
    ) -> Result<OpenClawGetNeighborsResponse, AgentApiError> {
        let response = self.service.get_neighbors(&request.into())?;
        Ok(compact_neighbors(response))
    }

    pub fn propose_memory(
        &self,
        request: OpenClawProposeMemoryRequest,
    ) -> Result<OpenClawProposeMemoryResponse, AgentApiError> {
        let response = self.service.propose_memory(&request.into())?;
        Ok(compact_propose_memory(response))
    }

    pub fn propose_lesson(
        &self,
        request: OpenClawProposeLessonRequest,
    ) -> Result<OpenClawProposeLessonResponse, AgentApiError> {
        let response = self.service.propose_lesson(&request.into())?;
        Ok(compact_propose_lesson(response))
    }

    pub fn record_outcome(
        &self,
        request: OpenClawRecordOutcomeRequest,
    ) -> Result<OpenClawRecordOutcomeResponse, AgentApiError> {
        let response = self.service.record_outcome(&request.into())?;
        Ok(compact_record_outcome(response))
    }

    pub fn generate_imagined_scenarios(
        &self,
        request: OpenClawGenerateImaginedScenariosRequest,
    ) -> Result<OpenClawGenerateImaginedScenariosResponse, AgentApiError> {
        let response = self.service.generate_imagined_scenarios(&request.into())?;
        Ok(compact_imagined_scenarios(response))
    }

    #[must_use]
    pub fn tool_descriptions(&self) -> Vec<OpenClawToolDescription> {
        openclaw_tool_descriptions()
    }
}

#[must_use]
pub fn compact_recall_context(response: RecallContextResponse) -> OpenClawRecallContextResponse {
    OpenClawRecallContextResponse {
        summary: compact_summary_line(
            response.core_nodes.len() + response.related_neighbors.len(),
            response.lessons.len(),
            response.checkpoint_summary.is_some(),
            response.trait_snapshot.iter().count(),
        ),
        nodes: response
            .core_nodes
            .into_iter()
            .chain(response.related_neighbors)
            .map(OpenClawNodeSummary::from)
            .collect(),
        lessons: response
            .lessons
            .into_iter()
            .map(OpenClawLessonSummary::from)
            .collect(),
        checkpoint_summary: checkpoint_summary_text(response.checkpoint_summary),
        trait_snapshot: response
            .trait_snapshot
            .into_iter()
            .map(OpenClawTraitSummary::from)
            .collect(),
    }
}

#[must_use]
pub fn compact_neighbors(response: GetNeighborsResponse) -> OpenClawGetNeighborsResponse {
    OpenClawGetNeighborsResponse {
        node_id: response.node_id,
        neighbors: response
            .neighbors
            .into_iter()
            .map(OpenClawNodeSummary::from)
            .collect(),
        connection_count: response.connecting_edges.len(),
    }
}

#[must_use]
pub fn compact_propose_memory(response: ProposeMemoryResponse) -> OpenClawProposeMemoryResponse {
    OpenClawProposeMemoryResponse {
        candidate_node_count: response.ingest_output.candidate_nodes.len(),
        candidate_lesson_count: response.ingest_output.candidate_lessons.len(),
        decisions: response
            .admission_decisions
            .into_iter()
            .map(OpenClawAdmissionDecisionSummary::from)
            .collect(),
    }
}

#[must_use]
pub fn compact_propose_lesson(response: ProposeLessonResponse) -> OpenClawProposeLessonResponse {
    OpenClawProposeLessonResponse {
        outcomes: response
            .outcomes
            .into_iter()
            .map(|outcome| match outcome {
                LessonOutcomeDto::CreateNew {
                    lesson,
                    source_memory_ids,
                } => OpenClawLessonProposalOutcome::CreateNew {
                    lesson_title: lesson.title,
                    source_memory_ids,
                },
                LessonOutcomeDto::ReinforceExisting {
                    updated_lesson,
                    evidence_links,
                } => OpenClawLessonProposalOutcome::ReinforceExisting {
                    lesson_title: updated_lesson.title,
                    evidence_node_ids: evidence_links
                        .into_iter()
                        .map(|link| link.node_id)
                        .collect(),
                },
                LessonOutcomeDto::RefineExisting {
                    updated_lesson,
                    evidence_links,
                } => OpenClawLessonProposalOutcome::RefineExisting {
                    lesson_title: updated_lesson.title,
                    evidence_node_ids: evidence_links
                        .into_iter()
                        .map(|link| link.node_id)
                        .collect(),
                },
                LessonOutcomeDto::ContradictionHook {
                    target_lesson_id,
                    evidence_links,
                } => OpenClawLessonProposalOutcome::ContradictionHook {
                    target_lesson_id,
                    evidence_node_ids: evidence_links
                        .into_iter()
                        .map(|link| link.node_id)
                        .collect(),
                },
            })
            .collect(),
    }
}

#[must_use]
pub fn compact_record_outcome(response: RecordOutcomeResponse) -> OpenClawRecordOutcomeResponse {
    OpenClawRecordOutcomeResponse {
        updated_trait_count: response.updated_traits.len(),
        updates: response
            .updates
            .into_iter()
            .map(|update| OpenClawTraitUpdateSummary {
                trait_type: update.trait_type,
                previous_strength: update.previous_strength,
                updated_strength: update.updated_strength,
            })
            .collect(),
    }
}

#[must_use]
pub fn compact_imagined_scenarios(
    response: GenerateImaginedScenariosResponse,
) -> OpenClawGenerateImaginedScenariosResponse {
    OpenClawGenerateImaginedScenariosResponse {
        planning_task: response.planning_task,
        scenarios: response
            .scenarios
            .into_iter()
            .map(OpenClawImaginedScenarioSummary::from)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::Utc;
    use memory_core::{
        AdmissionAction, AdmissionDecision, AdmissionScore, ImaginationStatus, Lesson, LessonId,
        LessonType, MemoryPacket, MemoryPacketId, MemoryStatus, Node, NodeId, NodeType, ScenarioId,
        TraitId, TraitState, TraitType,
    };
    use memory_ingest::{AdmissionContext, IngestEvent, IngestOutput, MessageEvent};
    use uuid::Uuid;

    use super::{compact_recall_context, OpenClawAdapter};
    use crate::adapters::openclaw_types::{
        OpenClawGenerateImaginedScenariosRequest, OpenClawProposeLessonRequest,
        OpenClawProposeMemoryRequest, OpenClawRecallContextRequest,
    };
    use crate::{
        AgentApiError, AgentMemoryService, GenerateImaginedScenariosRequest,
        GenerateImaginedScenariosResponse, GetNeighborsRequest, GetNeighborsResponse,
        ProposeLessonRequest, ProposeLessonResponse, ProposeMemoryRequest, ProposeMemoryResponse,
        RecallContextRequest, RecallContextResponse, RecordOutcomeRequest, RecordOutcomeResponse,
    };

    #[test]
    fn recall_context_compacts_verified_outputs() {
        let response = compact_recall_context(RecallContextResponse {
            packet: sample_packet(),
            core_nodes: vec![sample_node("Architecture decision")],
            related_neighbors: vec![sample_node("Migration notes")],
            lessons: vec![sample_lesson()],
            checkpoint_summary: None,
            trait_snapshot: Some(sample_trait()),
        });

        assert_eq!(response.nodes.len(), 2);
        assert_eq!(response.lessons.len(), 1);
        assert_eq!(response.trait_snapshot.len(), 1);
        assert!(response.summary.contains("verified nodes"));
    }

    #[test]
    fn adapter_maps_requests_to_service_calls() {
        let service = Arc::new(MockService::default());
        let adapter = OpenClawAdapter::new(service.clone());

        let request = OpenClawRecallContextRequest {
            text: "Continue architecture discussion".to_owned(),
            session_id: Some("session-1".to_owned()),
            topic: Some("architecture".to_owned()),
            nodes: vec![sample_node("Architecture decision")],
            edges: Vec::new(),
            lessons: Vec::new(),
            checkpoints: Vec::new(),
            traits: Vec::new(),
        };

        let response = adapter
            .recall_context(request)
            .expect("adapter call should succeed");

        assert_eq!(response.nodes.len(), 1);
        assert_eq!(service.recorded_calls(), vec!["recall_context"]);
    }

    #[test]
    fn propose_memory_and_lesson_surface_validation_results() {
        let service = Arc::new(MockService::default());
        let adapter = OpenClawAdapter::new(service);

        let memory_response = adapter
            .propose_memory(OpenClawProposeMemoryRequest {
                event: IngestEvent::UserMessage(MessageEvent {
                    event_id: "evt-1".to_owned(),
                    session_id: Some("session-1".to_owned()),
                    message_id: Some("msg-1".to_owned()),
                    text: "Remember rollout notes.".to_owned(),
                }),
                context: AdmissionContext::default(),
            })
            .expect("memory proposal should succeed");
        assert_eq!(
            memory_response.decisions[0].action,
            AdmissionAction::CreateNewNode
        );

        let lesson_response = adapter
            .propose_lesson(OpenClawProposeLessonRequest {
                accepted_memories: vec![sample_node("Architecture decision")],
                existing_lessons: vec![sample_lesson()],
            })
            .expect("lesson proposal should succeed");
        assert!(!lesson_response.outcomes.is_empty());
    }

    #[test]
    fn imagined_scenarios_remain_hypothetical_in_outputs() {
        let service = Arc::new(MockService::default());
        let adapter = OpenClawAdapter::new(service);

        let response = adapter
            .generate_imagined_scenarios(OpenClawGenerateImaginedScenariosRequest {
                planning_task: "Plan next release".to_owned(),
                desired_scenarios: 1,
                context_packet: sample_packet(),
                active_goal_node_ids: Vec::new(),
            })
            .expect("imagination call should succeed");

        assert_eq!(response.scenarios.len(), 1);
        assert!(response.scenarios[0].hypothetical);
    }

    #[derive(Debug, Default)]
    struct MockService {
        calls: Mutex<Vec<&'static str>>,
    }

    impl MockService {
        fn recorded_calls(&self) -> Vec<&'static str> {
            self.calls.lock().expect("calls lock should work").clone()
        }

        fn push_call(&self, name: &'static str) {
            self.calls
                .lock()
                .expect("calls lock should work")
                .push(name);
        }
    }

    impl AgentMemoryService for MockService {
        fn recall_context(
            &self,
            _request: &RecallContextRequest,
        ) -> Result<RecallContextResponse, AgentApiError> {
            self.push_call("recall_context");
            Ok(RecallContextResponse {
                packet: sample_packet(),
                core_nodes: vec![sample_node("Architecture decision")],
                related_neighbors: Vec::new(),
                lessons: vec![sample_lesson()],
                checkpoint_summary: None,
                trait_snapshot: Some(sample_trait()),
            })
        }

        fn get_neighbors(
            &self,
            _request: &GetNeighborsRequest,
        ) -> Result<GetNeighborsResponse, AgentApiError> {
            self.push_call("get_neighbors");
            Ok(GetNeighborsResponse {
                node_id: sample_node("Architecture decision").id,
                neighbors: vec![sample_node("Migration notes")],
                connecting_edges: Vec::new(),
            })
        }

        fn propose_memory(
            &self,
            _request: &ProposeMemoryRequest,
        ) -> Result<ProposeMemoryResponse, AgentApiError> {
            self.push_call("propose_memory");
            Ok(ProposeMemoryResponse {
                ingest_output: IngestOutput {
                    candidate_nodes: vec![sample_node("Architecture decision")],
                    candidate_edges: Vec::new(),
                    candidate_lessons: vec![sample_lesson()],
                    salience_score: 0.8,
                    extracted_entities: Vec::new(),
                },
                admission_decisions: vec![AdmissionDecision {
                    candidate_node_id: sample_node("Architecture decision").id,
                    action: AdmissionAction::CreateNewNode,
                    score: AdmissionScore {
                        connectedness: 0.8,
                        usefulness: 0.8,
                        recurrence: 0.4,
                        novelty: 0.6,
                        importance: 0.8,
                        total: 0.72,
                    },
                    matched_node_id: None,
                    reason: "validated candidate".to_owned(),
                }],
            })
        }

        fn propose_lesson(
            &self,
            _request: &ProposeLessonRequest,
        ) -> Result<ProposeLessonResponse, AgentApiError> {
            self.push_call("propose_lesson");
            Ok(ProposeLessonResponse {
                outcomes: vec![crate::LessonOutcomeDto::CreateNew {
                    lesson: sample_lesson(),
                    source_memory_ids: vec![sample_node("Architecture decision").id],
                }],
            })
        }

        fn record_outcome(
            &self,
            _request: &RecordOutcomeRequest,
        ) -> Result<RecordOutcomeResponse, AgentApiError> {
            self.push_call("record_outcome");
            Ok(RecordOutcomeResponse {
                updated_traits: vec![sample_trait()],
                updates: vec![crate::TraitUpdateDto {
                    trait_type: TraitType::Practicality,
                    previous_strength: 0.5,
                    updated_strength: 0.6,
                }],
            })
        }

        fn generate_imagined_scenarios(
            &self,
            _request: &GenerateImaginedScenariosRequest,
        ) -> Result<GenerateImaginedScenariosResponse, AgentApiError> {
            self.push_call("generate_imagined_scenarios");
            Ok(GenerateImaginedScenariosResponse {
                planning_task: "Plan next release".to_owned(),
                scenarios: vec![memory_core::ImaginedScenario {
                    id: ScenarioId(Uuid::new_v4()),
                    status: ImaginationStatus::Proposed,
                    title: "Hypothetical scenario".to_owned(),
                    premise: "If rollout notes are improved, onboarding may speed up.".to_owned(),
                    narrative: "This is hypothetical.".to_owned(),
                    basis_source_node_ids: vec![sample_node("Architecture decision").id],
                    basis_lesson_ids: vec![sample_lesson().id],
                    active_goal_node_ids: Vec::new(),
                    trait_snapshot: vec![sample_trait()],
                    predicted_outcomes: vec!["Faster planning alignment.".to_owned()],
                    plausibility_score: 0.7,
                    novelty_score: 0.5,
                    usefulness_score: 0.8,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                }],
            })
        }
    }

    fn sample_packet() -> MemoryPacket {
        MemoryPacket {
            id: MemoryPacketId(Uuid::new_v4()),
            request_id: Some("req-1".to_owned()),
            created_at: Utc::now(),
            nodes: vec![sample_node("Architecture decision")],
            edges: Vec::new(),
            lessons: vec![sample_lesson()],
            traits: vec![sample_trait()],
            checkpoints: Vec::new(),
            imagined_scenarios: Vec::new(),
        }
    }

    fn sample_node(title: &str) -> Node {
        Node {
            id: NodeId(Uuid::new_v4()),
            node_type: NodeType::Semantic,
            status: MemoryStatus::Active,
            title: title.to_owned(),
            summary: format!("{title} summary"),
            content: Some(format!("{title} content")),
            tags: vec!["architecture".to_owned()],
            confidence: 0.8,
            importance: 0.8,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            source_event_id: Some("evt-1".to_owned()),
        }
    }

    fn sample_lesson() -> Lesson {
        Lesson {
            id: LessonId(Uuid::new_v4()),
            lesson_type: LessonType::Strategy,
            status: MemoryStatus::Active,
            title: "Track rollout notes".to_owned(),
            statement: "Release planning works better when rollout notes are explicit.".to_owned(),
            confidence: 0.8,
            evidence_count: 2,
            reinforcement_count: 1,
            supporting_node_ids: vec![sample_node("Architecture decision").id],
            contradicting_node_ids: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn sample_trait() -> TraitState {
        TraitState {
            id: TraitId(Uuid::new_v4()),
            trait_type: TraitType::Practicality,
            status: MemoryStatus::Active,
            label: "Practicality".to_owned(),
            description: "Optimizes for workable outcomes.".to_owned(),
            strength: 0.7,
            confidence: 0.7,
            supporting_lesson_ids: vec![sample_lesson().id],
            supporting_node_ids: vec![sample_node("Architecture decision").id],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
