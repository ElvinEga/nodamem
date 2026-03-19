use std::future::Future;

use libsql::Connection;
use memory_core::{Node, NodeId};
use memory_store::StoreRepository;
use tracing::debug;

use crate::{MemoryQuery, RetrievalError};

#[derive(Debug, Clone, PartialEq)]
pub struct VectorCandidate {
    pub node_id: NodeId,
    pub vector_similarity_score: f32,
}

pub trait VectorSearch: Send + Sync {
    fn search(
        &self,
        query: &MemoryQuery,
        nodes: &[Node],
        limit: usize,
    ) -> Result<Vec<VectorCandidate>, RetrievalError>;
}

impl<T> VectorSearch for &T
where
    T: VectorSearch + ?Sized,
{
    fn search(
        &self,
        query: &MemoryQuery,
        nodes: &[Node],
        limit: usize,
    ) -> Result<Vec<VectorCandidate>, RetrievalError> {
        (**self).search(query, nodes, limit)
    }
}

impl<T> VectorSearch for Box<T>
where
    T: VectorSearch + ?Sized,
{
    fn search(
        &self,
        query: &MemoryQuery,
        nodes: &[Node],
        limit: usize,
    ) -> Result<Vec<VectorCandidate>, RetrievalError> {
        (**self).search(query, nodes, limit)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedQuery {
    pub embedding_model: String,
    pub embedding: Vec<f32>,
}

pub trait QueryEmbeddingProvider: Send + Sync {
    fn embed_query(&self, query: &MemoryQuery) -> Result<EmbeddedQuery, RetrievalError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicQueryEmbedder {
    embedding_model: String,
    dimensions: usize,
}

impl DeterministicQueryEmbedder {
    pub const DEFAULT_EMBEDDING_MODEL: &'static str = "nodamem-semantic-v1";
    pub const DEFAULT_DIMENSIONS: usize = 64;

    #[must_use]
    pub fn new(embedding_model: impl Into<String>, dimensions: usize) -> Self {
        Self {
            embedding_model: embedding_model.into(),
            dimensions: dimensions.max(8),
        }
    }

    #[must_use]
    pub fn embedding_model(&self) -> &str {
        &self.embedding_model
    }

    #[must_use]
    pub fn embed_text(&self, text: &str) -> Vec<f32> {
        let tokens = normalized_tokens(text);
        let mut embedding = vec![0.0; self.dimensions];

        for token in &tokens {
            add_feature(&mut embedding, token, 1.0);
        }

        for pair in tokens.windows(2) {
            add_feature(&mut embedding, &format!("{}::{}", pair[0], pair[1]), 0.75);
        }

        normalize_embedding(&mut embedding);
        embedding
    }
}

impl Default for DeterministicQueryEmbedder {
    fn default() -> Self {
        Self::new(Self::DEFAULT_EMBEDDING_MODEL, Self::DEFAULT_DIMENSIONS)
    }
}

impl QueryEmbeddingProvider for DeterministicQueryEmbedder {
    fn embed_query(&self, query: &MemoryQuery) -> Result<EmbeddedQuery, RetrievalError> {
        let mut text = query.text.trim().to_owned();
        if let Some(topic) = query
            .topic
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(topic);
        }

        Ok(EmbeddedQuery {
            embedding_model: self.embedding_model.clone(),
            embedding: self.embed_text(&text),
        })
    }
}

#[derive(Debug)]
pub struct TursoVectorSearch<E> {
    connection: Connection,
    embedder: E,
}

impl<E> TursoVectorSearch<E> {
    #[must_use]
    pub fn new(connection: Connection, embedder: E) -> Self {
        Self {
            connection,
            embedder,
        }
    }
}

impl<E> VectorSearch for TursoVectorSearch<E>
where
    E: QueryEmbeddingProvider,
{
    fn search(
        &self,
        query: &MemoryQuery,
        nodes: &[Node],
        limit: usize,
    ) -> Result<Vec<VectorCandidate>, RetrievalError> {
        if query.text.trim().is_empty() || limit == 0 || nodes.is_empty() {
            return Ok(Vec::new());
        }

        let embedded_query = self.embedder.embed_query(query)?;
        let allowed_node_ids = nodes
            .iter()
            .map(|node| node.id)
            .collect::<std::collections::HashSet<_>>();
        let matches = block_on_store_future(async {
            let repository = StoreRepository::new(&self.connection);
            repository
                .search_node_embeddings(
                    &embedded_query.embedding,
                    &embedded_query.embedding_model,
                    limit as u32,
                )
                .await
                .map_err(|error| RetrievalError::Vector(error.to_string()))
        })?;

        let candidates = matches
            .into_iter()
            .filter(|candidate| allowed_node_ids.contains(&candidate.node_id))
            .map(|candidate| VectorCandidate {
                node_id: candidate.node_id,
                vector_similarity_score: candidate.similarity_score.clamp(0.0, 1.0),
            })
            .collect::<Vec<_>>();

        debug!(
            query = %query.text,
            embedding_model = %embedded_query.embedding_model,
            requested_limit = limit,
            returned_hits = candidates.len(),
            "turso vector hits collected"
        );

        Ok(candidates)
    }
}

fn block_on_store_future<F, T>(future: F) -> Result<T, RetrievalError>
where
    F: Future<Output = Result<T, RetrievalError>>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| RetrievalError::Vector(error.to_string()))?;
        runtime.block_on(future)
    }
}

/// Placeholder vector service for local-first operation when no embedding-backed lookup is wired in.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullVectorSearch;

impl VectorSearch for NullVectorSearch {
    fn search(
        &self,
        _query: &MemoryQuery,
        _nodes: &[Node],
        _limit: usize,
    ) -> Result<Vec<VectorCandidate>, RetrievalError> {
        Ok(Vec::new())
    }
}

fn normalized_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| canonicalize_token(&token.to_lowercase()))
        .filter(|token| !token.is_empty())
        .collect()
}

fn canonicalize_token(token: &str) -> String {
    match token {
        "deployment" | "deploy" | "release" | "cutover" | "rollout" => "rollout".to_owned(),
        "playbook" | "checklist" | "guide" | "runbook" => "guide".to_owned(),
        "design" | "architecture" | "boundary" | "boundaries" => "architecture".to_owned(),
        "memory" | "recall" | "retrieval" => "memory".to_owned(),
        "goal" | "objective" | "priority" => "goal".to_owned(),
        _ => token.to_owned(),
    }
}

fn add_feature(embedding: &mut [f32], feature: &str, weight: f32) {
    let index = stable_hash(feature) as usize % embedding.len();
    embedding[index] += weight;
}

fn stable_hash(input: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn normalize_embedding(embedding: &mut [f32]) {
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm > 0.0 {
        for value in embedding {
            *value /= norm;
        }
    }
}
