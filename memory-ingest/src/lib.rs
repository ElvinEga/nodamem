//! Deterministic ingestion pipeline for turning events into candidate memory objects.

use chrono::Utc;
use memory_core::{
    CoreMarker, Edge, EdgeId, EdgeType, Lesson, LessonId, LessonType, MemoryStatus, Node, NodeId,
    NodeType,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// Supported event inputs for the ingestion pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IngestEvent {
    UserMessage(MessageEvent),
    AssistantMessage(MessageEvent),
    ToolResult(ToolResultEvent),
    SystemEvent(SystemEvent),
}

/// Message payload shared by user and assistant events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEvent {
    pub event_id: String,
    pub session_id: Option<String>,
    pub message_id: Option<String>,
    pub text: String,
}

/// Tool execution result input for ingestion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultEvent {
    pub event_id: String,
    pub tool_name: String,
    pub invocation_id: Option<String>,
    pub content_text: String,
    pub metadata: JsonValue,
}

/// Generic system event input for ingestion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemEvent {
    pub event_id: String,
    pub event_kind: String,
    pub description: String,
    pub metadata: JsonValue,
}

/// Extracted entity mention from an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub label: String,
    pub entity_type: EntityType,
    pub source_text: String,
}

/// Lightweight entity categories used by deterministic extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Tool,
    Project,
    Topic,
    Unknown,
}

/// Candidate objects produced by ingestion before downstream validation or admission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestOutput {
    pub candidate_nodes: Vec<Node>,
    pub candidate_edges: Vec<Edge>,
    pub candidate_lessons: Vec<Lesson>,
    pub salience_score: f32,
    pub extracted_entities: Vec<ExtractedEntity>,
}

/// Interface for future event-to-memory extraction strategies.
pub trait EventExtractor {
    fn extract(&self, event: &IngestEvent) -> IngestOutput;
}

/// Interface for extracting entity candidates from event text.
pub trait EntityExtractor {
    fn extract_entities(&self, event: &IngestEvent) -> Vec<ExtractedEntity>;
}

/// Interface for extracting candidate lessons from event text and node context.
pub trait LessonExtractor {
    fn extract_lessons(&self, event: &IngestEvent, nodes: &[Node]) -> Vec<Lesson>;
}

/// Production-facing deterministic pipeline with pluggable extractor interfaces.
#[derive(Debug, Default)]
pub struct IngestPipeline {
    entity_extractor: DeterministicEntityExtractor,
    lesson_extractor: DeterministicLessonExtractor,
}

impl IngestPipeline {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest(&self, event: &IngestEvent) -> IngestOutput {
        self.extract(event)
    }
}

impl EventExtractor for IngestPipeline {
    fn extract(&self, event: &IngestEvent) -> IngestOutput {
        let candidate_nodes = build_candidate_nodes(event);
        let extracted_entities = self.entity_extractor.extract_entities(event);
        let mut entity_nodes = build_entity_nodes(event, &extracted_entities);
        let mut candidate_edges = build_entity_edges(&candidate_nodes, &entity_nodes);
        let candidate_lessons = self
            .lesson_extractor
            .extract_lessons(event, &candidate_nodes);

        let salience_score = compute_salience(event, &candidate_nodes, &candidate_lessons);

        let mut all_nodes = candidate_nodes;
        all_nodes.append(&mut entity_nodes);

        candidate_edges.extend(build_lesson_edges(&all_nodes, &candidate_lessons));

        IngestOutput {
            candidate_nodes: all_nodes,
            candidate_edges,
            candidate_lessons,
            salience_score,
            extracted_entities,
        }
    }
}

/// Deterministic entity extractor used until a learned or LLM-backed extractor is plugged in.
#[derive(Debug, Default)]
pub struct DeterministicEntityExtractor;

impl EntityExtractor for DeterministicEntityExtractor {
    fn extract_entities(&self, event: &IngestEvent) -> Vec<ExtractedEntity> {
        let mut entities = Vec::new();

        match event {
            IngestEvent::ToolResult(tool_result) => {
                entities.push(ExtractedEntity {
                    label: tool_result.tool_name.clone(),
                    entity_type: EntityType::Tool,
                    source_text: tool_result.tool_name.clone(),
                });
                entities.extend(extract_entities_from_text(&tool_result.content_text));
            }
            IngestEvent::SystemEvent(system_event) => {
                entities.extend(extract_entities_from_text(&system_event.description));
            }
            IngestEvent::UserMessage(message) | IngestEvent::AssistantMessage(message) => {
                entities.extend(extract_entities_from_text(&message.text));
            }
        }

        dedupe_entities(entities)
    }
}

/// Deterministic lesson extractor used until a richer inference stage is introduced.
#[derive(Debug, Default)]
pub struct DeterministicLessonExtractor;

impl LessonExtractor for DeterministicLessonExtractor {
    fn extract_lessons(&self, event: &IngestEvent, nodes: &[Node]) -> Vec<Lesson> {
        let text = event_text(event);
        let lower = text.to_ascii_lowercase();

        if !lower.contains("should")
            && !lower.contains("learned")
            && !lower.contains("remember")
            && !lower.contains("best practice")
        {
            return Vec::new();
        }

        let now = Utc::now();
        let statement = sentence_summary(text);
        let lesson_type = if lower.contains("should") || lower.contains("best practice") {
            LessonType::Strategic
        } else if lower.contains("remember") {
            LessonType::Preference
        } else {
            LessonType::Behavioral
        };

        vec![Lesson {
            id: LessonId(Uuid::new_v4()),
            lesson_type,
            status: MemoryStatus::Candidate,
            title: truncate_title(&statement),
            statement,
            confidence: 0.45,
            reinforcement_count: 0,
            supporting_node_ids: nodes.iter().map(|node| node.id).collect(),
            contradicting_node_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        }]
    }
}

fn build_candidate_nodes(event: &IngestEvent) -> Vec<Node> {
    let now = Utc::now();
    let (node_type, title, summary, content, source_event_id, tags) = match event {
        IngestEvent::UserMessage(message) => (
            NodeType::Episodic,
            "user_message".to_owned(),
            sentence_summary(&message.text),
            Some(message.text.clone()),
            Some(message.event_id.clone()),
            vec!["user".to_owned(), "conversation".to_owned()],
        ),
        IngestEvent::AssistantMessage(message) => (
            NodeType::Episodic,
            "assistant_message".to_owned(),
            sentence_summary(&message.text),
            Some(message.text.clone()),
            Some(message.event_id.clone()),
            vec!["assistant".to_owned(), "conversation".to_owned()],
        ),
        IngestEvent::ToolResult(tool_result) => (
            NodeType::Semantic,
            format!("tool_result:{}", tool_result.tool_name),
            sentence_summary(&tool_result.content_text),
            Some(tool_result.content_text.clone()),
            Some(tool_result.event_id.clone()),
            vec!["tool".to_owned(), tool_result.tool_name.clone()],
        ),
        IngestEvent::SystemEvent(system_event) => (
            NodeType::Semantic,
            format!("system_event:{}", system_event.event_kind),
            sentence_summary(&system_event.description),
            Some(system_event.description.clone()),
            Some(system_event.event_id.clone()),
            vec!["system".to_owned(), system_event.event_kind.clone()],
        ),
    };

    vec![Node {
        id: NodeId(Uuid::new_v4()),
        node_type,
        status: MemoryStatus::Candidate,
        title,
        summary,
        content,
        tags,
        confidence: 0.55,
        importance: base_importance(event),
        created_at: now,
        updated_at: now,
        last_accessed_at: None,
        source_event_id,
    }]
}

fn build_entity_nodes(event: &IngestEvent, entities: &[ExtractedEntity]) -> Vec<Node> {
    let now = Utc::now();
    let source_event_id = Some(event_id(event).to_owned());

    entities
        .iter()
        .map(|entity| Node {
            id: NodeId(Uuid::new_v4()),
            node_type: NodeType::Entity,
            status: MemoryStatus::Candidate,
            title: entity.label.clone(),
            summary: format!("Extracted {:?} entity from event.", entity.entity_type),
            content: Some(entity.source_text.clone()),
            tags: vec![
                "entity".to_owned(),
                format!("{:?}", entity.entity_type).to_ascii_lowercase(),
            ],
            confidence: 0.4,
            importance: 0.35,
            created_at: now,
            updated_at: now,
            last_accessed_at: None,
            source_event_id: source_event_id.clone(),
        })
        .collect()
}

fn build_entity_edges(core_nodes: &[Node], entity_nodes: &[Node]) -> Vec<Edge> {
    let now = Utc::now();

    core_nodes
        .iter()
        .flat_map(|core_node| {
            entity_nodes.iter().map(move |entity_node| Edge {
                id: EdgeId(Uuid::new_v4()),
                edge_type: EdgeType::RelatedTo,
                from_node_id: core_node.id,
                to_node_id: entity_node.id,
                weight: 0.35,
                created_at: now,
                updated_at: now,
            })
        })
        .collect()
}

fn build_lesson_edges(nodes: &[Node], lessons: &[Lesson]) -> Vec<Edge> {
    let now = Utc::now();
    let lesson_node_ids: Vec<NodeId> = lessons.iter().map(|lesson| NodeId(lesson.id.0)).collect();

    nodes
        .iter()
        .flat_map(|node| {
            lesson_node_ids.iter().map(move |lesson_node_id| Edge {
                id: EdgeId(Uuid::new_v4()),
                edge_type: EdgeType::DerivedFrom,
                from_node_id: *lesson_node_id,
                to_node_id: node.id,
                weight: 0.5,
                created_at: now,
                updated_at: now,
            })
        })
        .collect()
}

fn compute_salience(event: &IngestEvent, nodes: &[Node], lessons: &[Lesson]) -> f32 {
    let text_len = event_text(event).split_whitespace().count() as f32;
    let event_weight = match event {
        IngestEvent::UserMessage(_) => 0.55,
        IngestEvent::AssistantMessage(_) => 0.45,
        IngestEvent::ToolResult(_) => 0.65,
        IngestEvent::SystemEvent(_) => 0.5,
    };

    let score = event_weight
        + (text_len / 200.0)
        + (nodes.len() as f32 * 0.05)
        + (lessons.len() as f32 * 0.1);
    score.clamp(0.0, 1.0)
}

fn base_importance(event: &IngestEvent) -> f32 {
    match event {
        IngestEvent::UserMessage(message) => importance_from_text(&message.text, 0.55),
        IngestEvent::AssistantMessage(message) => importance_from_text(&message.text, 0.45),
        IngestEvent::ToolResult(tool_result) => {
            importance_from_text(&tool_result.content_text, 0.65)
        }
        IngestEvent::SystemEvent(system_event) => {
            importance_from_text(&system_event.description, 0.5)
        }
    }
}

fn importance_from_text(text: &str, base: f32) -> f32 {
    let lower = text.to_ascii_lowercase();
    let mut score = base;

    if lower.contains("error") || lower.contains("failed") || lower.contains("important") {
        score += 0.15;
    }
    if lower.contains("remember") || lower.contains("should") {
        score += 0.1;
    }

    score.clamp(0.0, 1.0)
}

fn extract_entities_from_text(text: &str) -> Vec<ExtractedEntity> {
    text.split_whitespace()
        .filter_map(|token| {
            let cleaned = token.trim_matches(|character: char| {
                !character.is_alphanumeric() && character != '-' && character != '_'
            });
            if cleaned.len() < 2 {
                return None;
            }

            let starts_uppercase = cleaned.chars().next().is_some_and(char::is_uppercase);

            if cleaned.contains('-') || starts_uppercase {
                Some(ExtractedEntity {
                    label: cleaned.to_owned(),
                    entity_type: infer_entity_type(cleaned),
                    source_text: cleaned.to_owned(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn infer_entity_type(token: &str) -> EntityType {
    let lower = token.to_ascii_lowercase();

    if matches!(
        lower.as_str(),
        "cargo" | "rust" | "sqlite" | "turso" | "libsql"
    ) {
        EntityType::Tool
    } else if lower.contains("project") || lower.contains("nodamem") {
        EntityType::Project
    } else if token.chars().next().is_some_and(char::is_uppercase) {
        EntityType::Person
    } else {
        EntityType::Topic
    }
}

fn dedupe_entities(entities: Vec<ExtractedEntity>) -> Vec<ExtractedEntity> {
    let mut deduped = Vec::new();

    for entity in entities {
        if !deduped
            .iter()
            .any(|existing: &ExtractedEntity| existing.label.eq_ignore_ascii_case(&entity.label))
        {
            deduped.push(entity);
        }
    }

    deduped
}

fn event_id(event: &IngestEvent) -> &str {
    match event {
        IngestEvent::UserMessage(message) | IngestEvent::AssistantMessage(message) => {
            &message.event_id
        }
        IngestEvent::ToolResult(tool_result) => &tool_result.event_id,
        IngestEvent::SystemEvent(system_event) => &system_event.event_id,
    }
}

fn event_text(event: &IngestEvent) -> &str {
    match event {
        IngestEvent::UserMessage(message) | IngestEvent::AssistantMessage(message) => &message.text,
        IngestEvent::ToolResult(tool_result) => &tool_result.content_text,
        IngestEvent::SystemEvent(system_event) => &system_event.description,
    }
}

fn sentence_summary(text: &str) -> String {
    let trimmed = text.trim();
    let summary = trimmed
        .split_terminator(['.', '!', '?'])
        .next()
        .unwrap_or(trimmed)
        .trim();

    if summary.is_empty() {
        "empty event".to_owned()
    } else {
        truncate_title(summary)
    }
}

fn truncate_title(text: &str) -> String {
    const MAX_LEN: usize = 80;
    if text.chars().count() <= MAX_LEN {
        text.to_owned()
    } else {
        let truncated: String = text.chars().take(MAX_LEN - 3).collect();
        format!("{truncated}...")
    }
}

/// Marker preserved for lightweight crate composition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IngestMarker {
    pub core: CoreMarker,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{IngestEvent, IngestPipeline, MessageEvent, SystemEvent, ToolResultEvent};
    use memory_core::{LessonType, NodeType};

    #[test]
    fn ingests_user_message_into_candidate_memory() {
        let pipeline = IngestPipeline::new();
        let output = pipeline.ingest(&IngestEvent::UserMessage(MessageEvent {
            event_id: "evt-user-1".to_owned(),
            session_id: Some("session-1".to_owned()),
            message_id: Some("msg-1".to_owned()),
            text: "Remember that Alice prefers Turso for Nodamem persistence.".to_owned(),
        }));

        assert_eq!(output.candidate_nodes[0].node_type, NodeType::Episodic);
        assert!(!output.extracted_entities.is_empty());
        assert_eq!(output.candidate_lessons.len(), 1);
        assert_eq!(
            output.candidate_lessons[0].lesson_type,
            LessonType::Preference
        );
        assert!(output.salience_score > 0.5);
    }

    #[test]
    fn ingests_tool_result_with_tool_entity_and_semantic_node() {
        let pipeline = IngestPipeline::new();
        let output = pipeline.ingest(&IngestEvent::ToolResult(ToolResultEvent {
            event_id: "evt-tool-1".to_owned(),
            tool_name: "cargo".to_owned(),
            invocation_id: Some("invoke-1".to_owned()),
            content_text: "cargo test failed in memory-store with SQLite migration error"
                .to_owned(),
            metadata: json!({"exit_code": 101}),
        }));

        assert_eq!(output.candidate_nodes[0].node_type, NodeType::Semantic);
        assert!(output
            .extracted_entities
            .iter()
            .any(|entity| entity.label.eq_ignore_ascii_case("cargo")));
        assert!(!output.candidate_edges.is_empty());
    }

    #[test]
    fn ingests_system_event_without_lessons_when_no_guidance_language_exists() {
        let pipeline = IngestPipeline::new();
        let output = pipeline.ingest(&IngestEvent::SystemEvent(SystemEvent {
            event_id: "evt-system-1".to_owned(),
            event_kind: "checkpoint_created".to_owned(),
            description: "Checkpoint created for recent retrieval work.".to_owned(),
            metadata: json!({"checkpoint_id": "cp-1"}),
        }));

        assert_eq!(output.candidate_nodes[0].node_type, NodeType::Semantic);
        assert!(output.candidate_lessons.is_empty());
        assert!(output.salience_score > 0.0);
    }

    #[test]
    fn ingests_assistant_message_with_strategic_lesson_stub() {
        let pipeline = IngestPipeline::new();
        let output = pipeline.ingest(&IngestEvent::AssistantMessage(MessageEvent {
            event_id: "evt-assistant-1".to_owned(),
            session_id: Some("session-2".to_owned()),
            message_id: Some("msg-2".to_owned()),
            text: "We should store edges separately so graph traversals stay efficient.".to_owned(),
        }));

        assert_eq!(output.candidate_lessons.len(), 1);
        assert_eq!(
            output.candidate_lessons[0].lesson_type,
            LessonType::Strategic
        );
        assert!(output.candidate_edges.iter().any(|edge| edge.weight > 0.0));
    }
}
