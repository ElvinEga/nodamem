//! Deterministic background consolidation jobs for Nodamem.

use std::collections::{HashMap, HashSet};

use chrono::Duration;
use memory_core::{
    Checkpoint, CheckpointId, Edge, Lesson, MemoryStatus, Node, NodeId, Timestamp, TraitState,
};
use memory_lessons::LessonsMarker;
use uuid::Uuid;

/// Lightweight marker preserved for workspace wiring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SleepMarker {
    pub lessons: LessonsMarker,
}

/// In-memory state consumed and mutated by deterministic consolidation jobs.
#[derive(Debug, Clone, Default)]
pub struct SleepState {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub lessons: Vec<Lesson>,
    pub traits: Vec<TraitState>,
    pub checkpoints: Vec<Checkpoint>,
    pub recall_counts: HashMap<NodeId, u32>,
}

/// Policy for safe and deterministic maintenance behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct SleepPolicy {
    pub checkpoint_node_limit: usize,
    pub checkpoint_lesson_limit: usize,
    pub duplicate_similarity_threshold: f32,
    pub duplicate_weight_merge_cap: f32,
    pub weak_edge_threshold: f32,
    pub weak_edge_decay: f32,
    pub stale_edge_days: i64,
    pub isolated_importance_threshold: f32,
    pub isolated_confidence_threshold: f32,
    pub lesson_reinforcement_threshold: usize,
    pub lesson_confidence_increment: f32,
    pub reconsolidation_recall_threshold: u32,
    pub reconsolidation_confidence_increment: f32,
    pub reconsolidation_importance_increment: f32,
}

impl Default for SleepPolicy {
    fn default() -> Self {
        Self {
            checkpoint_node_limit: 5,
            checkpoint_lesson_limit: 3,
            duplicate_similarity_threshold: 0.88,
            duplicate_weight_merge_cap: 1.0,
            weak_edge_threshold: 0.35,
            weak_edge_decay: 0.1,
            stale_edge_days: 14,
            isolated_importance_threshold: 0.35,
            isolated_confidence_threshold: 0.45,
            lesson_reinforcement_threshold: 2,
            lesson_confidence_increment: 0.04,
            reconsolidation_recall_threshold: 3,
            reconsolidation_confidence_increment: 0.03,
            reconsolidation_importance_increment: 0.02,
        }
    }
}

/// Structured report for a single job run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReport {
    pub job_name: &'static str,
    pub changes: usize,
    pub logs: Vec<String>,
}

impl JobReport {
    fn unchanged(job_name: &'static str) -> Self {
        Self {
            job_name,
            changes: 0,
            logs: vec!["no changes".to_owned()],
        }
    }
}

/// Aggregate output from a scheduler run.
#[derive(Debug, Clone)]
pub struct SleepRunResult {
    pub state: SleepState,
    pub reports: Vec<JobReport>,
}

/// Interface for a deterministic maintenance job.
pub trait SleepJob {
    fn name(&self) -> &'static str;

    fn run(&self, state: &mut SleepState, policy: &SleepPolicy, now: Timestamp) -> JobReport;
}

/// Scheduler abstraction for running background consolidation work.
pub trait SleepScheduler {
    fn run_all(&self, state: SleepState, policy: &SleepPolicy, now: Timestamp) -> SleepRunResult;
}

/// Default runner that executes all configured jobs in sequence.
#[derive(Debug, Clone)]
pub struct SleepRunner {
    jobs: Vec<JobKind>,
}

impl SleepRunner {
    #[must_use]
    pub fn new(jobs: Vec<JobKind>) -> Self {
        Self { jobs }
    }
}

impl Default for SleepRunner {
    fn default() -> Self {
        Self::new(vec![
            JobKind::CheckpointGeneration,
            JobKind::DuplicateMerging,
            JobKind::WeakEdgeDecay,
            JobKind::ArchiveIsolatedWeakNodes,
            JobKind::LessonReinforcement,
            JobKind::Reconsolidation,
        ])
    }
}

impl SleepScheduler for SleepRunner {
    fn run_all(
        &self,
        mut state: SleepState,
        policy: &SleepPolicy,
        now: Timestamp,
    ) -> SleepRunResult {
        let reports = self
            .jobs
            .iter()
            .map(|job| job.run(&mut state, policy, now))
            .collect();

        SleepRunResult { state, reports }
    }
}

/// Available built-in sleep jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    CheckpointGeneration,
    DuplicateMerging,
    WeakEdgeDecay,
    ArchiveIsolatedWeakNodes,
    LessonReinforcement,
    Reconsolidation,
}

impl SleepJob for JobKind {
    fn name(&self) -> &'static str {
        match self {
            Self::CheckpointGeneration => "checkpoint_generation",
            Self::DuplicateMerging => "duplicate_merging",
            Self::WeakEdgeDecay => "weak_edge_decay",
            Self::ArchiveIsolatedWeakNodes => "archive_isolated_weak_nodes",
            Self::LessonReinforcement => "lesson_reinforcement",
            Self::Reconsolidation => "reconsolidation",
        }
    }

    fn run(&self, state: &mut SleepState, policy: &SleepPolicy, now: Timestamp) -> JobReport {
        match self {
            Self::CheckpointGeneration => run_checkpoint_generation(state, policy, now),
            Self::DuplicateMerging => run_duplicate_merging(state, policy, now),
            Self::WeakEdgeDecay => run_weak_edge_decay(state, policy, now),
            Self::ArchiveIsolatedWeakNodes => run_archive_isolated_weak_nodes(state, policy, now),
            Self::LessonReinforcement => run_lesson_reinforcement(state, policy, now),
            Self::Reconsolidation => run_reconsolidation(state, policy, now),
        }
    }
}

fn run_checkpoint_generation(
    state: &mut SleepState,
    policy: &SleepPolicy,
    now: Timestamp,
) -> JobReport {
    let mut active_nodes: Vec<&Node> = state
        .nodes
        .iter()
        .filter(|node| matches!(node.status, MemoryStatus::Active | MemoryStatus::Reinforced))
        .collect();
    active_nodes.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.importance.total_cmp(&left.importance))
    });

    let node_ids: Vec<NodeId> = active_nodes
        .iter()
        .take(policy.checkpoint_node_limit)
        .map(|node| node.id)
        .collect();

    if node_ids.is_empty() {
        return JobReport::unchanged(JobKind::CheckpointGeneration.name());
    }

    let mut lessons: Vec<&Lesson> = state
        .lessons
        .iter()
        .filter(|lesson| {
            lesson
                .supporting_node_ids
                .iter()
                .any(|node_id| node_ids.contains(node_id))
        })
        .collect();
    lessons.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.confidence.total_cmp(&left.confidence))
    });
    let lesson_ids = lessons
        .iter()
        .take(policy.checkpoint_lesson_limit)
        .map(|lesson| lesson.id)
        .collect::<Vec<_>>();

    let summary = format!(
        "Checkpointed {} active nodes and {} lessons for background recall.",
        node_ids.len(),
        lesson_ids.len()
    );

    let checkpoint_id = CheckpointId(Uuid::new_v4());
    state.checkpoints.push(Checkpoint {
        id: checkpoint_id,
        status: MemoryStatus::Active,
        title: format!("Checkpoint {}", now.format("%Y-%m-%d %H:%M:%S UTC")),
        summary,
        node_ids,
        lesson_ids,
        trait_ids: state.traits.iter().map(|entry| entry.id).collect(),
        created_at: now,
        updated_at: now,
    });

    JobReport {
        job_name: JobKind::CheckpointGeneration.name(),
        changes: 1,
        logs: vec![format!("created checkpoint {}", checkpoint_id.0)],
    }
}

fn run_duplicate_merging(
    state: &mut SleepState,
    policy: &SleepPolicy,
    now: Timestamp,
) -> JobReport {
    let mut changes = 0;
    let mut logs = Vec::new();
    let mut archived_ids = HashSet::new();

    for left_index in 0..state.nodes.len() {
        if archived_ids.contains(&state.nodes[left_index].id) {
            continue;
        }

        for right_index in (left_index + 1)..state.nodes.len() {
            if archived_ids.contains(&state.nodes[right_index].id) {
                continue;
            }

            let similarity = node_similarity(&state.nodes[left_index], &state.nodes[right_index]);
            if similarity < policy.duplicate_similarity_threshold {
                continue;
            }

            let (survivor_index, archived_index) = choose_duplicate_survivor(
                &state.nodes[left_index],
                &state.nodes[right_index],
                left_index,
                right_index,
            );

            let survivor_id = state.nodes[survivor_index].id;
            let archived_id = state.nodes[archived_index].id;

            for edge in &mut state.edges {
                if edge.from_node_id == archived_id {
                    edge.from_node_id = survivor_id;
                    edge.weight = edge.weight.min(policy.duplicate_weight_merge_cap);
                    edge.updated_at = now;
                }

                if edge.to_node_id == archived_id {
                    edge.to_node_id = survivor_id;
                    edge.weight = edge.weight.min(policy.duplicate_weight_merge_cap);
                    edge.updated_at = now;
                }
            }

            for lesson in &mut state.lessons {
                replace_node_id(&mut lesson.supporting_node_ids, archived_id, survivor_id);
                replace_node_id(&mut lesson.contradicting_node_ids, archived_id, survivor_id);
            }

            for trait_state in &mut state.traits {
                replace_node_id(
                    &mut trait_state.supporting_node_ids,
                    archived_id,
                    survivor_id,
                );
            }

            let archived = &mut state.nodes[archived_index];
            archived.status = MemoryStatus::Archived;
            archived.updated_at = now;
            archived_ids.insert(archived_id);
            changes += 1;
            logs.push(format!(
                "archived duplicate node {} into survivor {} with similarity {:.2}",
                archived_id.0, survivor_id.0, similarity
            ));
        }
    }

    if changes == 0 {
        JobReport::unchanged(JobKind::DuplicateMerging.name())
    } else {
        JobReport {
            job_name: JobKind::DuplicateMerging.name(),
            changes,
            logs,
        }
    }
}

fn run_weak_edge_decay(state: &mut SleepState, policy: &SleepPolicy, now: Timestamp) -> JobReport {
    let stale_after = now - Duration::days(policy.stale_edge_days);
    let mut changes = 0;
    let mut logs = Vec::new();

    for edge in &mut state.edges {
        if edge.weight > policy.weak_edge_threshold || edge.updated_at >= stale_after {
            continue;
        }

        let previous = edge.weight;
        edge.weight = (edge.weight - policy.weak_edge_decay).max(0.0);
        edge.updated_at = now;
        changes += 1;
        logs.push(format!(
            "decayed edge {} from {:.2} to {:.2}",
            edge.id.0, previous, edge.weight
        ));
    }

    if changes == 0 {
        JobReport::unchanged(JobKind::WeakEdgeDecay.name())
    } else {
        JobReport {
            job_name: JobKind::WeakEdgeDecay.name(),
            changes,
            logs,
        }
    }
}

fn run_archive_isolated_weak_nodes(
    state: &mut SleepState,
    policy: &SleepPolicy,
    now: Timestamp,
) -> JobReport {
    let connected_nodes = connected_node_ids(&state.edges);
    let mut changes = 0;
    let mut logs = Vec::new();

    for node in &mut state.nodes {
        if connected_nodes.contains(&node.id) {
            continue;
        }

        if node.importance >= policy.isolated_importance_threshold
            || node.confidence >= policy.isolated_confidence_threshold
        {
            continue;
        }

        if matches!(node.status, MemoryStatus::Archived | MemoryStatus::Pruned) {
            continue;
        }

        node.status = MemoryStatus::Archived;
        node.updated_at = now;
        changes += 1;
        logs.push(format!("archived isolated weak node {}", node.id.0));
    }

    if changes == 0 {
        JobReport::unchanged(JobKind::ArchiveIsolatedWeakNodes.name())
    } else {
        JobReport {
            job_name: JobKind::ArchiveIsolatedWeakNodes.name(),
            changes,
            logs,
        }
    }
}

fn run_lesson_reinforcement(
    state: &mut SleepState,
    policy: &SleepPolicy,
    now: Timestamp,
) -> JobReport {
    let strong_nodes: HashSet<NodeId> = state
        .nodes
        .iter()
        .filter(|node| matches!(node.status, MemoryStatus::Active | MemoryStatus::Reinforced))
        .map(|node| node.id)
        .collect();

    let mut changes = 0;
    let mut logs = Vec::new();

    for lesson in &mut state.lessons {
        let support_count = lesson
            .supporting_node_ids
            .iter()
            .filter(|node_id| strong_nodes.contains(node_id))
            .count();

        if support_count < policy.lesson_reinforcement_threshold {
            continue;
        }

        lesson.reinforcement_count += 1;
        lesson.evidence_count += support_count as u32;
        lesson.confidence = (lesson.confidence + policy.lesson_confidence_increment).min(1.0);
        lesson.status = MemoryStatus::Reinforced;
        lesson.updated_at = now;
        changes += 1;
        logs.push(format!(
            "reinforced lesson {} with {} active supporting nodes",
            lesson.id.0, support_count
        ));
    }

    if changes == 0 {
        JobReport::unchanged(JobKind::LessonReinforcement.name())
    } else {
        JobReport {
            job_name: JobKind::LessonReinforcement.name(),
            changes,
            logs,
        }
    }
}

fn run_reconsolidation(state: &mut SleepState, policy: &SleepPolicy, now: Timestamp) -> JobReport {
    let mut changes = 0;
    let mut logs = Vec::new();

    for node in &mut state.nodes {
        let recall_count = state
            .recall_counts
            .get(&node.id)
            .copied()
            .unwrap_or_default();
        if recall_count < policy.reconsolidation_recall_threshold {
            continue;
        }

        node.confidence = (node.confidence + policy.reconsolidation_confidence_increment).min(1.0);
        node.importance = (node.importance + policy.reconsolidation_importance_increment).min(1.0);
        node.status = MemoryStatus::Reinforced;
        node.last_accessed_at = Some(now);
        node.updated_at = now;
        changes += 1;
        logs.push(format!(
            "reconsolidated node {} after {} recalls",
            node.id.0, recall_count
        ));
    }

    if changes == 0 {
        JobReport::unchanged(JobKind::Reconsolidation.name())
    } else {
        JobReport {
            job_name: JobKind::Reconsolidation.name(),
            changes,
            logs,
        }
    }
}

fn connected_node_ids(edges: &[Edge]) -> HashSet<NodeId> {
    edges
        .iter()
        .flat_map(|edge| [edge.from_node_id, edge.to_node_id])
        .collect()
}

fn replace_node_id(node_ids: &mut Vec<NodeId>, old_id: NodeId, new_id: NodeId) {
    for node_id in node_ids.iter_mut() {
        if *node_id == old_id {
            *node_id = new_id;
        }
    }

    let mut seen = HashSet::new();
    node_ids.retain(|node_id| seen.insert(*node_id));
}

fn choose_duplicate_survivor(
    left: &Node,
    right: &Node,
    left_index: usize,
    right_index: usize,
) -> (usize, usize) {
    let left_score = left.importance + left.confidence;
    let right_score = right.importance + right.confidence;

    if right_score > left_score {
        (right_index, left_index)
    } else {
        (left_index, right_index)
    }
}

fn node_similarity(left: &Node, right: &Node) -> f32 {
    let title_overlap = text_overlap(&left.title, &right.title);
    let summary_overlap = text_overlap(&left.summary, &right.summary);
    let type_bonus = if left.node_type == right.node_type {
        0.2
    } else {
        0.0
    };

    ((title_overlap * 0.6) + (summary_overlap * 0.4) + type_bonus).clamp(0.0, 1.0)
}

fn text_overlap(left: &str, right: &str) -> f32 {
    let left_tokens = tokenize(left);
    let right_tokens = tokenize(right);

    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0.0;
    }

    let shared = left_tokens.intersection(&right_tokens).count() as f32;
    let total = left_tokens.union(&right_tokens).count() as f32;
    shared / total
}

fn tokenize(input: &str) -> HashSet<String> {
    input
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.len() > 2)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use memory_core::{
        EdgeId, EdgeType, LessonId, LessonType, MemoryStatus, NodeType, TraitId, TraitType,
    };

    #[test]
    fn archives_isolated_low_value_nodes() {
        let now = Utc::now();
        let weak_node = sample_node("weak node", "isolated note", 0.1, 0.2, now);
        let strong_node = sample_node("strong node", "connected note", 0.8, 0.8, now);
        let connected_edge = sample_edge(strong_node.id, NodeId(Uuid::new_v4()), 0.7, now);

        let state = SleepState {
            nodes: vec![weak_node.clone(), strong_node],
            edges: vec![connected_edge],
            ..SleepState::default()
        };

        let result = SleepRunner::new(vec![JobKind::ArchiveIsolatedWeakNodes]).run_all(
            state,
            &SleepPolicy::default(),
            now,
        );

        let archived = result
            .state
            .nodes
            .iter()
            .find(|node| node.id == weak_node.id)
            .expect("weak node should remain present");
        assert_eq!(archived.status, MemoryStatus::Archived);
        assert_eq!(result.reports[0].changes, 1);
    }

    #[test]
    fn reconsolidates_frequently_recalled_nodes() {
        let now = Utc::now();
        let node = sample_node("important recall", "often used memory", 0.5, 0.4, now);
        let node_id = node.id;
        let mut recall_counts = HashMap::new();
        recall_counts.insert(node_id, 4);

        let state = SleepState {
            nodes: vec![node],
            recall_counts,
            ..SleepState::default()
        };

        let result = SleepRunner::new(vec![JobKind::Reconsolidation]).run_all(
            state,
            &SleepPolicy::default(),
            now,
        );

        let updated = &result.state.nodes[0];
        assert_eq!(updated.status, MemoryStatus::Reinforced);
        assert!(updated.confidence > 0.5);
        assert!(updated.importance > 0.4);
        assert_eq!(updated.last_accessed_at, Some(now));
    }

    #[test]
    fn merges_duplicate_nodes_safely() {
        let now = Utc::now();
        let original = sample_node(
            "rust workspace layout",
            "set up rust workspace crates",
            0.8,
            0.7,
            now,
        );
        let duplicate = sample_node(
            "rust workspace layout",
            "set up rust workspace crates",
            0.6,
            0.6,
            now,
        );
        let edge = sample_edge(duplicate.id, NodeId(Uuid::new_v4()), 0.5, now);

        let state = SleepState {
            nodes: vec![original.clone(), duplicate.clone()],
            edges: vec![edge],
            ..SleepState::default()
        };

        let result = SleepRunner::new(vec![JobKind::DuplicateMerging]).run_all(
            state,
            &SleepPolicy::default(),
            now,
        );

        let archived = result
            .state
            .nodes
            .iter()
            .find(|node| node.id == duplicate.id)
            .expect("duplicate node should still exist");
        assert_eq!(archived.status, MemoryStatus::Archived);
        assert_eq!(result.reports[0].changes, 1);
        assert!(result
            .state
            .edges
            .iter()
            .all(|entry| entry.from_node_id != duplicate.id));
    }

    #[test]
    fn decays_stale_weak_edges() {
        let now = Utc::now();
        let old = now - Duration::days(30);
        let edge = sample_edge(NodeId(Uuid::new_v4()), NodeId(Uuid::new_v4()), 0.2, old);

        let state = SleepState {
            edges: vec![edge],
            ..SleepState::default()
        };

        let result = SleepRunner::new(vec![JobKind::WeakEdgeDecay]).run_all(
            state,
            &SleepPolicy::default(),
            now,
        );

        assert_eq!(result.reports[0].changes, 1);
        assert!(result.state.edges[0].weight < 0.2);
    }

    #[test]
    fn creates_checkpoint_and_reinforces_lessons() {
        let now = Utc::now();
        let node_a = sample_node("task memory", "active node one", 0.8, 0.7, now);
        let node_b = sample_node("task memory two", "active node two", 0.7, 0.6, now);
        let lesson = Lesson {
            id: LessonId(Uuid::new_v4()),
            lesson_type: LessonType::Task,
            status: MemoryStatus::Active,
            title: "Batch related work".to_owned(),
            statement: "Batch related work to reduce context switching.".to_owned(),
            confidence: 0.6,
            evidence_count: 1,
            reinforcement_count: 0,
            supporting_node_ids: vec![node_a.id, node_b.id],
            contradicting_node_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        let trait_state = TraitState {
            id: TraitId(Uuid::new_v4()),
            trait_type: TraitType::Practicality,
            status: MemoryStatus::Active,
            label: "Practicality".to_owned(),
            description: "Bias toward concrete solutions.".to_owned(),
            strength: 0.6,
            confidence: 0.7,
            supporting_lesson_ids: vec![lesson.id],
            supporting_node_ids: vec![node_a.id],
            created_at: now,
            updated_at: now,
        };

        let state = SleepState {
            nodes: vec![node_a, node_b],
            lessons: vec![lesson],
            traits: vec![trait_state],
            ..SleepState::default()
        };

        let result = SleepRunner::new(vec![
            JobKind::LessonReinforcement,
            JobKind::CheckpointGeneration,
        ])
        .run_all(state, &SleepPolicy::default(), now);

        assert_eq!(result.state.lessons[0].status, MemoryStatus::Reinforced);
        assert_eq!(result.state.lessons[0].reinforcement_count, 1);
        assert_eq!(result.state.checkpoints.len(), 1);
        assert_eq!(result.reports.len(), 2);
    }

    fn sample_node(
        title: &str,
        summary: &str,
        confidence: f32,
        importance: f32,
        now: Timestamp,
    ) -> Node {
        Node {
            id: NodeId(Uuid::new_v4()),
            node_type: NodeType::Episodic,
            status: MemoryStatus::Active,
            title: title.to_owned(),
            summary: summary.to_owned(),
            content: None,
            tags: Vec::new(),
            confidence,
            importance,
            created_at: now,
            updated_at: now,
            last_accessed_at: None,
            source_event_id: None,
        }
    }

    fn sample_edge(from_node_id: NodeId, to_node_id: NodeId, weight: f32, now: Timestamp) -> Edge {
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
