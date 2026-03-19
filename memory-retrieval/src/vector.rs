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

pub trait VectorSearch {
    fn search(
        &self,
        query: &MemoryQuery,
        nodes: &[Node],
        limit: usize,
    ) -> Result<Vec<VectorCandidate>, RetrievalError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedQuery {
    pub embedding_model: String,
    pub embedding: Vec<f32>,
}

pub trait QueryEmbeddingProvider {
    fn embed_query(&self, query: &MemoryQuery) -> Result<EmbeddedQuery, RetrievalError>;
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
