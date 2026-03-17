use memory_core::{Node, NodeId};

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
