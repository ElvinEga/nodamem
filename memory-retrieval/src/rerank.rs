use std::collections::HashMap;

use chrono::{Duration, Utc};
use memory_core::{Edge, Node, NodeId};

use crate::graph::NeighborCandidate;
use crate::lexical::LexicalCandidate;
use crate::{RetrievalScoreBreakdown};
use crate::vector::VectorCandidate;

#[derive(Debug, Clone, PartialEq)]
pub struct HybridCandidate {
    pub node: Node,
    pub score: RetrievalScoreBreakdown,
    pub hop_distance: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HybridWeights {
    pub lexical: f32,
    pub vector: f32,
    pub edge_strength: f32,
    pub recency: f32,
    pub importance: f32,
    pub confidence: f32,
}

impl Default for HybridWeights {
    fn default() -> Self {
        Self {
            lexical: 0.30,
            vector: 0.30,
            edge_strength: 0.15,
            recency: 0.10,
            importance: 0.10,
            confidence: 0.05,
        }
    }
}

pub fn merge_and_rank(
    nodes: &[Node],
    edges: &[Edge],
    lexical_hits: &[LexicalCandidate],
    vector_hits: &[VectorCandidate],
    neighbor_hits: &[NeighborCandidate],
    weights: &HybridWeights,
) -> Vec<HybridCandidate> {
    let lexical_max = lexical_hits
        .iter()
        .map(|candidate| candidate.lexical_score)
        .fold(0.0f32, f32::max)
        .max(1.0);
    let vector_max = vector_hits
        .iter()
        .map(|candidate| candidate.vector_similarity_score)
        .fold(0.0f32, f32::max)
        .max(1.0);

    let lexical_scores = lexical_hits
        .iter()
        .map(|candidate| (candidate.node_id, (candidate.lexical_score / lexical_max).clamp(0.0, 1.0)))
        .collect::<HashMap<_, _>>();
    let vector_scores = vector_hits
        .iter()
        .map(|candidate| {
            (
                candidate.node_id,
                (candidate.vector_similarity_score / vector_max).clamp(0.0, 1.0),
            )
        })
        .collect::<HashMap<_, _>>();
    let neighbor_scores = neighbor_hits
        .iter()
        .map(|candidate| (candidate.node_id, candidate.clone()))
        .collect::<HashMap<_, _>>();

    let mut ranked = nodes
        .iter()
        .filter_map(|node| {
            let lexical_score = lexical_scores.get(&node.id).copied().unwrap_or_default();
            let vector_score = vector_scores.get(&node.id).copied().unwrap_or_default();
            let neighbor = neighbor_scores.get(&node.id);
            let edge_strength = neighbor
                .map(|candidate| candidate.edge_strength.clamp(0.0, 1.0))
                .unwrap_or_default();
            let recency = recency_score(node);
            let importance = node.importance.clamp(0.0, 1.0);
            let confidence = node.confidence.clamp(0.0, 1.0);
            let centrality = centrality_score(node.id, edges);
            let total = (lexical_score * weights.lexical
                + vector_score * weights.vector
                + edge_strength * weights.edge_strength
                + recency * weights.recency
                + importance * weights.importance
                + confidence * weights.confidence)
                .clamp(0.0, 1.0);

            (lexical_score > 0.0 || vector_score > 0.0 || edge_strength > 0.0).then_some(
                HybridCandidate {
                    node: node.clone(),
                    score: RetrievalScoreBreakdown {
                        lexical_score,
                        vector_score,
                        edge_strength,
                        recency,
                        importance,
                        confidence,
                        centrality,
                        total,
                    },
                    hop_distance: neighbor.map(|candidate| candidate.hop_distance),
                },
            )
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| right.score.total.total_cmp(&left.score.total));
    ranked
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
