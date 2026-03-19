//! Imagination subsystem for generating planning-oriented hypothetical scenarios.

use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;
use std::fmt;

use chrono::Utc;
use memory_core::{
    CoreMarker, Edge, ImaginationStatus, ImaginedScenario, ImaginedScenarioKind, Lesson,
    MemoryPacket, Node, NodeId, NodeType, ScenarioId, SelfModel, TraitState, TraitType,
};
use tracing::{debug, info};
use uuid::Uuid;

/// Agent request for hypothetical scenarios grounded in retrieved memory context.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanningImaginationRequest {
    pub planning_task: String,
    pub desired_scenarios: usize,
    pub context_packet: MemoryPacket,
    pub active_goal_node_ids: Vec<NodeId>,
}

/// Agent-facing planning response. Imagined content remains separate from verified memory.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanningImaginationResponse {
    pub planning_task: String,
    pub scenarios: Vec<ImaginedScenario>,
}

/// Review decision for a simulated scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioReviewDecision {
    AcceptAsHypothesis,
    Reject,
}

/// Errors produced while generating hypothetical scenarios.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImaginationError {
    EmptyPlanningTask,
    EmptyContext,
}

impl fmt::Display for ImaginationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPlanningTask => write!(formatter, "planning task must not be empty"),
            Self::EmptyContext => {
                write!(formatter, "planning imagination requires verified context")
            }
        }
    }
}

impl StdError for ImaginationError {}

/// Configuration for deterministic planning imagination behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct ImaginationPolicy {
    pub default_scenario_count: usize,
    pub max_scenarios: usize,
    pub max_basis_nodes_per_scenario: usize,
    pub max_lessons_per_scenario: usize,
}

impl Default for ImaginationPolicy {
    fn default() -> Self {
        Self {
            default_scenario_count: 3,
            max_scenarios: 5,
            max_basis_nodes_per_scenario: 3,
            max_lessons_per_scenario: 2,
        }
    }
}

/// Agent-facing interface for planning-oriented hypothetical scenario generation.
pub trait PlanningImaginationApi {
    fn imagine_for_planning(
        &self,
        request: &PlanningImaginationRequest,
    ) -> Result<PlanningImaginationResponse, ImaginationError>;
}

/// Deterministic imagination engine grounded in connected memory, lessons, goals, traits, and self-model.
#[derive(Debug, Clone)]
pub struct ImaginationService {
    pub policy: ImaginationPolicy,
}

impl ImaginationService {
    #[must_use]
    pub fn new(policy: ImaginationPolicy) -> Self {
        Self { policy }
    }

    #[must_use]
    pub fn review_scenario(
        &self,
        scenario: &ImaginedScenario,
        decision: ScenarioReviewDecision,
    ) -> ImaginedScenario {
        let reviewed = ImaginedScenario {
            status: match decision {
                ScenarioReviewDecision::AcceptAsHypothesis => {
                    ImaginationStatus::AcceptedAsHypothesis
                }
                ScenarioReviewDecision::Reject => ImaginationStatus::Rejected,
            },
            updated_at: Utc::now(),
            ..scenario.clone()
        };

        info!(
            scenario_id = %reviewed.id.0,
            kind = ?reviewed.kind,
            decision = ?decision,
            basis_nodes = reviewed.basis_source_node_ids.len(),
            "scenario acceptance/rejection recorded"
        );

        reviewed
    }

    fn build_scenario(
        &self,
        planning_task: &str,
        cluster: &[&Node],
        goal: Option<&Node>,
        preferences: &[&Node],
        lessons: &[Lesson],
        trait_snapshot: &[TraitState],
        self_model_snapshot: Option<&SelfModel>,
        kind: ImaginedScenarioKind,
        scenario_index: usize,
    ) -> ImaginedScenario {
        let now = Utc::now();
        let cluster_summary = summarize_nodes(cluster);
        let goal_title = goal
            .map(|node| node.title.as_str())
            .unwrap_or("the current planning objective");
        let lesson_summary = summarize_lessons(lessons);
        let preference_summary = summarize_nodes(preferences);
        let trait_summary = summarize_traits(trait_snapshot);
        let self_model_summary = summarize_self_model(self_model_snapshot);
        let predicted_outcomes = build_predicted_outcomes(
            kind,
            planning_task,
            cluster,
            goal,
            preferences,
            lessons,
            trait_snapshot,
            self_model_snapshot,
        );

        let plausibility_score =
            plausibility_score(cluster, goal, lessons, trait_snapshot, self_model_snapshot);
        let novelty_score =
            novelty_score(cluster, lessons, trait_snapshot, self_model_snapshot, kind);
        let usefulness_score = usefulness_score(
            goal,
            preferences,
            lessons,
            trait_snapshot,
            self_model_snapshot,
            kind,
        );

        debug!(
            scenario_index,
            kind = ?kind,
            basis_nodes = cluster.len(),
            basis_lessons = lessons.len(),
            preferences = preferences.len(),
            plausibility_score,
            novelty_score,
            usefulness_score,
            "scenario scoring completed"
        );

        ImaginedScenario {
            id: ScenarioId(Uuid::new_v4()),
            kind,
            status: ImaginationStatus::Simulated,
            title: scenario_title(kind, scenario_index, goal_title),
            premise: scenario_premise(kind, planning_task, &cluster_summary, goal_title),
            narrative: scenario_narrative(
                kind,
                &cluster_summary,
                &lesson_summary,
                &preference_summary,
                &trait_summary,
                &self_model_summary,
            ),
            basis_source_node_ids: cluster.iter().map(|node| node.id).collect(),
            basis_lesson_ids: lessons.iter().map(|lesson| lesson.id).collect(),
            active_goal_node_ids: goal.into_iter().map(|node| node.id).collect(),
            trait_snapshot: trait_snapshot.to_vec(),
            self_model_snapshot: self_model_snapshot.cloned(),
            predicted_outcomes,
            plausibility_score,
            novelty_score,
            usefulness_score,
            created_at: now,
            updated_at: now,
        }
    }
}

impl Default for ImaginationService {
    fn default() -> Self {
        Self::new(ImaginationPolicy::default())
    }
}

impl PlanningImaginationApi for ImaginationService {
    fn imagine_for_planning(
        &self,
        request: &PlanningImaginationRequest,
    ) -> Result<PlanningImaginationResponse, ImaginationError> {
        if request.planning_task.trim().is_empty() {
            return Err(ImaginationError::EmptyPlanningTask);
        }

        let verified_nodes = request
            .context_packet
            .nodes
            .iter()
            .filter(|node| node.node_type != NodeType::Imagined)
            .cloned()
            .collect::<Vec<_>>();
        if verified_nodes.is_empty() {
            return Err(ImaginationError::EmptyContext);
        }

        let clusters = connected_clusters(&verified_nodes, &request.context_packet.edges);
        let goals = active_goals(&verified_nodes, &request.active_goal_node_ids);
        let preferences = active_preferences(&verified_nodes);
        let desired = match request.desired_scenarios {
            0 => self.policy.default_scenario_count,
            count => count.min(self.policy.max_scenarios),
        };

        debug!(
            planning_task = %request.planning_task,
            desired_scenarios = desired,
            cluster_count = clusters.len(),
            goal_count = goals.len(),
            preference_count = preferences.len(),
            lesson_count = request.context_packet.lessons.len(),
            has_self_model = request.context_packet.self_model_snapshot.is_some(),
            "scenario generation started"
        );

        let scenarios = (0..desired)
            .map(|index| {
                let cluster =
                    pick_cluster(&clusters, index, self.policy.max_basis_nodes_per_scenario);
                let goal = goals.get(index % goals.len().max(1));
                let supporting_lessons = select_supporting_lessons(
                    &request.context_packet.lessons,
                    cluster,
                    self.policy.max_lessons_per_scenario,
                );
                let scenario_kind = scenario_kind_for_index(index);
                let cluster_preferences = select_preference_basis(
                    &preferences,
                    cluster,
                    self.policy.max_lessons_per_scenario,
                );

                let scenario = self.build_scenario(
                    &request.planning_task,
                    cluster,
                    goal.copied(),
                    &cluster_preferences,
                    &supporting_lessons,
                    &request.context_packet.traits,
                    request.context_packet.self_model_snapshot.as_ref(),
                    scenario_kind,
                    index,
                );

                info!(
                    scenario_id = %scenario.id.0,
                    kind = ?scenario.kind,
                    basis_nodes = scenario.basis_source_node_ids.len(),
                    basis_lessons = scenario.basis_lesson_ids.len(),
                    has_self_model = scenario.self_model_snapshot.is_some(),
                    "scenario generation completed"
                );

                scenario
            })
            .collect::<Vec<_>>();

        Ok(PlanningImaginationResponse {
            planning_task: request.planning_task.clone(),
            scenarios,
        })
    }
}

fn scenario_kind_for_index(index: usize) -> ImaginedScenarioKind {
    match index % 3 {
        0 => ImaginedScenarioKind::FutureNeedPrediction,
        1 => ImaginedScenarioKind::AlternativePlan,
        _ => ImaginedScenarioKind::Counterfactual,
    }
}

fn connected_clusters<'a>(nodes: &'a [Node], edges: &[Edge]) -> Vec<Vec<&'a Node>> {
    let node_by_id = nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<HashMap<_, _>>();
    let mut adjacency = HashMap::<NodeId, Vec<NodeId>>::new();

    for edge in edges {
        if node_by_id.contains_key(&edge.from_node_id) && node_by_id.contains_key(&edge.to_node_id)
        {
            adjacency
                .entry(edge.from_node_id)
                .or_default()
                .push(edge.to_node_id);
            adjacency
                .entry(edge.to_node_id)
                .or_default()
                .push(edge.from_node_id);
        }
    }

    let mut visited = HashSet::new();
    let mut clusters = Vec::new();

    for node in nodes {
        if !visited.insert(node.id) {
            continue;
        }

        let mut stack = vec![node.id];
        let mut cluster = Vec::new();

        while let Some(current) = stack.pop() {
            if let Some(cluster_node) = node_by_id.get(&current) {
                cluster.push(*cluster_node);
            }

            for neighbor in adjacency.get(&current).into_iter().flatten() {
                if visited.insert(*neighbor) {
                    stack.push(*neighbor);
                }
            }
        }

        clusters.push(cluster);
    }

    clusters.sort_by(|left, right| {
        cluster_rank(right)
            .partial_cmp(&cluster_rank(left))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    clusters
}

fn pick_cluster<'a>(
    clusters: &'a [Vec<&'a Node>],
    index: usize,
    max_basis_nodes: usize,
) -> &'a [&'a Node] {
    let cluster = clusters
        .get(index % clusters.len().max(1))
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let capped_len = cluster.len().min(max_basis_nodes.max(1));
    &cluster[..capped_len]
}

fn active_goals<'a>(nodes: &'a [Node], requested_goal_ids: &[NodeId]) -> Vec<&'a Node> {
    let requested = requested_goal_ids.iter().copied().collect::<HashSet<_>>();
    let mut goals = nodes
        .iter()
        .filter(|node| {
            node.node_type == NodeType::Goal
                || (!requested.is_empty() && requested.contains(&node.id))
        })
        .collect::<Vec<_>>();

    goals.sort_by(|left, right| right.importance.total_cmp(&left.importance));

    if goals.is_empty() {
        nodes
            .iter()
            .max_by(|left, right| left.importance.total_cmp(&right.importance))
            .into_iter()
            .collect()
    } else {
        goals
    }
}

fn active_preferences(nodes: &[Node]) -> Vec<&Node> {
    let mut preferences = nodes
        .iter()
        .filter(|node| node.node_type == NodeType::Preference)
        .collect::<Vec<_>>();
    preferences.sort_by(|left, right| right.importance.total_cmp(&left.importance));
    preferences
}

fn select_preference_basis<'a>(
    preferences: &'a [&'a Node],
    cluster: &[&Node],
    limit: usize,
) -> Vec<&'a Node> {
    let cluster_tags = cluster
        .iter()
        .flat_map(|node| node.tags.iter().map(String::as_str))
        .collect::<HashSet<_>>();

    let mut ranked = preferences
        .iter()
        .copied()
        .map(|preference| {
            let overlap = preference
                .tags
                .iter()
                .filter(|tag| cluster_tags.contains(tag.as_str()))
                .count() as f32;
            let score = preference.importance * 0.7 + overlap * 0.3;
            (preference, score)
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
    ranked
        .into_iter()
        .take(limit.max(1))
        .map(|(preference, _)| preference)
        .collect()
}

fn select_supporting_lessons(
    lessons: &[Lesson],
    cluster: &[&Node],
    max_lessons: usize,
) -> Vec<Lesson> {
    let basis_ids = cluster.iter().map(|node| node.id).collect::<HashSet<_>>();
    let mut ranked = lessons
        .iter()
        .cloned()
        .map(|lesson| {
            let overlap = lesson
                .supporting_node_ids
                .iter()
                .filter(|node_id| basis_ids.contains(node_id))
                .count() as f32;
            let score = overlap * 0.6
                + lesson.confidence.clamp(0.0, 1.0) * 0.3
                + (lesson.reinforcement_count as f32 / 10.0).clamp(0.0, 1.0) * 0.1;
            (lesson, score)
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
    ranked
        .into_iter()
        .take(max_lessons.max(1))
        .map(|(lesson, _)| lesson)
        .collect()
}

fn build_predicted_outcomes(
    kind: ImaginedScenarioKind,
    planning_task: &str,
    cluster: &[&Node],
    goal: Option<&Node>,
    preferences: &[&Node],
    lessons: &[Lesson],
    trait_snapshot: &[TraitState],
    self_model_snapshot: Option<&SelfModel>,
) -> Vec<String> {
    let goal_title = goal
        .map(|node| node.title.clone())
        .unwrap_or_else(|| "the current plan".to_owned());
    let lesson_hint = lessons
        .first()
        .map(|lesson| lesson.statement.clone())
        .unwrap_or_else(|| "the plan stays anchored in existing memory links".to_owned());
    let strongest_trait = trait_snapshot
        .iter()
        .max_by(|left, right| left.strength.total_cmp(&right.strength))
        .map(|trait_state| trait_state.label.clone())
        .unwrap_or_else(|| "Balanced judgment".to_owned());
    let cluster_focus = cluster
        .first()
        .map(|node| node.title.clone())
        .unwrap_or_else(|| "the current memory cluster".to_owned());
    let preference_hint = preferences
        .first()
        .map(|node| node.summary.clone())
        .unwrap_or_else(|| "no strong preference constraints are available".to_owned());
    let self_model_hint = self_model_snapshot
        .and_then(|snapshot| snapshot.behavioral_tendencies.first().cloned())
        .unwrap_or_else(|| "no stable self-model tendency is available".to_owned());

    match kind {
        ImaginedScenarioKind::FutureNeedPrediction => vec![
            format!(
                "The next likely need for '{planning_task}' could be explicit support material around {cluster_focus}."
            ),
            format!("That need is more plausible if {lesson_hint}."),
            format!("The prediction reflects the current {strongest_trait} trait and {self_model_hint}."),
        ],
        ImaginedScenarioKind::AlternativePlan => vec![
            format!("An alternative plan could advance {goal_title} by reusing patterns from {cluster_focus}."),
            format!("The alternative stays useful if it respects this preference: {preference_hint}."),
            format!("Execution would likely lean on {strongest_trait} and {self_model_hint}."),
        ],
        ImaginedScenarioKind::Counterfactual => vec![
            format!("If the cluster around {cluster_focus} had been absent, {goal_title} would likely need a slower path."),
            format!("The counterfactual is grounded by the lesson that {lesson_hint}."),
            format!("The simulated difference is bounded by {strongest_trait} and {self_model_hint}."),
        ],
    }
}

fn summarize_traits(trait_snapshot: &[TraitState]) -> String {
    let mut traits = trait_snapshot
        .iter()
        .map(|trait_state| format!("{}:{:.2}", trait_state.label, trait_state.strength))
        .collect::<Vec<_>>();
    traits.sort();

    if traits.is_empty() {
        "no strong trait evidence".to_owned()
    } else {
        traits.join(", ")
    }
}

fn summarize_self_model(self_model_snapshot: Option<&SelfModel>) -> String {
    self_model_snapshot
        .map(|snapshot| {
            snapshot
                .behavioral_tendencies
                .iter()
                .chain(snapshot.user_interaction_preferences.iter())
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        })
        .filter(|summary| !summary.is_empty())
        .unwrap_or_else(|| "no stable self-model evidence".to_owned())
}

fn summarize_nodes(nodes: &[&Node]) -> String {
    if nodes.is_empty() {
        "no node basis".to_owned()
    } else {
        nodes
            .iter()
            .map(|node| node.title.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn summarize_lessons(lessons: &[Lesson]) -> String {
    if lessons.is_empty() {
        "without strong lesson support".to_owned()
    } else {
        lessons
            .iter()
            .map(|lesson| lesson.title.clone())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn scenario_title(kind: ImaginedScenarioKind, scenario_index: usize, goal_title: &str) -> String {
    match kind {
        ImaginedScenarioKind::FutureNeedPrediction => {
            format!(
                "Future-need simulation {} for {}",
                scenario_index + 1,
                goal_title
            )
        }
        ImaginedScenarioKind::AlternativePlan => {
            format!(
                "Alternative-plan simulation {} for {}",
                scenario_index + 1,
                goal_title
            )
        }
        ImaginedScenarioKind::Counterfactual => {
            format!(
                "Counterfactual simulation {} for {}",
                scenario_index + 1,
                goal_title
            )
        }
    }
}

fn scenario_premise(
    kind: ImaginedScenarioKind,
    planning_task: &str,
    cluster_summary: &str,
    goal_title: &str,
) -> String {
    match kind {
        ImaginedScenarioKind::FutureNeedPrediction => format!(
            "If the planning task '{planning_task}' continues from the connected cluster [{cluster_summary}] while pursuing {goal_title}, the next likely need can be simulated."
        ),
        ImaginedScenarioKind::AlternativePlan => format!(
            "If the planning task '{planning_task}' reuses the connected cluster [{cluster_summary}] but changes execution order, an alternative plan can be simulated for {goal_title}."
        ),
        ImaginedScenarioKind::Counterfactual => format!(
            "If the planning task '{planning_task}' had to proceed without some of the connected cluster [{cluster_summary}], a counterfactual path can be simulated for {goal_title}."
        ),
    }
}

fn scenario_narrative(
    kind: ImaginedScenarioKind,
    cluster_summary: &str,
    lesson_summary: &str,
    preference_summary: &str,
    trait_summary: &str,
    self_model_summary: &str,
) -> String {
    let frame = match kind {
        ImaginedScenarioKind::FutureNeedPrediction => "This predicts a future need",
        ImaginedScenarioKind::AlternativePlan => "This suggests an alternative plan",
        ImaginedScenarioKind::Counterfactual => "This explores a counterfactual path",
    };

    format!(
        "{frame} grounded in verified nodes [{cluster_summary}], validated lessons [{lesson_summary}], relevant preferences [{preference_summary}], the current trait snapshot ({trait_summary}), and the latest self-model ({self_model_summary}). This is a simulated hypothetical, not a verified memory or established fact."
    )
}

fn cluster_rank(cluster: &[&Node]) -> f32 {
    if cluster.is_empty() {
        return 0.0;
    }

    let total = cluster
        .iter()
        .map(|node| node.importance.clamp(0.0, 1.0) * 0.6 + node.confidence.clamp(0.0, 1.0) * 0.4)
        .sum::<f32>();
    total / cluster.len() as f32
}

fn plausibility_score(
    cluster: &[&Node],
    goal: Option<&Node>,
    lessons: &[Lesson],
    trait_snapshot: &[TraitState],
    self_model_snapshot: Option<&SelfModel>,
) -> f32 {
    let cluster_confidence = average(cluster.iter().map(|node| node.confidence));
    let lesson_confidence = average(lessons.iter().map(|lesson| lesson.confidence));
    let goal_confidence = goal.map_or(0.5, |node| node.confidence.clamp(0.0, 1.0));
    let evidence_trait = trait_strength(
        trait_snapshot,
        &[
            TraitType::EvidenceReliance,
            TraitType::Reliability,
            TraitType::Caution,
        ],
    );
    let self_model_bias = self_model_snapshot.map_or(0.5, |snapshot| {
        (snapshot.recurring_strengths.len().min(3) as f32 / 3.0 * 0.5
            + snapshot.behavioral_tendencies.len().min(3) as f32 / 3.0 * 0.5)
            .clamp(0.0, 1.0)
    });

    (cluster_confidence * 0.32
        + lesson_confidence * 0.22
        + goal_confidence * 0.14
        + evidence_trait * 0.18
        + self_model_bias * 0.14)
        .clamp(0.0, 1.0)
}

fn novelty_score(
    cluster: &[&Node],
    lessons: &[Lesson],
    trait_snapshot: &[TraitState],
    self_model_snapshot: Option<&SelfModel>,
    kind: ImaginedScenarioKind,
) -> f32 {
    let unique_tags = cluster
        .iter()
        .flat_map(|node| node.tags.iter().cloned())
        .collect::<HashSet<_>>()
        .len() as f32;
    let type_diversity = cluster
        .iter()
        .map(|node| node.node_type as u8)
        .collect::<HashSet<_>>()
        .len() as f32;
    let lesson_diversity = lessons
        .iter()
        .map(|lesson| lesson.lesson_type as u8)
        .collect::<HashSet<_>>()
        .len() as f32;
    let novelty_trait = trait_strength(
        trait_snapshot,
        &[TraitType::NoveltySeeking, TraitType::Curiosity],
    );
    let self_model_novelty = self_model_snapshot.map_or(0.5, |snapshot| {
        let novelty_mentions = snapshot
            .behavioral_tendencies
            .iter()
            .chain(snapshot.recurring_strengths.iter())
            .filter(|entry| {
                let text = entry.to_lowercase();
                text.contains("novel") || text.contains("explore") || text.contains("creative")
            })
            .count() as f32;
        (novelty_mentions / 2.0).clamp(0.0, 1.0)
    });
    let kind_bias = match kind {
        ImaginedScenarioKind::FutureNeedPrediction => 0.55,
        ImaginedScenarioKind::AlternativePlan => 0.72,
        ImaginedScenarioKind::Counterfactual => 0.8,
    };

    ((unique_tags / 6.0).clamp(0.0, 1.0) * 0.28
        + (type_diversity / 4.0).clamp(0.0, 1.0) * 0.18
        + (lesson_diversity / 3.0).clamp(0.0, 1.0) * 0.12
        + novelty_trait * 0.22
        + self_model_novelty * 0.1
        + kind_bias * 0.1)
        .clamp(0.0, 1.0)
}

fn usefulness_score(
    goal: Option<&Node>,
    preferences: &[&Node],
    lessons: &[Lesson],
    trait_snapshot: &[TraitState],
    self_model_snapshot: Option<&SelfModel>,
    kind: ImaginedScenarioKind,
) -> f32 {
    let goal_importance = goal.map_or(0.5, |node| node.importance.clamp(0.0, 1.0));
    let lesson_confidence = average(lessons.iter().map(|lesson| lesson.confidence));
    let preference_alignment = average(preferences.iter().map(|node| node.importance));
    let practical_trait = trait_strength(
        trait_snapshot,
        &[
            TraitType::Practicality,
            TraitType::Proactivity,
            TraitType::EvidenceReliance,
        ],
    );
    let self_model_alignment = self_model_snapshot.map_or(0.5, |snapshot| {
        (snapshot.user_interaction_preferences.len().min(3) as f32 / 3.0 * 0.5
            + snapshot.behavioral_tendencies.len().min(3) as f32 / 3.0 * 0.5)
            .clamp(0.0, 1.0)
    });
    let kind_bias = match kind {
        ImaginedScenarioKind::FutureNeedPrediction => 0.65,
        ImaginedScenarioKind::AlternativePlan => 0.75,
        ImaginedScenarioKind::Counterfactual => 0.58,
    };

    (goal_importance * 0.28
        + lesson_confidence * 0.18
        + preference_alignment * 0.12
        + practical_trait * 0.22
        + self_model_alignment * 0.1
        + kind_bias * 0.1)
        .clamp(0.0, 1.0)
}

fn trait_strength(trait_snapshot: &[TraitState], trait_types: &[TraitType]) -> f32 {
    let matching = trait_snapshot
        .iter()
        .filter(|trait_state| trait_types.contains(&trait_state.trait_type))
        .map(|trait_state| trait_state.strength.clamp(0.0, 1.0))
        .collect::<Vec<_>>();

    if matching.is_empty() {
        0.5
    } else {
        average(matching.into_iter())
    }
}

fn average<I>(values: I) -> f32
where
    I: IntoIterator<Item = f32>,
{
    let collected = values.into_iter().collect::<Vec<_>>();
    if collected.is_empty() {
        0.5
    } else {
        collected.iter().sum::<f32>() / collected.len() as f32
    }
}

/// Marker preserved for lightweight crate wiring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImaginationMarker {
    pub core: CoreMarker,
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use memory_core::{
        Edge, EdgeId, EdgeType, ImaginedScenarioKind, Lesson, LessonId, LessonType, MemoryPacket,
        MemoryPacketId, MemoryStatus, Node, NodeId, NodeType, SelfModel, SelfModelId, TraitId,
        TraitState, TraitType,
    };
    use uuid::Uuid;

    use super::{
        ImaginationService, PlanningImaginationApi, PlanningImaginationRequest,
        ScenarioReviewDecision,
    };

    #[test]
    fn generates_grounded_hypothetical_scenarios_for_planning() {
        let service = ImaginationService::default();
        let goal = sample_node("Ship planner", NodeType::Goal, 0.92, 0.88);
        let basis_a = sample_node("Memory clustering", NodeType::Semantic, 0.88, 0.7);
        let basis_b = sample_node("Lesson extraction", NodeType::Episodic, 0.83, 0.76);
        let preference = sample_node("User prefers concise plans", NodeType::Preference, 0.9, 0.8);
        let packet = sample_packet(
            vec![
                goal.clone(),
                basis_a.clone(),
                basis_b.clone(),
                preference.clone(),
            ],
            vec![Edge {
                id: EdgeId(Uuid::new_v4()),
                edge_type: EdgeType::RelatedTo,
                from_node_id: basis_a.id,
                to_node_id: basis_b.id,
                weight: 0.8,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            vec![Lesson {
                id: LessonId(Uuid::new_v4()),
                lesson_type: LessonType::Strategy,
                status: MemoryStatus::Active,
                title: "Reuse linked context".to_owned(),
                statement: "Linked memories improve planning consistency.".to_owned(),
                confidence: 0.84,
                evidence_count: 2,
                reinforcement_count: 2,
                supporting_node_ids: vec![basis_a.id, basis_b.id],
                contradicting_node_ids: Vec::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            vec![sample_trait(TraitType::Practicality, "Practicality", 0.81)],
            Some(sample_self_model()),
        );

        let response = service
            .imagine_for_planning(&PlanningImaginationRequest {
                planning_task: "Plan the next release".to_owned(),
                desired_scenarios: 3,
                context_packet: packet,
                active_goal_node_ids: vec![goal.id],
            })
            .expect("scenario generation should work");

        assert_eq!(response.scenarios.len(), 3);
        assert_eq!(
            response.scenarios[0].status,
            memory_core::ImaginationStatus::Simulated
        );
        assert!(!response.scenarios[0].basis_source_node_ids.is_empty());
        assert_eq!(response.scenarios[0].active_goal_node_ids, vec![goal.id]);
        assert!(!response.scenarios[0].trait_snapshot.is_empty());
        assert!(response.scenarios[0].self_model_snapshot.is_some());
        assert!(response.scenarios[0].plausibility_score > 0.0);
        assert!(response.scenarios[0]
            .narrative
            .contains("simulated hypothetical"));
        assert_eq!(
            response.scenarios[0].kind,
            ImaginedScenarioKind::FutureNeedPrediction
        );
        assert_eq!(
            response.scenarios[1].kind,
            ImaginedScenarioKind::AlternativePlan
        );
        assert_eq!(
            response.scenarios[2].kind,
            ImaginedScenarioKind::Counterfactual
        );
    }

    #[test]
    fn imagined_scenarios_do_not_become_verified_memory_nodes() {
        let service = ImaginationService::default();
        let verified_node = sample_node("Verified memory", NodeType::Semantic, 0.9, 0.7);
        let packet = sample_packet(
            vec![verified_node.clone()],
            Vec::new(),
            Vec::new(),
            vec![sample_trait(
                TraitType::EvidenceReliance,
                "Evidence Reliance",
                0.9,
            )],
            Some(sample_self_model()),
        );

        let response = service
            .imagine_for_planning(&PlanningImaginationRequest {
                planning_task: "Test the truth boundary".to_owned(),
                desired_scenarios: 1,
                context_packet: packet.clone(),
                active_goal_node_ids: Vec::new(),
            })
            .expect("scenario generation should work");

        assert_eq!(packet.nodes.len(), 1);
        assert!(packet
            .nodes
            .iter()
            .all(|node| node.node_type != NodeType::Imagined));
        assert!(response
            .scenarios
            .iter()
            .all(|scenario| packet.nodes.iter().all(|node| node.id.0 != scenario.id.0)));
        assert!(response
            .scenarios
            .iter()
            .all(|scenario| scenario.status == memory_core::ImaginationStatus::Simulated));
        assert!(packet.imagined_scenarios.is_empty());
    }

    #[test]
    fn scenarios_are_influenced_by_traits_and_self_model() {
        let service = ImaginationService::default();
        let basis = sample_node("Prototype exploration", NodeType::Semantic, 0.85, 0.7);
        let packet = sample_packet(
            vec![basis],
            Vec::new(),
            vec![Lesson {
                id: LessonId(Uuid::new_v4()),
                lesson_type: LessonType::Personality,
                status: MemoryStatus::Active,
                title: "Explore novel options".to_owned(),
                statement: "Explore new options when they can unlock better plans.".to_owned(),
                confidence: 0.82,
                evidence_count: 2,
                reinforcement_count: 2,
                supporting_node_ids: Vec::new(),
                contradicting_node_ids: Vec::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }],
            vec![
                sample_trait(TraitType::NoveltySeeking, "Novelty Seeking", 0.9),
                sample_trait(TraitType::Curiosity, "Curiosity", 0.85),
            ],
            Some(SelfModel {
                id: SelfModelId(Uuid::new_v4()),
                version: 2,
                recurring_strengths: vec!["Looks for novel approaches when they pay off".to_owned()],
                user_interaction_preferences: vec![
                    "User prefers evidence-backed answers".to_owned()
                ],
                behavioral_tendencies: vec!["Explores novel options selectively".to_owned()],
                active_domains: vec!["Planning".to_owned()],
                supporting_lesson_ids: Vec::new(),
                supporting_trait_ids: Vec::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }),
        );

        let response = service
            .imagine_for_planning(&PlanningImaginationRequest {
                planning_task: "Find a better rollout approach".to_owned(),
                desired_scenarios: 2,
                context_packet: packet,
                active_goal_node_ids: Vec::new(),
            })
            .expect("scenario generation should work");

        assert!(response
            .scenarios
            .iter()
            .any(|scenario| scenario.novelty_score > 0.5));
        assert!(response.scenarios.iter().any(|scenario| scenario
            .predicted_outcomes
            .join(" ")
            .contains("Explores novel options")));
        assert!(response
            .scenarios
            .iter()
            .any(|scenario| scenario.self_model_snapshot.is_some()));
    }

    #[test]
    fn imagined_content_is_not_promoted_to_facts_without_validation() {
        let service = ImaginationService::default();
        let packet = sample_packet(
            vec![sample_node(
                "Release note draft",
                NodeType::Semantic,
                0.8,
                0.8,
            )],
            Vec::new(),
            Vec::new(),
            vec![sample_trait(TraitType::Practicality, "Practicality", 0.8)],
            Some(sample_self_model()),
        );

        let response = service
            .imagine_for_planning(&PlanningImaginationRequest {
                planning_task: "Simulate a better release workflow".to_owned(),
                desired_scenarios: 1,
                context_packet: packet.clone(),
                active_goal_node_ids: Vec::new(),
            })
            .expect("scenario generation should work");
        let reviewed = service.review_scenario(
            &response.scenarios[0],
            ScenarioReviewDecision::AcceptAsHypothesis,
        );

        assert_eq!(
            reviewed.status,
            memory_core::ImaginationStatus::AcceptedAsHypothesis
        );
        assert!(packet
            .nodes
            .iter()
            .all(|node| node.node_type != NodeType::Imagined));
        assert!(packet.imagined_scenarios.is_empty());
        assert!(reviewed.narrative.contains("not a verified memory"));
    }

    fn sample_packet(
        nodes: Vec<Node>,
        edges: Vec<Edge>,
        lessons: Vec<Lesson>,
        traits: Vec<TraitState>,
        self_model_snapshot: Option<SelfModel>,
    ) -> MemoryPacket {
        MemoryPacket {
            id: MemoryPacketId(Uuid::new_v4()),
            request_id: Some("req-1".to_owned()),
            created_at: Utc::now(),
            nodes,
            edges,
            lessons,
            traits,
            self_model_snapshot,
            checkpoints: Vec::new(),
            imagined_scenarios: Vec::new(),
        }
    }

    fn sample_node(title: &str, node_type: NodeType, confidence: f32, importance: f32) -> Node {
        Node {
            id: NodeId(Uuid::new_v4()),
            node_type,
            status: MemoryStatus::Active,
            title: title.to_owned(),
            summary: format!("{title} summary"),
            content: Some(format!("{title} content")),
            tags: vec!["planning".to_owned(), "memory".to_owned()],
            confidence,
            importance,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            source_event_id: Some("event-1".to_owned()),
        }
    }

    fn sample_trait(trait_type: TraitType, label: &str, strength: f32) -> TraitState {
        TraitState {
            id: TraitId(Uuid::new_v4()),
            trait_type,
            status: MemoryStatus::Active,
            label: label.to_owned(),
            description: format!("{label} description"),
            strength,
            confidence: 0.7,
            supporting_lesson_ids: Vec::new(),
            supporting_node_ids: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn sample_self_model() -> SelfModel {
        SelfModel {
            id: SelfModelId(Uuid::new_v4()),
            version: 1,
            recurring_strengths: vec!["Practical and outcome-focused".to_owned()],
            user_interaction_preferences: vec!["User prefers concise responses".to_owned()],
            behavioral_tendencies: vec!["Offers next steps without waiting to be asked".to_owned()],
            active_domains: vec!["Release".to_owned(), "Planning".to_owned()],
            supporting_lesson_ids: Vec::new(),
            supporting_trait_ids: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
