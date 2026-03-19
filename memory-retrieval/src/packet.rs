use std::collections::HashSet;

use chrono::Utc;
use memory_core::{
    Checkpoint, Edge, Lesson, MemoryPacket, MemoryPacketId, Node, NodeId, SelfModel, TraitState,
};
use uuid::Uuid;

use crate::rerank::HybridCandidate;
use crate::{MemoryQuery, RetrievalPolicy, RetrievedMemoryPacket};

pub fn assemble_memory_packet(
    query: &MemoryQuery,
    _nodes: &[Node],
    edges: &[Edge],
    lessons: &[Lesson],
    checkpoint_summary: Option<Checkpoint>,
    trait_snapshot: Option<TraitState>,
    self_model_snapshot: Option<SelfModel>,
    ranked: &[HybridCandidate],
    policy: &RetrievalPolicy,
) -> RetrievedMemoryPacket {
    let core_nodes = ranked
        .iter()
        .filter(|candidate| candidate.score.total >= policy.min_score)
        .take(policy.core_node_limit.clamp(3, 5))
        .map(|candidate| candidate.node.clone())
        .collect::<Vec<_>>();

    let core_ids = core_nodes
        .iter()
        .map(|node| node.id)
        .collect::<HashSet<_>>();
    let related_neighbors = ranked
        .iter()
        .filter(|candidate| !core_ids.contains(&candidate.node.id))
        .take(policy.neighbor_limit.clamp(2, 3))
        .map(|candidate| candidate.node.clone())
        .collect::<Vec<_>>();

    let selected_ids = core_nodes
        .iter()
        .chain(related_neighbors.iter())
        .map(|node| node.id)
        .collect::<HashSet<_>>();

    let packet_edges = edges
        .iter()
        .filter(|edge| {
            selected_ids.contains(&edge.from_node_id) && selected_ids.contains(&edge.to_node_id)
        })
        .cloned()
        .collect::<Vec<_>>();

    let packet_lessons = select_lessons(lessons, &selected_ids, policy.lesson_limit.clamp(1, 2));

    let mut packet_nodes = core_nodes.clone();
    packet_nodes.extend(related_neighbors.clone());

    let packet = MemoryPacket {
        id: MemoryPacketId(Uuid::new_v4()),
        request_id: query.session_id.clone(),
        created_at: Utc::now(),
        nodes: packet_nodes,
        edges: packet_edges,
        lessons: packet_lessons.clone(),
        traits: trait_snapshot.iter().cloned().collect(),
        self_model_snapshot,
        checkpoints: checkpoint_summary.iter().cloned().collect(),
        imagined_scenarios: Vec::new(),
    };

    RetrievedMemoryPacket {
        core_nodes,
        related_neighbors,
        lessons: packet_lessons,
        checkpoint_summary,
        trait_snapshot,
        packet,
    }
}

fn select_lessons(
    lessons: &[Lesson],
    selected_node_ids: &HashSet<NodeId>,
    limit: usize,
) -> Vec<Lesson> {
    let mut ranked_lessons = lessons
        .iter()
        .filter_map(|lesson| {
            let overlap = lesson
                .supporting_node_ids
                .iter()
                .filter(|node_id| selected_node_ids.contains(node_id))
                .count() as f32;
            let score = overlap * 0.6 + lesson.confidence.clamp(0.0, 1.0) * 0.4;
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
