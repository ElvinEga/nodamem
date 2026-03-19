//! Agent integration layer for Nodamem.

use std::collections::HashSet;
use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use libsql::Connection;
use memory_core::{Checkpoint, CoreMarker, Edge, Lesson, MemoryPacket, Node, NodeId, TraitState};
use memory_imagination::PlanningImaginationRequest;
pub use memory_imagination::{
    ImaginationError, ImaginationPolicy, ImaginationService, PlanningImaginationApi,
};
use memory_ingest::{
    AdmissionContext, AdmissionEngine, DeterministicDuplicateDetector, EventExtractor, IngestEvent,
    IngestOutput, IngestPipeline, MemoryAdmission,
};
use memory_lessons::{
    DeterministicContradictionHandler, DeterministicLessonMatcher, EvidenceRole, LessonOutcome,
    LessonService,
};
use memory_personality::{OutcomeRecord, PersonalityService, TraitUpdate};
use memory_retrieval::{
    vector::{DeterministicQueryEmbedder, NullVectorSearch, TursoVectorSearch, VectorSearch},
    MemoryQuery, RetrievalEngine, RetrievalError, RetrievalPolicy, RetrievalSource,
};
use memory_sleep::{EveryNRecallBatches, SleepMarker, SleepPolicy, StoreSleepService};
use memory_store::StoreMarker;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tracing::{debug, warn};

use memory_imagination::ImaginationMarker;
use memory_ingest::IngestMarker;
use memory_lessons::LessonsMarker;
use memory_personality::PersonalityMarker;
use memory_retrieval::RetrievalMarker;

pub mod adapters;

/// Lightweight marker preserved for crate wiring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentApi {
    pub core: CoreMarker,
    pub imagination: ImaginationMarker,
    pub ingest: IngestMarker,
    pub lessons: LessonsMarker,
    pub personality: PersonalityMarker,
    pub retrieval: RetrievalMarker,
    pub sleep: SleepMarker,
    pub store: StoreMarker,
}

/// Service-layer error for agent-facing operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentApiError {
    BadRequest(String),
    Retrieval(String),
    Imagination(String),
}

impl fmt::Display for AgentApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest(message) => write!(formatter, "bad request: {message}"),
            Self::Retrieval(message) => write!(formatter, "retrieval error: {message}"),
            Self::Imagination(message) => write!(formatter, "imagination error: {message}"),
        }
    }
}

impl StdError for AgentApiError {}

impl From<RetrievalError> for AgentApiError {
    fn from(error: RetrievalError) -> Self {
        Self::Retrieval(error.to_string())
    }
}

impl From<ImaginationError> for AgentApiError {
    fn from(error: ImaginationError) -> Self {
        Self::Imagination(error.to_string())
    }
}

/// Tool description for external agents integrating with the HTTP API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentToolDescription {
    pub name: String,
    pub method: String,
    pub path: String,
    pub description: String,
    pub example_request: JsonValue,
}

/// DTO for `/recall-context`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallContextRequest {
    pub text: String,
    pub session_id: Option<String>,
    pub topic: Option<String>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub lessons: Vec<Lesson>,
    pub checkpoints: Vec<Checkpoint>,
    pub traits: Vec<TraitState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallContextResponse {
    pub packet: MemoryPacket,
    pub core_nodes: Vec<Node>,
    pub related_neighbors: Vec<Node>,
    pub lessons: Vec<Lesson>,
    pub checkpoint_summary: Option<Checkpoint>,
    pub trait_snapshot: Option<TraitState>,
}

/// DTO for `/get-neighbors`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetNeighborsRequest {
    pub node_id: NodeId,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetNeighborsResponse {
    pub node_id: NodeId,
    pub neighbors: Vec<Node>,
    pub connecting_edges: Vec<Edge>,
}

/// DTO for `/propose-memory`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposeMemoryRequest {
    pub event: IngestEvent,
    pub context: AdmissionContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposeMemoryResponse {
    pub ingest_output: IngestOutput,
    pub admission_decisions: Vec<memory_core::AdmissionDecision>,
}

/// DTO for `/propose-lesson`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposeLessonRequest {
    pub accepted_memories: Vec<Node>,
    pub existing_lessons: Vec<Lesson>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LessonOutcomeDto {
    CreateNew {
        lesson: Lesson,
        source_memory_ids: Vec<NodeId>,
    },
    ReinforceExisting {
        updated_lesson: Lesson,
        evidence_links: Vec<LessonEvidenceLinkDto>,
    },
    RefineExisting {
        updated_lesson: Lesson,
        evidence_links: Vec<LessonEvidenceLinkDto>,
    },
    ContradictionHook {
        target_lesson_id: memory_core::LessonId,
        evidence_links: Vec<LessonEvidenceLinkDto>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LessonEvidenceLinkDto {
    pub lesson_id: memory_core::LessonId,
    pub node_id: NodeId,
    pub role: EvidenceRoleDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRoleDto {
    Supporting,
    Contradicting,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposeLessonResponse {
    pub outcomes: Vec<LessonOutcomeDto>,
}

/// DTO for `/record-outcome`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordOutcomeRequest {
    pub existing_traits: Vec<TraitState>,
    pub outcome: OutcomeRecordDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutcomeRecordDto {
    pub outcome_id: String,
    pub subject_node_id: Option<NodeId>,
    pub success: bool,
    pub usefulness: f32,
    pub prediction_correct: bool,
    pub user_accepted: bool,
    pub validated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraitUpdateDto {
    pub trait_type: memory_core::TraitType,
    pub previous_strength: f32,
    pub updated_strength: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordOutcomeResponse {
    pub updated_traits: Vec<TraitState>,
    pub updates: Vec<TraitUpdateDto>,
}

/// DTO for `/generate-imagined-scenarios`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerateImaginedScenariosRequest {
    pub planning_task: String,
    pub desired_scenarios: usize,
    pub context_packet: MemoryPacket,
    pub active_goal_node_ids: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerateImaginedScenariosResponse {
    pub planning_task: String,
    pub scenarios: Vec<memory_core::ImaginedScenario>,
}

/// JSON error response used by the HTTP transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl IntoResponse for AgentApiError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Retrieval(_) | Self::Imagination(_) => StatusCode::UNPROCESSABLE_ENTITY,
        };

        let body = Json(ErrorResponse {
            error: self.to_string(),
        });

        (status, body).into_response()
    }
}

/// Internal service façade. It composes existing memory crates but stays independent of transport.
pub struct AgentApiService {
    ingest_pipeline: IngestPipeline,
    admission_engine: AdmissionEngine<DeterministicDuplicateDetector>,
    lesson_service: LessonService<DeterministicLessonMatcher, DeterministicContradictionHandler>,
    personality_service: PersonalityService,
    imagination_service: ImaginationService,
    vector_search: Box<dyn VectorSearch>,
    sleep_service: Option<StoreSleepService<EveryNRecallBatches>>,
}

impl fmt::Debug for AgentApiService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentApiService")
            .field("ingest_pipeline", &self.ingest_pipeline)
            .field("admission_engine", &self.admission_engine)
            .field("lesson_service", &self.lesson_service)
            .field("personality_service", &self.personality_service)
            .field("imagination_service", &self.imagination_service)
            .field("vector_search", &"dyn VectorSearch")
            .field("sleep_service", &self.sleep_service.is_some())
            .finish()
    }
}

/// Internal service interface that adapter layers can depend on without touching transport or storage.
pub trait AgentMemoryService: Send + Sync {
    fn recall_context(
        &self,
        request: &RecallContextRequest,
    ) -> Result<RecallContextResponse, AgentApiError>;

    fn get_neighbors(
        &self,
        request: &GetNeighborsRequest,
    ) -> Result<GetNeighborsResponse, AgentApiError>;

    fn propose_memory(
        &self,
        request: &ProposeMemoryRequest,
    ) -> Result<ProposeMemoryResponse, AgentApiError>;

    fn propose_lesson(
        &self,
        request: &ProposeLessonRequest,
    ) -> Result<ProposeLessonResponse, AgentApiError>;

    fn record_outcome(
        &self,
        request: &RecordOutcomeRequest,
    ) -> Result<RecordOutcomeResponse, AgentApiError>;

    fn generate_imagined_scenarios(
        &self,
        request: &GenerateImaginedScenariosRequest,
    ) -> Result<GenerateImaginedScenariosResponse, AgentApiError>;
}

impl Default for AgentApiService {
    fn default() -> Self {
        Self {
            ingest_pipeline: IngestPipeline::default(),
            admission_engine: AdmissionEngine::default(),
            lesson_service: LessonService::default(),
            personality_service: PersonalityService::default(),
            imagination_service: ImaginationService::default(),
            vector_search: Box::new(NullVectorSearch),
            sleep_service: None,
        }
    }
}

impl AgentApiService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_vector_search<V>(mut self, vector_search: V) -> Self
    where
        V: VectorSearch + 'static,
    {
        self.vector_search = Box::new(vector_search);
        self
    }

    #[must_use]
    pub fn new_with_connection(connection: Connection) -> Self {
        Self::new_with_store_connections(connection, None)
    }

    #[must_use]
    pub fn new_with_store_connections(
        vector_connection: Connection,
        maintenance_connection: Option<Connection>,
    ) -> Self {
        Self::default()
            .with_vector_search(TursoVectorSearch::new(
                vector_connection,
                DeterministicQueryEmbedder::default(),
            ))
            .with_sleep_service(maintenance_connection.map(|connection| {
                StoreSleepService::new(
                    connection,
                    memory_sleep::SleepRunner::default(),
                    SleepPolicy::default(),
                    EveryNRecallBatches::default(),
                )
            }))
    }

    #[must_use]
    pub fn with_sleep_service(
        mut self,
        sleep_service: Option<StoreSleepService<EveryNRecallBatches>>,
    ) -> Self {
        self.sleep_service = sleep_service;
        self
    }

    #[must_use]
    fn block_on_sleep_service<F>(future: F) -> Result<F::Output, AgentApiError>
    where
        F: std::future::Future,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            Ok(tokio::task::block_in_place(|| handle.block_on(future)))
        } else {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| AgentApiError::Retrieval(error.to_string()))?;
            Ok(runtime.block_on(future))
        }
    }

    fn record_recall_side_effect(&self, node_ids: &[NodeId]) {
        let Some(sleep_service) = &self.sleep_service else {
            return;
        };

        match Self::block_on_sleep_service(
            sleep_service.record_recall_and_maybe_run(node_ids, chrono::Utc::now()),
        ) {
            Ok(Ok(report)) => {
                if !report.job_reports.is_empty() {
                    debug!(
                        recall_nodes_recorded = report.recall_nodes_recorded,
                        job_runs = report.job_reports.len(),
                        "nodamem sleep maintenance ran inline after recall"
                    );
                }
            }
            Ok(Err(error)) => {
                warn!(%error, "failed to persist nodamem sleep maintenance side effect");
            }
            Err(error) => {
                warn!(%error, "failed to execute nodamem sleep maintenance side effect");
            }
        }
    }

    pub fn recall_context(
        &self,
        request: &RecallContextRequest,
    ) -> Result<RecallContextResponse, AgentApiError> {
        if request.text.trim().is_empty() {
            return Err(AgentApiError::BadRequest(
                "recall_context requires non-empty text".to_owned(),
            ));
        }

        let source = RequestRetrievalSource {
            nodes: request.nodes.clone(),
            edges: request.edges.clone(),
            lessons: request.lessons.clone(),
            checkpoints: request.checkpoints.clone(),
            traits: request.traits.clone(),
        };
        let engine = RetrievalEngine::new(source, &*self.vector_search, RetrievalPolicy::default());
        let retrieved = engine.recall_context(&MemoryQuery {
            text: request.text.clone(),
            session_id: request.session_id.clone(),
            topic: request.topic.clone(),
        })?;
        let recalled_node_ids = retrieved
            .packet
            .nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        self.record_recall_side_effect(&recalled_node_ids);

        Ok(RecallContextResponse {
            packet: retrieved.packet,
            core_nodes: retrieved.core_nodes,
            related_neighbors: retrieved.related_neighbors,
            lessons: retrieved.lessons,
            checkpoint_summary: retrieved.checkpoint_summary,
            trait_snapshot: retrieved.trait_snapshot,
        })
    }

    pub fn get_neighbors(
        &self,
        request: &GetNeighborsRequest,
    ) -> Result<GetNeighborsResponse, AgentApiError> {
        let selected_ids = request
            .edges
            .iter()
            .filter_map(|edge| {
                if edge.from_node_id == request.node_id {
                    Some(edge.to_node_id)
                } else if edge.to_node_id == request.node_id {
                    Some(edge.from_node_id)
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>();

        let neighbors = request
            .nodes
            .iter()
            .filter(|node| selected_ids.contains(&node.id))
            .cloned()
            .collect::<Vec<_>>();
        let connecting_edges = request
            .edges
            .iter()
            .filter(|edge| {
                edge.from_node_id == request.node_id || edge.to_node_id == request.node_id
            })
            .cloned()
            .collect::<Vec<_>>();

        Ok(GetNeighborsResponse {
            node_id: request.node_id,
            neighbors,
            connecting_edges,
        })
    }

    pub fn propose_memory(
        &self,
        request: &ProposeMemoryRequest,
    ) -> Result<ProposeMemoryResponse, AgentApiError> {
        let ingest_output = self.ingest_pipeline.extract(&request.event);
        let admission_decisions = self
            .admission_engine
            .evaluate(&ingest_output, &request.context);

        Ok(ProposeMemoryResponse {
            ingest_output,
            admission_decisions,
        })
    }

    pub fn propose_lesson(
        &self,
        request: &ProposeLessonRequest,
    ) -> Result<ProposeLessonResponse, AgentApiError> {
        let outcomes = self
            .lesson_service
            .process_memories(&request.accepted_memories, &request.existing_lessons)
            .into_iter()
            .map(LessonOutcomeDto::from)
            .collect();

        Ok(ProposeLessonResponse { outcomes })
    }

    pub fn record_outcome(
        &self,
        request: &RecordOutcomeRequest,
    ) -> Result<RecordOutcomeResponse, AgentApiError> {
        let outcome = OutcomeRecord::from(request.outcome.clone());
        let (updated_traits, updates) = self
            .personality_service
            .record_outcome(&request.existing_traits, &outcome);

        Ok(RecordOutcomeResponse {
            updated_traits,
            updates: updates.into_iter().map(TraitUpdateDto::from).collect(),
        })
    }

    pub fn generate_imagined_scenarios(
        &self,
        request: &GenerateImaginedScenariosRequest,
    ) -> Result<GenerateImaginedScenariosResponse, AgentApiError> {
        let response =
            self.imagination_service
                .imagine_for_planning(&PlanningImaginationRequest {
                    planning_task: request.planning_task.clone(),
                    desired_scenarios: request.desired_scenarios,
                    context_packet: request.context_packet.clone(),
                    active_goal_node_ids: request.active_goal_node_ids.clone(),
                })?;

        Ok(GenerateImaginedScenariosResponse {
            planning_task: response.planning_task,
            scenarios: response.scenarios,
        })
    }

    #[must_use]
    pub fn tool_descriptions(&self) -> Vec<AgentToolDescription> {
        tool_descriptions()
    }
}

impl AgentMemoryService for AgentApiService {
    fn recall_context(
        &self,
        request: &RecallContextRequest,
    ) -> Result<RecallContextResponse, AgentApiError> {
        Self::recall_context(self, request)
    }

    fn get_neighbors(
        &self,
        request: &GetNeighborsRequest,
    ) -> Result<GetNeighborsResponse, AgentApiError> {
        Self::get_neighbors(self, request)
    }

    fn propose_memory(
        &self,
        request: &ProposeMemoryRequest,
    ) -> Result<ProposeMemoryResponse, AgentApiError> {
        Self::propose_memory(self, request)
    }

    fn propose_lesson(
        &self,
        request: &ProposeLessonRequest,
    ) -> Result<ProposeLessonResponse, AgentApiError> {
        Self::propose_lesson(self, request)
    }

    fn record_outcome(
        &self,
        request: &RecordOutcomeRequest,
    ) -> Result<RecordOutcomeResponse, AgentApiError> {
        Self::record_outcome(self, request)
    }

    fn generate_imagined_scenarios(
        &self,
        request: &GenerateImaginedScenariosRequest,
    ) -> Result<GenerateImaginedScenariosResponse, AgentApiError> {
        Self::generate_imagined_scenarios(self, request)
    }
}

/// Build the HTTP transport layer. The router only delegates to [`AgentApiService`].
#[must_use]
pub fn build_http_router(service: Arc<AgentApiService>) -> Router {
    Router::new()
        .route("/recall-context", post(recall_context_handler))
        .route("/get-neighbors", post(get_neighbors_handler))
        .route("/propose-memory", post(propose_memory_handler))
        .route("/propose-lesson", post(propose_lesson_handler))
        .route("/record-outcome", post(record_outcome_handler))
        .route(
            "/generate-imagined-scenarios",
            post(generate_imagined_scenarios_handler),
        )
        .with_state(service)
}

#[must_use]
pub fn tool_descriptions() -> Vec<AgentToolDescription> {
    vec![
        AgentToolDescription {
            name: "recall_context".to_owned(),
            method: "POST".to_owned(),
            path: "/recall-context".to_owned(),
            description: "Retrieve a curated memory packet for a task without exposing raw tables."
                .to_owned(),
            example_request: json!({
                "text": "Find relevant release planning context",
                "session_id": "session-1",
                "topic": "planning",
                "nodes": [],
                "edges": [],
                "lessons": [],
                "checkpoints": [],
                "traits": []
            }),
        },
        AgentToolDescription {
            name: "get_neighbors".to_owned(),
            method: "POST".to_owned(),
            path: "/get-neighbors".to_owned(),
            description: "Inspect the local graph neighborhood for a single node.".to_owned(),
            example_request: json!({
                "node_id": "00000000-0000-0000-0000-000000000000",
                "nodes": [],
                "edges": []
            }),
        },
        AgentToolDescription {
            name: "propose_memory".to_owned(),
            method: "POST".to_owned(),
            path: "/propose-memory".to_owned(),
            description: "Run ingestion and admission policy for a candidate memory event."
                .to_owned(),
            example_request: json!({
                "event": {
                    "UserMessage": {
                        "event_id": "evt-1",
                        "session_id": "session-1",
                        "message_id": "msg-1",
                        "text": "Remember that release notes should mention migrations."
                    }
                },
                "context": {
                    "existing_nodes": [],
                    "existing_edges": []
                }
            }),
        },
        AgentToolDescription {
            name: "propose_lesson".to_owned(),
            method: "POST".to_owned(),
            path: "/propose-lesson".to_owned(),
            description: "Evaluate accepted memories against existing lessons.".to_owned(),
            example_request: json!({
                "accepted_memories": [],
                "existing_lessons": []
            }),
        },
        AgentToolDescription {
            name: "record_outcome".to_owned(),
            method: "POST".to_owned(),
            path: "/record-outcome".to_owned(),
            description: "Apply a validated outcome to the trait subsystem.".to_owned(),
            example_request: json!({
                "existing_traits": [],
                "outcome": {
                    "outcome_id": "out-1",
                    "subject_node_id": null,
                    "success": true,
                    "usefulness": 0.9,
                    "prediction_correct": true,
                    "user_accepted": true,
                    "validated": true
                }
            }),
        },
        AgentToolDescription {
            name: "generate_imagined_scenarios".to_owned(),
            method: "POST".to_owned(),
            path: "/generate-imagined-scenarios".to_owned(),
            description: "Generate hypothetical planning scenarios from a verified memory packet."
                .to_owned(),
            example_request: json!({
                "planning_task": "Plan the next release",
                "desired_scenarios": 2,
                "context_packet": {
                    "id": "00000000-0000-0000-0000-000000000000",
                    "request_id": null,
                    "created_at": "2026-01-01T00:00:00Z",
                    "nodes": [],
                    "edges": [],
                    "lessons": [],
                    "traits": [],
                    "checkpoints": [],
                    "imagined_scenarios": []
                },
                "active_goal_node_ids": []
            }),
        },
    ]
}

async fn recall_context_handler(
    State(service): State<Arc<AgentApiService>>,
    Json(request): Json<RecallContextRequest>,
) -> Result<Json<RecallContextResponse>, AgentApiError> {
    Ok(Json(service.recall_context(&request)?))
}

async fn get_neighbors_handler(
    State(service): State<Arc<AgentApiService>>,
    Json(request): Json<GetNeighborsRequest>,
) -> Result<Json<GetNeighborsResponse>, AgentApiError> {
    Ok(Json(service.get_neighbors(&request)?))
}

async fn propose_memory_handler(
    State(service): State<Arc<AgentApiService>>,
    Json(request): Json<ProposeMemoryRequest>,
) -> Result<Json<ProposeMemoryResponse>, AgentApiError> {
    Ok(Json(service.propose_memory(&request)?))
}

async fn propose_lesson_handler(
    State(service): State<Arc<AgentApiService>>,
    Json(request): Json<ProposeLessonRequest>,
) -> Result<Json<ProposeLessonResponse>, AgentApiError> {
    Ok(Json(service.propose_lesson(&request)?))
}

async fn record_outcome_handler(
    State(service): State<Arc<AgentApiService>>,
    Json(request): Json<RecordOutcomeRequest>,
) -> Result<Json<RecordOutcomeResponse>, AgentApiError> {
    Ok(Json(service.record_outcome(&request)?))
}

async fn generate_imagined_scenarios_handler(
    State(service): State<Arc<AgentApiService>>,
    Json(request): Json<GenerateImaginedScenariosRequest>,
) -> Result<Json<GenerateImaginedScenariosResponse>, AgentApiError> {
    Ok(Json(service.generate_imagined_scenarios(&request)?))
}

#[derive(Debug, Clone)]
struct RequestRetrievalSource {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    lessons: Vec<Lesson>,
    checkpoints: Vec<Checkpoint>,
    traits: Vec<TraitState>,
}

impl RetrievalSource for RequestRetrievalSource {
    fn all_nodes(&self) -> Result<Vec<Node>, RetrievalError> {
        Ok(self.nodes.clone())
    }

    fn all_edges(&self) -> Result<Vec<Edge>, RetrievalError> {
        Ok(self.edges.clone())
    }

    fn all_lessons(&self) -> Result<Vec<Lesson>, RetrievalError> {
        Ok(self.lessons.clone())
    }

    fn recent_checkpoints(&self, limit: usize) -> Result<Vec<Checkpoint>, RetrievalError> {
        Ok(self.checkpoints.iter().take(limit).cloned().collect())
    }

    fn current_traits(&self, limit: usize) -> Result<Vec<TraitState>, RetrievalError> {
        Ok(self.traits.iter().take(limit).cloned().collect())
    }
}

impl From<LessonOutcome> for LessonOutcomeDto {
    fn from(value: LessonOutcome) -> Self {
        match value {
            LessonOutcome::CreateNew(proposal) => Self::CreateNew {
                lesson: proposal.lesson,
                source_memory_ids: proposal.source_memory_ids,
            },
            LessonOutcome::ReinforceExisting {
                updated_lesson,
                evidence_links,
            } => Self::ReinforceExisting {
                updated_lesson,
                evidence_links: evidence_links
                    .into_iter()
                    .map(LessonEvidenceLinkDto::from)
                    .collect(),
            },
            LessonOutcome::RefineExisting {
                updated_lesson,
                evidence_links,
            } => Self::RefineExisting {
                updated_lesson,
                evidence_links: evidence_links
                    .into_iter()
                    .map(LessonEvidenceLinkDto::from)
                    .collect(),
            },
            LessonOutcome::ContradictionHook {
                target_lesson_id,
                evidence_links,
            } => Self::ContradictionHook {
                target_lesson_id,
                evidence_links: evidence_links
                    .into_iter()
                    .map(LessonEvidenceLinkDto::from)
                    .collect(),
            },
        }
    }
}

impl From<memory_lessons::LessonEvidenceLink> for LessonEvidenceLinkDto {
    fn from(value: memory_lessons::LessonEvidenceLink) -> Self {
        Self {
            lesson_id: value.lesson_id,
            node_id: value.node_id,
            role: EvidenceRoleDto::from(value.role),
        }
    }
}

impl From<EvidenceRole> for EvidenceRoleDto {
    fn from(value: EvidenceRole) -> Self {
        match value {
            EvidenceRole::Supporting => Self::Supporting,
            EvidenceRole::Contradicting => Self::Contradicting,
        }
    }
}

impl From<OutcomeRecordDto> for OutcomeRecord {
    fn from(value: OutcomeRecordDto) -> Self {
        Self {
            outcome_id: value.outcome_id,
            subject_node_id: value.subject_node_id,
            success: value.success,
            usefulness: value.usefulness,
            prediction_correct: value.prediction_correct,
            user_accepted: value.user_accepted,
            validated: value.validated,
        }
    }
}

impl From<TraitUpdate> for TraitUpdateDto {
    fn from(value: TraitUpdate) -> Self {
        Self {
            trait_type: value.trait_type,
            previous_strength: value.previous_strength,
            updated_strength: value.updated_strength,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::{Method, Request, StatusCode};
    use memory_core::{MemoryStatus, Node, NodeId, NodeType};
    use memory_retrieval::vector::DeterministicQueryEmbedder;
    use memory_store::{NodeEmbeddingRecord, StoreConfig, StoreRuntime};
    use tempfile::tempdir;
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::{build_http_router, tool_descriptions, AgentApiService, RecallContextRequest};

    #[tokio::test]
    async fn exposes_recall_context_endpoint() {
        let router = build_http_router(Arc::new(AgentApiService::default()));
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/recall-context")
                    .header("content-type", "application/json")
                    .body(
                        serde_json::to_vec(&RecallContextRequest {
                            text: "release".to_owned(),
                            session_id: None,
                            topic: None,
                            nodes: vec![sample_node("release")],
                            edges: Vec::new(),
                            lessons: Vec::new(),
                            checkpoints: Vec::new(),
                            traits: Vec::new(),
                        })
                        .expect("request serialization should work")
                        .into(),
                    )
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn includes_external_agent_tool_descriptions() {
        let descriptions = tool_descriptions();

        assert_eq!(descriptions.len(), 6);
        assert!(descriptions
            .iter()
            .any(|description| description.path == "/generate-imagined-scenarios"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recall_context_uses_turso_vector_search_when_configured() {
        let embedder = DeterministicQueryEmbedder::default();
        let tempdir = tempdir().expect("temporary directory should exist");
        let runtime = StoreRuntime::open(StoreConfig {
            local_database_path: tempdir.path().join("agent-api-vectors.db"),
            ..StoreConfig::default()
        })
        .await
        .expect("store should open");
        let semantic_node = sample_node("migration rollout notes");
        runtime
            .repository()
            .insert_node(&semantic_node)
            .await
            .expect("node should persist");
        runtime
            .repository()
            .upsert_node_embedding(&NodeEmbeddingRecord {
                node_id: semantic_node.id,
                embedding_model: embedder.embedding_model().to_owned(),
                embedding: embedder.embed_text(
                    "migration rollout notes architecture docs should include rollout steps and rollback guidance",
                ),
            })
            .await
            .expect("embedding should persist");

        let service = AgentApiService::new_with_connection(
            runtime
                .database
                .connect()
                .expect("service vector connection should open"),
        );
        let response = service
            .recall_context(&RecallContextRequest {
                text: "deployment checklist".to_owned(),
                session_id: Some("svc-session".to_owned()),
                topic: Some("release".to_owned()),
                nodes: vec![semantic_node.clone()],
                edges: Vec::new(),
                lessons: Vec::new(),
                checkpoints: Vec::new(),
                traits: Vec::new(),
            })
            .expect("recall should succeed");

        assert_eq!(
            response.core_nodes.first().map(|node| node.id),
            Some(semantic_node.id)
        );
    }

    fn sample_node(title: &str) -> Node {
        let now = chrono::Utc::now();

        Node {
            id: NodeId(Uuid::new_v4()),
            node_type: NodeType::Semantic,
            status: MemoryStatus::Active,
            title: title.to_owned(),
            summary: format!("{title} summary"),
            content: Some(format!("{title} content")),
            tags: vec!["test".to_owned()],
            confidence: 0.8,
            importance: 0.8,
            created_at: now,
            updated_at: now,
            last_accessed_at: None,
            source_event_id: Some("event-1".to_owned()),
        }
    }
}
