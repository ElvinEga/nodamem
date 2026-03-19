//! Hybrid memory retrieval for Nodamem.
//!
//! Developer notes:
//! - Turso/libSQL remains the source of truth for nodes, edges, and embeddings.
//! - Tantivy acts as the local lexical retrieval layer and can be maintained incrementally through
//!   [`lexical::IndexedStoreRepository`] or rebuilt from source nodes when the app cold-starts.
//! - Hybrid retrieval combines lexical BM25, vector similarity, graph expansion, and weighted
//!   reranking before assembling a compact memory packet.

use std::error::Error as StdError;
use std::fmt;

use memory_core::{Checkpoint, CoreMarker, Edge, Lesson, MemoryPacket, Node, NodeId, TraitState};
use memory_store::StoreMarker;
use tracing::debug;

pub mod graph;
pub mod lexical;
pub mod packet;
pub mod rerank;
pub mod vector;

use graph::{GraphExpander, GraphExpansionConfig};
use lexical::{LexicalCandidate, LexicalSearch, TantivyLexicalIndex};
use packet::assemble_memory_packet;
use rerank::{merge_and_rank, HybridWeights};
use vector::{NullVectorSearch, VectorCandidate, VectorSearch};

/// Query input for building a memory packet for an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryQuery {
    pub text: String,
    pub session_id: Option<String>,
    pub topic: Option<String>,
}

/// Weighted scoring breakdown after lexical, vector, graph, and node metadata are merged.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalScoreBreakdown {
    pub lexical_score: f32,
    pub vector_score: f32,
    pub edge_strength: f32,
    pub recency: f32,
    pub importance: f32,
    pub confidence: f32,
    pub centrality: f32,
    pub total: f32,
}

/// Agent-facing retrieval response with compact verified context.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievedMemoryPacket {
    pub core_nodes: Vec<Node>,
    pub related_neighbors: Vec<Node>,
    pub lessons: Vec<Lesson>,
    pub checkpoint_summary: Option<Checkpoint>,
    pub trait_snapshot: Option<TraitState>,
    pub packet: MemoryPacket,
}

/// Retrieval policy controlling search limits and weighted reranking.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalPolicy {
    pub core_node_limit: usize,
    pub neighbor_limit: usize,
    pub lesson_limit: usize,
    pub lexical_limit: usize,
    pub vector_limit: usize,
    pub min_score: f32,
    pub graph: GraphExpansionConfig,
    pub weights: HybridWeights,
}

impl Default for RetrievalPolicy {
    fn default() -> Self {
        Self {
            core_node_limit: 3,
            neighbor_limit: 2,
            lesson_limit: 2,
            lexical_limit: 8,
            vector_limit: 8,
            min_score: 0.2,
            graph: GraphExpansionConfig {
                max_hops: 1,
                max_candidates: 12,
            },
            weights: HybridWeights::default(),
        }
    }
}

/// Retrieval errors.
#[derive(Debug)]
pub enum RetrievalError {
    Source(String),
    Lexical(String),
    Vector(String),
}

impl fmt::Display for RetrievalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(message) => write!(formatter, "retrieval source error: {message}"),
            Self::Lexical(message) => write!(formatter, "lexical retrieval error: {message}"),
            Self::Vector(message) => write!(formatter, "vector retrieval error: {message}"),
        }
    }
}

impl StdError for RetrievalError {}

impl From<tantivy::TantivyError> for RetrievalError {
    fn from(error: tantivy::TantivyError) -> Self {
        Self::Lexical(error.to_string())
    }
}

impl From<memory_store::StoreError> for RetrievalError {
    fn from(error: memory_store::StoreError) -> Self {
        Self::Source(error.to_string())
    }
}

/// Storage-agnostic retrieval source.
pub trait RetrievalSource {
    fn all_nodes(&self) -> Result<Vec<Node>, RetrievalError>;
    fn all_edges(&self) -> Result<Vec<Edge>, RetrievalError>;
    fn all_lessons(&self) -> Result<Vec<Lesson>, RetrievalError>;
    fn recent_checkpoints(&self, limit: usize) -> Result<Vec<Checkpoint>, RetrievalError>;
    fn current_traits(&self, limit: usize) -> Result<Vec<TraitState>, RetrievalError>;
}

/// Retrieval engine that combines lexical BM25, vector retrieval, graph expansion, and reranking.
#[derive(Debug, Clone)]
pub struct RetrievalEngine<S, V> {
    source: S,
    vector_search: V,
    policy: RetrievalPolicy,
}

impl<S, V> RetrievalEngine<S, V> {
    #[must_use]
    pub fn new(source: S, vector_search: V, policy: RetrievalPolicy) -> Self {
        Self {
            source,
            vector_search,
            policy,
        }
    }
}

impl<S> RetrievalEngine<S, NullVectorSearch> {
    #[must_use]
    pub fn with_hybrid_defaults(source: S) -> Self {
        Self::new(source, NullVectorSearch, RetrievalPolicy::default())
    }
}

impl<S, V> RetrievalEngine<S, V>
where
    S: RetrievalSource,
    V: VectorSearch,
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

        debug!(
            query_terms = query.text.split_whitespace().count(),
            node_count = nodes.len(),
            edge_count = edges.len(),
            lesson_count = lessons.len(),
            checkpoint_count = checkpoints.len(),
            trait_count = traits.len(),
            "retrieval source inputs loaded"
        );

        let lexical_index = TantivyLexicalIndex::from_nodes(&nodes)?;
        let lexical_hits = lexical_index.search(&query.text, self.policy.lexical_limit)?;
        let vector_hits = self
            .vector_search
            .search(query, &nodes, self.policy.vector_limit)?;

        debug!(lexical_hits = lexical_hits.len(), "lexical hits collected");
        debug!(vector_hits = vector_hits.len(), "vector hits collected");

        let seed_ids = seed_node_ids(&lexical_hits, &vector_hits);
        let neighbor_hits = GraphExpander::new(self.policy.graph).expand(&seed_ids, &nodes, &edges);

        debug!(
            merged_seed_ids = seed_ids.len(),
            neighbor_hits = neighbor_hits.len(),
            "graph expansion completed"
        );

        let ranked = merge_and_rank(
            &nodes,
            &edges,
            &lexical_hits,
            &vector_hits,
            &neighbor_hits,
            &self.policy.weights,
        );

        for candidate in ranked.iter().take(5) {
            debug!(
                node_id = %candidate.node.id.0,
                lexical_score = candidate.score.lexical_score,
                vector_score = candidate.score.vector_score,
                edge_strength = candidate.score.edge_strength,
                recency = candidate.score.recency,
                importance = candidate.score.importance,
                confidence = candidate.score.confidence,
                centrality = candidate.score.centrality,
                total = candidate.score.total,
                "hybrid reranking score"
            );
        }

        Ok(assemble_memory_packet(
            query,
            &nodes,
            &edges,
            &lessons,
            checkpoints.into_iter().next(),
            traits.into_iter().next(),
            &ranked,
            &self.policy,
        ))
    }
}

fn seed_node_ids(
    lexical_hits: &[LexicalCandidate],
    vector_hits: &[VectorCandidate],
) -> Vec<NodeId> {
    let mut seen = std::collections::HashSet::new();

    lexical_hits
        .iter()
        .map(|candidate| candidate.node_id)
        .chain(vector_hits.iter().map(|candidate| candidate.node_id))
        .filter(|node_id| seen.insert(*node_id))
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
    use std::collections::HashMap;
    use std::future::Future;

    use chrono::{Duration, Utc};
    use memory_core::{
        Checkpoint, CheckpointId, Edge, EdgeId, EdgeType, Lesson, LessonId, LessonType,
        MemoryStatus, Node, NodeId, NodeType, TraitId, TraitState, TraitType,
    };
    use memory_store::{NodeEmbeddingRecord, StoreConfig, StoreRuntime};
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::lexical::{LexicalSearch, TantivyLexicalIndex};
    use super::rerank::{merge_and_rank, HybridWeights};
    use super::vector::{
        EmbeddedQuery, QueryEmbeddingProvider, TursoVectorSearch, VectorCandidate, VectorSearch,
    };
    use super::{MemoryQuery, RetrievalEngine, RetrievalError, RetrievalPolicy, RetrievalSource};

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

    #[derive(Debug, Clone)]
    struct MockVectorSearch {
        hits: Vec<VectorCandidate>,
    }

    impl VectorSearch for MockVectorSearch {
        fn search(
            &self,
            _query: &MemoryQuery,
            _nodes: &[Node],
            limit: usize,
        ) -> Result<Vec<VectorCandidate>, RetrievalError> {
            Ok(self.hits.iter().take(limit).cloned().collect())
        }
    }

    #[derive(Debug, Clone)]
    struct TestQueryEmbedder {
        embeddings: HashMap<String, EmbeddedQuery>,
    }

    impl QueryEmbeddingProvider for TestQueryEmbedder {
        fn embed_query(&self, query: &MemoryQuery) -> Result<EmbeddedQuery, RetrievalError> {
            self.embeddings.get(&query.text).cloned().ok_or_else(|| {
                RetrievalError::Vector(format!("missing embedding for {}", query.text))
            })
        }
    }

    #[test]
    fn bm25_search_returns_exact_keyword_matches() {
        let source = sample_source();
        let index =
            TantivyLexicalIndex::from_nodes(&source.nodes).expect("index build should work");
        let hits = index
            .search("migrations", 5)
            .expect("lexical search should work");

        assert!(!hits.is_empty());
        assert!(hits[0]
            .matched_fields
            .iter()
            .any(|field| field == "title" || field == "content"));
    }

    #[test]
    fn vector_results_are_merged_with_lexical_hits() {
        let source = sample_source();
        let index =
            TantivyLexicalIndex::from_nodes(&source.nodes).expect("index build should work");
        let lexical_hits = index
            .search("architecture", 5)
            .expect("lexical search should work");
        let vector_hits = vec![VectorCandidate {
            node_id: source.nodes[1].id,
            vector_similarity_score: 0.9,
        }];

        let ranked = merge_and_rank(
            &source.nodes,
            &source.edges,
            &lexical_hits,
            &vector_hits,
            &[],
            &HybridWeights::default(),
        );

        let unique_ids = ranked
            .iter()
            .map(|candidate| candidate.node.id)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ranked.len(), unique_ids.len());
    }

    #[test]
    fn real_vector_search_finds_semantic_match_without_exact_keywords() {
        let source = sample_source();
        let runtime = build_vector_store(&source.nodes, &sample_embeddings());
        let vector_search = sample_turso_vector_search(&runtime.database);

        let hits = vector_search
            .search(
                &MemoryQuery {
                    text: "deployment playbook".to_owned(),
                    session_id: None,
                    topic: Some("release".to_owned()),
                },
                &source.nodes,
                3,
            )
            .expect("vector search should succeed");

        assert!(!hits.is_empty());
        assert_eq!(hits[0].node_id, source.nodes[0].id);
        assert!(hits[0].vector_similarity_score > 0.9);
    }

    #[test]
    fn real_vector_results_merge_with_lexical_candidates() {
        let source = sample_source();
        let runtime = build_vector_store(&source.nodes, &sample_embeddings());
        let vector_search = sample_turso_vector_search(&runtime.database);
        let index =
            TantivyLexicalIndex::from_nodes(&source.nodes).expect("index build should work");
        let lexical_hits = index
            .search("architecture", 5)
            .expect("lexical search should work");
        let vector_hits = vector_search
            .search(
                &MemoryQuery {
                    text: "system design".to_owned(),
                    session_id: None,
                    topic: Some("architecture".to_owned()),
                },
                &source.nodes,
                5,
            )
            .expect("vector search should succeed");

        let ranked = merge_and_rank(
            &source.nodes,
            &source.edges,
            &lexical_hits,
            &vector_hits,
            &[],
            &HybridWeights::default(),
        );

        let unique_ids = ranked
            .iter()
            .map(|candidate| candidate.node.id)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(ranked.len(), unique_ids.len());
        assert!(ranked
            .iter()
            .any(|candidate| candidate.node.id == source.nodes[1].id));
    }

    #[test]
    fn reranking_prefers_good_combined_result_over_weak_single_signal_result() {
        let source = sample_source();
        let lexical_hits = vec![
            super::lexical::LexicalCandidate {
                node_id: source.nodes[0].id,
                lexical_score: 10.0,
                matched_fields: vec!["title".to_owned()],
            },
            super::lexical::LexicalCandidate {
                node_id: source.nodes[1].id,
                lexical_score: 7.0,
                matched_fields: vec!["summary".to_owned()],
            },
        ];
        let vector_hits = vec![VectorCandidate {
            node_id: source.nodes[1].id,
            vector_similarity_score: 0.95,
        }];

        let ranked = merge_and_rank(
            &source.nodes,
            &source.edges,
            &lexical_hits,
            &vector_hits,
            &[],
            &HybridWeights::default(),
        );

        assert_eq!(ranked[0].node.id, source.nodes[1].id);
    }

    #[test]
    fn final_reranking_uses_real_vector_results_in_hybrid_flow() {
        let source = sample_source();
        let runtime = build_vector_store(&source.nodes, &sample_embeddings());
        let engine = RetrievalEngine::new(
            source.clone(),
            sample_turso_vector_search(&runtime.database),
            RetrievalPolicy::default(),
        );

        let packet = engine
            .recall_context(&MemoryQuery {
                text: "system design architecture".to_owned(),
                session_id: Some("session-2".to_owned()),
                topic: Some("planning".to_owned()),
            })
            .expect("hybrid retrieval should succeed");

        assert_eq!(packet.core_nodes[0].id, source.nodes[1].id);
    }

    #[test]
    fn memory_packet_assembly_remains_compact_and_stable() {
        let source = sample_source();
        let vector_target = source.nodes[1].id;
        let engine = RetrievalEngine::new(
            source,
            MockVectorSearch {
                hits: vec![VectorCandidate {
                    node_id: vector_target,
                    vector_similarity_score: 0.88,
                }],
            },
            RetrievalPolicy::default(),
        );
        let packet = engine
            .recall_context(&MemoryQuery {
                text: "architecture migrations".to_owned(),
                session_id: Some("session-1".to_owned()),
                topic: Some("planning".to_owned()),
            })
            .expect("hybrid retrieval should succeed");

        assert!((3..=5).contains(&packet.core_nodes.len()));
        assert!((0..=3).contains(&packet.related_neighbors.len()));
        assert!((1..=2).contains(&packet.lessons.len()));
    }

    fn sample_source() -> TestSource {
        let now = Utc::now();
        let node_a = node(
            "migration rollout notes",
            "architecture docs should include migrations and rollout steps",
            Some("Track migrations, rollout steps, and rollback guidance."),
            0.9,
            0.95,
            now - Duration::hours(2),
            vec!["architecture".to_owned(), "migrations".to_owned()],
        );
        let node_b = node(
            "architecture review",
            "review service boundaries and memory graph rules",
            Some("Architecture review covered migration safety and runtime boundaries."),
            0.85,
            0.9,
            now - Duration::hours(5),
            vec!["architecture".to_owned(), "review".to_owned()],
        );
        let node_c = node(
            "release planning",
            "planning notes for release cutover",
            Some("Release planning improved when rollout notes were explicit."),
            0.8,
            0.82,
            now - Duration::days(1),
            vec!["planning".to_owned()],
        );
        let node_d = node(
            "Turso embedded",
            "embedded database details",
            Some("Turso embedded mode stores memory graph state locally."),
            0.7,
            0.75,
            now - Duration::days(3),
            vec!["database".to_owned(), "turso".to_owned()],
        );
        let node_e = node(
            "OpenClaw adapter",
            "agent runtime adapter behavior",
            Some("Adapters should not bypass memory validation."),
            0.78,
            0.8,
            now - Duration::days(2),
            vec!["adapter".to_owned()],
        );

        TestSource {
            nodes: vec![
                node_a.clone(),
                node_b.clone(),
                node_c.clone(),
                node_d.clone(),
                node_e.clone(),
            ],
            edges: vec![
                edge(node_a.id, node_b.id, 0.9),
                edge(node_b.id, node_c.id, 0.8),
                edge(node_c.id, node_e.id, 0.65),
                edge(node_b.id, node_d.id, 0.5),
            ],
            lessons: vec![
                Lesson {
                    id: LessonId(Uuid::new_v4()),
                    lesson_type: LessonType::Strategy,
                    status: MemoryStatus::Active,
                    title: "Keep rollout notes explicit".to_owned(),
                    statement: "Planning improves when rollout notes and migrations are explicit."
                        .to_owned(),
                    confidence: 0.82,
                    evidence_count: 2,
                    reinforcement_count: 2,
                    supporting_node_ids: vec![node_a.id, node_c.id],
                    contradicting_node_ids: Vec::new(),
                    created_at: now,
                    updated_at: now,
                },
                Lesson {
                    id: LessonId(Uuid::new_v4()),
                    lesson_type: LessonType::Domain,
                    status: MemoryStatus::Active,
                    title: "Preserve store boundaries".to_owned(),
                    statement: "Adapters should not touch raw database tables.".to_owned(),
                    confidence: 0.77,
                    evidence_count: 1,
                    reinforcement_count: 1,
                    supporting_node_ids: vec![node_e.id],
                    contradicting_node_ids: Vec::new(),
                    created_at: now,
                    updated_at: now,
                },
            ],
            checkpoints: vec![Checkpoint {
                id: CheckpointId(Uuid::new_v4()),
                status: MemoryStatus::Active,
                title: "recent planning".to_owned(),
                summary: "Recent work emphasized rollout notes and architecture review.".to_owned(),
                node_ids: vec![node_a.id, node_b.id, node_c.id],
                lesson_ids: Vec::new(),
                trait_ids: Vec::new(),
                created_at: now,
                updated_at: now,
            }],
            traits: vec![TraitState {
                id: TraitId(Uuid::new_v4()),
                trait_type: TraitType::Practicality,
                status: MemoryStatus::Active,
                label: "Practicality".to_owned(),
                description: "Optimizes for useful plans.".to_owned(),
                strength: 0.8,
                confidence: 0.75,
                supporting_lesson_ids: Vec::new(),
                supporting_node_ids: vec![node_c.id],
                created_at: now,
                updated_at: now,
            }],
        }
    }

    fn node(
        title: &str,
        summary: &str,
        content: Option<&str>,
        confidence: f32,
        importance: f32,
        updated_at: chrono::DateTime<Utc>,
        tags: Vec<String>,
    ) -> Node {
        Node {
            id: NodeId(Uuid::new_v4()),
            node_type: NodeType::Semantic,
            status: MemoryStatus::Active,
            title: title.to_owned(),
            summary: summary.to_owned(),
            content: content.map(str::to_owned),
            tags,
            confidence,
            importance,
            created_at: updated_at,
            updated_at,
            last_accessed_at: None,
            source_event_id: Some("evt-1".to_owned()),
        }
    }

    fn edge(from_node_id: NodeId, to_node_id: NodeId, weight: f32) -> Edge {
        Edge {
            id: EdgeId(Uuid::new_v4()),
            edge_type: EdgeType::RelatedTo,
            from_node_id,
            to_node_id,
            weight,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn build_vector_store(nodes: &[Node], embeddings: &HashMap<String, Vec<f32>>) -> StoreRuntime {
        let tempdir = tempdir().expect("temporary directory should be created");
        let db_dir = tempdir.path().to_path_buf();
        std::mem::forget(tempdir);
        let config = StoreConfig {
            local_database_path: db_dir.join("retrieval-vectors.db"),
            ..StoreConfig::default()
        };

        let runtime = run_async(StoreRuntime::open(config)).expect("store should open");
        let repository = runtime.repository();
        run_async(async {
            for node in nodes {
                repository.insert_node(node).await?;
                if let Some(embedding) = embeddings.get(&node.title) {
                    repository
                        .upsert_node_embedding(&NodeEmbeddingRecord {
                            node_id: node.id,
                            embedding_model: "test-model".to_owned(),
                            embedding: embedding.clone(),
                        })
                        .await?;
                }
            }

            Result::<(), memory_store::StoreError>::Ok(())
        })
        .expect("nodes and embeddings should persist");

        runtime
    }

    fn sample_turso_vector_search(
        database: &libsql::Database,
    ) -> TursoVectorSearch<TestQueryEmbedder> {
        TursoVectorSearch::new(
            database
                .connect()
                .expect("vector search connection should open"),
            TestQueryEmbedder {
                embeddings: HashMap::from([
                    (
                        "deployment playbook".to_owned(),
                        EmbeddedQuery {
                            embedding_model: "test-model".to_owned(),
                            embedding: vec![1.0, 0.0, 0.0],
                        },
                    ),
                    (
                        "system design".to_owned(),
                        EmbeddedQuery {
                            embedding_model: "test-model".to_owned(),
                            embedding: vec![0.75, 0.65, 0.0],
                        },
                    ),
                    (
                        "system design architecture".to_owned(),
                        EmbeddedQuery {
                            embedding_model: "test-model".to_owned(),
                            embedding: vec![0.7, 0.7, 0.0],
                        },
                    ),
                ]),
            },
        )
    }

    fn sample_embeddings() -> HashMap<String, Vec<f32>> {
        HashMap::from([
            ("migration rollout notes".to_owned(), vec![1.0, 0.0, 0.0]),
            ("architecture review".to_owned(), vec![0.75, 0.65, 0.0]),
            ("release planning".to_owned(), vec![0.85, 0.2, 0.0]),
            ("Turso embedded".to_owned(), vec![0.1, 0.95, 0.0]),
            ("OpenClaw adapter".to_owned(), vec![0.0, 0.1, 1.0]),
        ])
    }

    fn run_async<F>(future: F) -> F::Output
    where
        F: Future,
    {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should be created")
            .block_on(future)
    }
}
