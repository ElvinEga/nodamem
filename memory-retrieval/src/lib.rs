//! Agent-friendly memory retrieval with pluggable ranking and vector lookup interfaces.

use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;
use std::fmt;

use chrono::{Duration, Utc};
use memory_core::{
    Checkpoint, CoreMarker, Edge, Lesson, MemoryPacket, MemoryPacketId, Node, NodeId, TraitState,
};
use memory_store::StoreMarker;
use uuid::Uuid;

/// Query input for building a memory packet for an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryQuery {
    pub text: String,
    pub session_id: Option<String>,
    pub topic: Option<String>,
}

/// Ranking breakdown used during retrieval scoring.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalScoreBreakdown {
    pub semantic_similarity: f32,
    pub edge_strength: f32,
    pub importance: f32,
    pub recency: f32,
    pub confidence: f32,
    pub centrality: f32,
    pub total: f32,
}

/// Retrieval policy controlling packet sizes and score cutoffs.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalPolicy {
    pub core_node_limit: usize,
    pub neighbor_limit: usize,
    pub lesson_limit: usize,
    pub min_score: f32,
}

impl Default for RetrievalPolicy {
    fn default() -> Self {
        Self {
            core_node_limit: 3,
            neighbor_limit: 2,
            lesson_limit: 2,
            min_score: 0.2,
        }
    }
}

/// Internal ranked node candidate used while building packets.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedNode {
    pub node: Node,
    pub score: RetrievalScoreBreakdown,
}

/// Agent-facing retrieval response with explicit slices for core context and neighborhood context.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievedMemoryPacket {
    pub core_nodes: Vec<Node>,
    pub related_neighbors: Vec<Node>,
    pub lessons: Vec<Lesson>,
    pub checkpoint_summary: Option<Checkpoint>,
    pub trait_snapshot: Option<TraitState>,
    pub packet: MemoryPacket,
}

/// Retrieval errors.
#[derive(Debug)]
pub enum RetrievalError {
    Source(String),
}

impl fmt::Display for RetrievalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(message) => write!(formatter, "retrieval source error: {message}"),
        }
    }
}

impl StdError for RetrievalError {}

/// Storage-agnostic retrieval source.
pub trait RetrievalSource {
    fn all_nodes(&self) -> Result<Vec<Node>, RetrievalError>;
    fn all_edges(&self) -> Result<Vec<Edge>, RetrievalError>;
    fn all_lessons(&self) -> Result<Vec<Lesson>, RetrievalError>;
    fn recent_checkpoints(&self, limit: usize) -> Result<Vec<Checkpoint>, RetrievalError>;
    fn current_traits(&self, limit: usize) -> Result<Vec<TraitState>, RetrievalError>;
}

/// Interface for future vector-backed retrieval.
pub trait VectorRetriever {
    fn retrieve(&self, query: &MemoryQuery, nodes: &[Node]) -> Vec<(NodeId, f32)>;
}

/// Temporary lexical fallback until vector search is wired in.
#[derive(Debug, Default, Clone, Copy)]
pub struct LexicalFallbackRetriever;

impl VectorRetriever for LexicalFallbackRetriever {
    fn retrieve(&self, query: &MemoryQuery, nodes: &[Node]) -> Vec<(NodeId, f32)> {
        let query_terms = split_terms(&query.text);

        nodes
            .iter()
            .filter_map(|node| {
                let node_terms = node_terms(node);
                if query_terms.is_empty() || node_terms.is_empty() {
                    return None;
                }

                let overlap = query_terms
                    .iter()
                    .filter(|term| node_terms.contains(*term))
                    .count() as f32;
                let denominator = query_terms.len().max(node_terms.len()) as f32;
                let score = (overlap / denominator).clamp(0.0, 1.0);

                (score > 0.0).then_some((node.id, score))
            })
            .collect()
    }
}

/// Retrieval engine that ranks nodes, expands the graph neighborhood, and builds a memory packet.
#[derive(Debug, Clone)]
pub struct RetrievalEngine<S, V> {
    source: S,
    vector_retriever: V,
    policy: RetrievalPolicy,
}

impl<S, V> RetrievalEngine<S, V> {
    #[must_use]
    pub fn new(source: S, vector_retriever: V, policy: RetrievalPolicy) -> Self {
        Self {
            source,
            vector_retriever,
            policy,
        }
    }
}

impl<S> RetrievalEngine<S, LexicalFallbackRetriever> {
    #[must_use]
    pub fn with_fallback(source: S) -> Self {
        Self::new(source, LexicalFallbackRetriever, RetrievalPolicy::default())
    }
}

impl<S, V> RetrievalEngine<S, V>
where
    S: RetrievalSource,
    V: VectorRetriever,
{
    pub fn recall_context(
        &self,
        query: &MemoryQuery,
    ) -> Result<RetrievedMemoryPacket, RetrievalError> {
        let nodes = self.source.all_nodes()?;
        let edges = self.source.all_edges()?;
        let lessons = self.source.all_lessons()?;
        let checkpoints = self.source.recent_checkpoints(1)?;
        let traits = self.source.current_traits(1)?;

        let ranked_nodes = self.rank_nodes(query, &nodes, &edges);
        let mut core_nodes = ranked_nodes
            .into_iter()
            .filter(|ranked| ranked.score.total >= self.policy.min_score)
            .take(self.policy.core_node_limit.clamp(3, 5))
            .map(|ranked| ranked.node)
            .collect::<Vec<_>>();

        if core_nodes.len() < 3 {
            let fallback_ranked = self.rank_nodes(query, &nodes, &edges);
            for ranked in fallback_ranked {
                if core_nodes.iter().any(|node| node.id == ranked.node.id) {
                    continue;
                }
                core_nodes.push(ranked.node);
                if core_nodes.len() == 3 {
                    break;
                }
            }
        }

        let neighbors = expand_neighbors(
            &core_nodes,
            &nodes,
            &edges,
            self.policy.neighbor_limit.clamp(2, 3),
        );
        let selected_node_ids = collect_node_ids(&core_nodes, &neighbors);
        let packet_edges = select_edges(&edges, &selected_node_ids);
        let packet_lessons = select_lessons(
            &lessons,
            &selected_node_ids,
            self.policy.lesson_limit.clamp(1, 2),
        );
        let checkpoint_summary = checkpoints.into_iter().next();
        let trait_snapshot = traits.into_iter().next();

        let packet_core_nodes = core_nodes;
        let packet_neighbors = neighbors;
        let mut packet_nodes = packet_core_nodes.clone();
        packet_nodes.extend(packet_neighbors.clone());

        let packet = MemoryPacket {
            id: MemoryPacketId(Uuid::new_v4()),
            request_id: query.session_id.clone(),
            created_at: Utc::now(),
            nodes: packet_nodes.clone(),
            edges: packet_edges,
            lessons: packet_lessons.clone(),
            traits: trait_snapshot.iter().cloned().collect(),
            checkpoints: checkpoint_summary.iter().cloned().collect(),
            imagined_scenarios: Vec::new(),
        };

        Ok(RetrievedMemoryPacket {
            core_nodes: packet_core_nodes,
            related_neighbors: packet_neighbors,
            lessons: packet_lessons,
            checkpoint_summary,
            trait_snapshot,
            packet,
        })
    }

    fn rank_nodes(&self, query: &MemoryQuery, nodes: &[Node], edges: &[Edge]) -> Vec<RankedNode> {
        let semantic_scores = self
            .vector_retriever
            .retrieve(query, nodes)
            .into_iter()
            .collect::<HashMap<_, _>>();

        let mut ranked = nodes
            .iter()
            .cloned()
            .map(|node| RankedNode {
                score: build_score_breakdown(
                    &node,
                    semantic_scores.get(&node.id).copied().unwrap_or_default(),
                    edges,
                ),
                node,
            })
            .collect::<Vec<_>>();

        ranked.sort_by(|left, right| right.score.total.total_cmp(&left.score.total));
        ranked
    }
}

fn build_score_breakdown(
    node: &Node,
    semantic_similarity: f32,
    edges: &[Edge],
) -> RetrievalScoreBreakdown {
    let edge_strength = edge_strength_score(node.id, edges);
    let importance = node.importance.clamp(0.0, 1.0);
    let recency = recency_score(node);
    let confidence = node.confidence.clamp(0.0, 1.0);
    let centrality = centrality_score(node.id, edges);
    let total = (semantic_similarity * 0.3
        + edge_strength * 0.15
        + importance * 0.2
        + recency * 0.1
        + confidence * 0.15
        + centrality * 0.1)
        .clamp(0.0, 1.0);

    RetrievalScoreBreakdown {
        semantic_similarity,
        edge_strength,
        importance,
        recency,
        confidence,
        centrality,
        total,
    }
}

fn edge_strength_score(node_id: NodeId, edges: &[Edge]) -> f32 {
    let total = edges
        .iter()
        .filter(|edge| edge.from_node_id == node_id || edge.to_node_id == node_id)
        .map(|edge| edge.weight)
        .sum::<f32>();

    (total / 3.0).clamp(0.0, 1.0)
}

fn recency_score(node: &Node) -> f32 {
    let age = Utc::now() - node.updated_at;
    if age <= Duration::days(1) {
        1.0
    } else if age <= Duration::days(7) {
        0.8
    } else if age <= Duration::days(30) {
        0.5
    } else {
        0.2
    }
}

fn centrality_score(node_id: NodeId, edges: &[Edge]) -> f32 {
    let degree = edges
        .iter()
        .filter(|edge| edge.from_node_id == node_id || edge.to_node_id == node_id)
        .count() as f32;

    (degree / 5.0).clamp(0.0, 1.0)
}

fn expand_neighbors(
    core_nodes: &[Node],
    all_nodes: &[Node],
    edges: &[Edge],
    limit: usize,
) -> Vec<Node> {
    let core_ids = core_nodes
        .iter()
        .map(|node| node.id)
        .collect::<HashSet<_>>();
    let mut neighbor_scores = HashMap::<NodeId, f32>::new();

    for edge in edges {
        if core_ids.contains(&edge.from_node_id) && !core_ids.contains(&edge.to_node_id) {
            *neighbor_scores.entry(edge.to_node_id).or_default() += edge.weight;
        }
        if core_ids.contains(&edge.to_node_id) && !core_ids.contains(&edge.from_node_id) {
            *neighbor_scores.entry(edge.from_node_id).or_default() += edge.weight;
        }
    }

    let mut neighbors = all_nodes
        .iter()
        .filter_map(|node| {
            neighbor_scores
                .get(&node.id)
                .copied()
                .map(|score| (score, node.clone()))
        })
        .collect::<Vec<_>>();

    neighbors.sort_by(|left, right| right.0.total_cmp(&left.0));
    neighbors
        .into_iter()
        .take(limit)
        .map(|(_, node)| node)
        .collect()
}

fn select_edges(edges: &[Edge], selected_node_ids: &HashSet<NodeId>) -> Vec<Edge> {
    edges
        .iter()
        .filter(|edge| {
            selected_node_ids.contains(&edge.from_node_id)
                && selected_node_ids.contains(&edge.to_node_id)
        })
        .cloned()
        .collect()
}

fn select_lessons(
    lessons: &[Lesson],
    selected_node_ids: &HashSet<NodeId>,
    limit: usize,
) -> Vec<Lesson> {
    let mut ranked_lessons = lessons
        .iter()
        .filter_map(|lesson| {
            let supporting_overlap = lesson
                .supporting_node_ids
                .iter()
                .filter(|node_id| selected_node_ids.contains(node_id))
                .count() as f32;
            let score = supporting_overlap * 0.5 + lesson.confidence * 0.5;

            (score > 0.0).then_some((score, lesson.clone()))
        })
        .collect::<Vec<_>>();

    ranked_lessons.sort_by(|left, right| right.0.total_cmp(&left.0));
    ranked_lessons
        .into_iter()
        .take(limit)
        .map(|(_, lesson)| lesson)
        .collect()
}

fn collect_node_ids(core_nodes: &[Node], neighbors: &[Node]) -> HashSet<NodeId> {
    core_nodes
        .iter()
        .chain(neighbors.iter())
        .map(|node| node.id)
        .collect()
}

fn node_terms(node: &Node) -> Vec<String> {
    let mut terms = split_terms(&node.title);
    terms.extend(split_terms(&node.summary));
    if let Some(content) = &node.content {
        terms.extend(split_terms(content));
    }
    terms.sort();
    terms.dedup();
    terms
}

fn split_terms(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|term| {
            term.trim_matches(|character: char| !character.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|term| term.len() > 2)
        .collect()
}

/// Marker preserved for crate composition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetrievalMarker {
    pub core: CoreMarker,
    pub store: StoreMarker,
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use memory_core::{
        Checkpoint, CheckpointId, Edge, EdgeId, EdgeType, Lesson, LessonId, LessonType,
        MemoryStatus, Node, NodeId, NodeType, TraitId, TraitState, TraitType,
    };
    use uuid::Uuid;

    use super::{
        LexicalFallbackRetriever, MemoryQuery, RetrievalEngine, RetrievalError, RetrievalPolicy,
        RetrievalSource,
    };

    #[derive(Debug, Clone)]
    struct TestSource {
        nodes: Vec<Node>,
        edges: Vec<Edge>,
        lessons: Vec<Lesson>,
        checkpoints: Vec<Checkpoint>,
        traits: Vec<TraitState>,
    }

    impl RetrievalSource for TestSource {
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

    #[test]
    fn builds_agent_friendly_memory_packet() {
        let source = sample_source();
        let engine =
            RetrievalEngine::new(source, LexicalFallbackRetriever, RetrievalPolicy::default());
        let packet = engine
            .recall_context(&MemoryQuery {
                text: "cargo migration failure for Nodamem".to_owned(),
                session_id: Some("session-1".to_owned()),
                topic: Some("persistence".to_owned()),
            })
            .expect("retrieval should succeed");

        assert!((3..=5).contains(&packet.core_nodes.len()));
        assert!((2..=3).contains(&packet.related_neighbors.len()));
        assert!((1..=2).contains(&packet.lessons.len()));
        assert!(packet.checkpoint_summary.is_some());
        assert!(packet.trait_snapshot.is_some());
        assert!(packet
            .packet
            .nodes
            .iter()
            .any(|node| node.title.contains("cargo")));
    }

    #[test]
    fn expands_graph_neighbors_for_core_nodes() {
        let engine = RetrievalEngine::with_fallback(sample_source());
        let packet = engine
            .recall_context(&MemoryQuery {
                text: "Nodamem database".to_owned(),
                session_id: None,
                topic: None,
            })
            .expect("retrieval should succeed");

        let neighbor_like = packet
            .related_neighbors
            .iter()
            .filter(|node| node.node_type == NodeType::Entity)
            .count();

        assert!(neighbor_like >= 1);
        assert!(packet.packet.edges.len() >= 1);
    }

    fn sample_source() -> TestSource {
        let now = Utc::now();
        let node_a = node(
            "cargo migration error",
            "cargo failed on migration setup",
            0.9,
            0.95,
            now - Duration::hours(2),
            vec!["tool".to_owned(), "cargo".to_owned()],
        );
        let node_b = node(
            "Nodamem database",
            "embedded Turso database for Nodamem",
            0.8,
            0.9,
            now - Duration::days(1),
            vec!["project".to_owned(), "database".to_owned()],
        );
        let node_c = node(
            "memory graph edges",
            "graph traversal and related nodes",
            0.7,
            0.8,
            now - Duration::days(3),
            vec!["graph".to_owned(), "retrieval".to_owned()],
        );
        let node_d = node(
            "Turso",
            "Turso embedded mode",
            0.65,
            0.7,
            now - Duration::days(4),
            vec!["database".to_owned(), "tool".to_owned()],
        );
        let node_e = node(
            "Nodamem",
            "main project entity",
            0.6,
            0.75,
            now - Duration::days(2),
            vec!["project".to_owned()],
        );

        TestSource {
            edges: vec![
                edge(node_a.id, node_b.id, 0.9),
                edge(node_b.id, node_d.id, 0.7),
                edge(node_b.id, node_e.id, 0.8),
                edge(node_c.id, node_e.id, 0.5),
            ],
            lessons: vec![
                Lesson {
                    id: LessonId(Uuid::new_v4()),
                    lesson_type: LessonType::Strategy,
                    status: MemoryStatus::Active,
                    title: "Handle migrations early".to_owned(),
                    statement: "Database migrations should run at startup.".to_owned(),
                    confidence: 0.8,
                    evidence_count: 2,
                    reinforcement_count: 3,
                    supporting_node_ids: vec![node_a.id, node_b.id],
                    contradicting_node_ids: Vec::new(),
                    created_at: now,
                    updated_at: now,
                },
                Lesson {
                    id: LessonId(Uuid::new_v4()),
                    lesson_type: LessonType::Task,
                    status: MemoryStatus::Active,
                    title: "Expand graph neighbors".to_owned(),
                    statement: "Graph retrieval should include related neighboring nodes."
                        .to_owned(),
                    confidence: 0.75,
                    evidence_count: 2,
                    reinforcement_count: 2,
                    supporting_node_ids: vec![node_c.id, node_e.id],
                    contradicting_node_ids: Vec::new(),
                    created_at: now,
                    updated_at: now,
                },
            ],
            checkpoints: vec![Checkpoint {
                id: CheckpointId(Uuid::new_v4()),
                status: MemoryStatus::Active,
                title: "Recent persistence work".to_owned(),
                summary: "Focused on embedded database and migration flow.".to_owned(),
                node_ids: vec![node_a.id, node_b.id],
                lesson_ids: Vec::new(),
                trait_ids: Vec::new(),
                created_at: now,
                updated_at: now,
            }],
            traits: vec![TraitState {
                id: TraitId(Uuid::new_v4()),
                trait_type: TraitType::EvidenceReliance,
                status: MemoryStatus::Active,
                label: "Evidence Reliance".to_owned(),
                description: "Favors validated signals before deciding.".to_owned(),
                strength: 0.8,
                confidence: 0.7,
                supporting_lesson_ids: Vec::new(),
                supporting_node_ids: vec![node_b.id],
                created_at: now,
                updated_at: now,
            }],
            nodes: vec![node_a, node_b, node_c, node_d, node_e],
        }
    }

    fn node(
        title: &str,
        summary: &str,
        confidence: f32,
        importance: f32,
        updated_at: chrono::DateTime<Utc>,
        tags: Vec<String>,
    ) -> Node {
        Node {
            id: NodeId(Uuid::new_v4()),
            node_type: if title == "Turso" || title == "Nodamem" {
                NodeType::Entity
            } else {
                NodeType::Semantic
            },
            status: MemoryStatus::Active,
            title: title.to_owned(),
            summary: summary.to_owned(),
            content: Some(summary.to_owned()),
            tags,
            confidence,
            importance,
            created_at: updated_at,
            updated_at,
            last_accessed_at: None,
            source_event_id: Some("seed".to_owned()),
        }
    }

    fn edge(from_node_id: NodeId, to_node_id: NodeId, weight: f32) -> Edge {
        let now = Utc::now();
        Edge {
            id: EdgeId(Uuid::new_v4()),
            edge_type: EdgeType::RelatedTo,
            from_node_id,
            to_node_id,
            weight,
            created_at: now,
            updated_at: now,
        }
    }
}
