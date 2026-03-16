//! Audit-oriented inspection views for explaining why graph records exist.

use memory_core::{Checkpoint, Edge, Lesson, Node, TraitState};

/// Store-backed explanation for why a node exists and how it connects into the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeAuditTrail {
    pub node: Node,
    pub inbound_edges: Vec<Edge>,
    pub outbound_edges: Vec<Edge>,
    pub supporting_lessons: Vec<Lesson>,
    pub contradicting_lessons: Vec<Lesson>,
    pub supporting_traits: Vec<TraitState>,
    pub checkpoints: Vec<Checkpoint>,
    pub reasons: Vec<String>,
}

/// Store-backed explanation for why a lesson exists and what currently supports it.
#[derive(Debug, Clone, PartialEq)]
pub struct LessonAuditTrail {
    pub lesson: Lesson,
    pub supporting_nodes: Vec<Node>,
    pub contradicting_nodes: Vec<Node>,
    pub influenced_traits: Vec<TraitState>,
    pub checkpoints: Vec<Checkpoint>,
    pub reasons: Vec<String>,
}
