//! Slow-moving personality and outcome-learning subsystem for Nodamem.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use chrono::Utc;
use memory_core::{
    CoreMarker, Lesson, LessonId, LessonType, MemoryStatus, NodeId, SelfModel, SelfModelId,
    TraitChangeKind, TraitEvent, TraitEventId, TraitId, TraitState, TraitType,
};
use tracing::{debug, info};
use uuid::Uuid;

/// Validated agent outcome used to update personality traits gradually over time.
#[derive(Debug, Clone, PartialEq)]
pub struct OutcomeRecord {
    pub outcome_id: String,
    pub subject_node_id: Option<NodeId>,
    pub success: bool,
    pub usefulness: f32,
    pub prediction_correct: bool,
    pub user_accepted: bool,
    pub validated: bool,
}

/// Summary of a trait update from a single outcome application.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitUpdate {
    pub trait_id: TraitId,
    pub trait_type: TraitType,
    pub change_kind: TraitChangeKind,
    pub previous_strength: f32,
    pub updated_strength: f32,
    pub delta: f32,
    pub reason: String,
    pub outcome_id: String,
    pub subject_node_id: Option<NodeId>,
    pub supporting_lesson_ids: Vec<LessonId>,
}

impl TraitUpdate {
    #[must_use]
    pub fn to_event(&self, event_id: TraitEventId) -> TraitEvent {
        TraitEvent {
            id: event_id,
            trait_id: self.trait_id,
            trait_type: self.trait_type,
            change_kind: self.change_kind,
            delta: self.delta,
            previous_strength: self.previous_strength,
            updated_strength: self.updated_strength,
            reason: self.reason.clone(),
            outcome_id: Some(self.outcome_id.clone()),
            lesson_id: self.supporting_lesson_ids.first().copied(),
            node_id: self.subject_node_id,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Personality profile snapshot exposed to other layers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PersonalityProfile {
    pub traits: Vec<TraitState>,
}

/// Deterministic outcome-learning behavior configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct PersonalityPolicy {
    pub learning_rate: f32,
    pub max_step_per_outcome: f32,
    pub confidence_increment: f32,
    pub minimum_start_strength: f32,
    pub lesson_signal_weight: f32,
    pub minimum_lesson_confidence: f32,
    pub minimum_lesson_evidence: u32,
    pub self_model_trait_threshold: f32,
    pub self_model_trait_confidence: f32,
    pub max_self_model_items: usize,
}

impl Default for PersonalityPolicy {
    fn default() -> Self {
        Self {
            learning_rate: 0.08,
            max_step_per_outcome: 0.05,
            confidence_increment: 0.02,
            minimum_start_strength: 0.5,
            lesson_signal_weight: 0.18,
            minimum_lesson_confidence: 0.72,
            minimum_lesson_evidence: 3,
            self_model_trait_threshold: 0.62,
            self_model_trait_confidence: 0.45,
            max_self_model_items: 4,
        }
    }
}

/// Output from applying a validated outcome to the personality subsystem.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PersonalityUpdateResult {
    pub updated_traits: Vec<TraitState>,
    pub updates: Vec<TraitUpdate>,
    pub refreshed_self_model: Option<SelfModel>,
}

/// Trait-oriented outcome learner. Agents provide validated outcomes; they do not mutate traits directly.
#[derive(Debug, Clone)]
pub struct PersonalityService {
    pub policy: PersonalityPolicy,
}

impl PersonalityService {
    #[must_use]
    pub fn new(policy: PersonalityPolicy) -> Self {
        Self { policy }
    }

    #[must_use]
    pub fn profile(&self, traits: &[TraitState]) -> PersonalityProfile {
        PersonalityProfile {
            traits: traits.to_vec(),
        }
    }

    #[must_use]
    pub fn record_outcome(
        &self,
        traits: &[TraitState],
        lessons: &[Lesson],
        existing_self_model: Option<&SelfModel>,
        outcome: &OutcomeRecord,
    ) -> PersonalityUpdateResult {
        if !outcome.validated {
            debug!(outcome_id = %outcome.outcome_id, "skipping trait update for unvalidated outcome");
            return PersonalityUpdateResult {
                updated_traits: traits.to_vec(),
                updates: Vec::new(),
                refreshed_self_model: existing_self_model.cloned(),
            };
        }

        let stable_lessons = stable_lessons(lessons, &self.policy);
        let lesson_signals = lesson_trait_signals(&stable_lessons);
        let mut updated_traits = traits.to_vec();
        let mut updates = Vec::new();

        for (trait_type, outcome_signal) in trait_signals(outcome) {
            let lesson_signal = lesson_signals.get(&trait_type).map_or(0.0, |signal| {
                signal.signal * self.policy.lesson_signal_weight
            });
            let combined_signal = (outcome_signal + lesson_signal).clamp(-1.0, 1.0);
            let existing_index = updated_traits
                .iter()
                .position(|trait_state| trait_state.trait_type == trait_type);

            let trait_state = existing_index
                .map(|index| updated_traits[index].clone())
                .unwrap_or_else(|| {
                    default_trait_state(trait_type, self.policy.minimum_start_strength)
                });

            let supporting_lessons = lesson_signals
                .get(&trait_type)
                .map_or_else(Vec::new, |signal| signal.lesson_ids.clone());
            let updated = self.apply_signal(
                &trait_state,
                combined_signal,
                &supporting_lessons,
                outcome.subject_node_id,
            );
            let delta = updated.strength - trait_state.strength;
            let change_kind = classify_change(delta);
            let reason = update_reason(outcome, trait_type, lesson_signal, &supporting_lessons);

            let update = TraitUpdate {
                trait_id: updated.id,
                trait_type,
                change_kind,
                previous_strength: trait_state.strength,
                updated_strength: updated.strength,
                delta,
                reason,
                outcome_id: outcome.outcome_id.clone(),
                subject_node_id: outcome.subject_node_id,
                supporting_lesson_ids: supporting_lessons.clone(),
            };

            match change_kind {
                TraitChangeKind::Reinforced => info!(
                    outcome_id = %outcome.outcome_id,
                    trait_type = ?trait_type,
                    previous_strength = trait_state.strength,
                    updated_strength = updated.strength,
                    lesson_support_count = supporting_lessons.len(),
                    "trait reinforcement recorded"
                ),
                TraitChangeKind::Weakened => info!(
                    outcome_id = %outcome.outcome_id,
                    trait_type = ?trait_type,
                    previous_strength = trait_state.strength,
                    updated_strength = updated.strength,
                    lesson_support_count = supporting_lessons.len(),
                    "trait weakening recorded"
                ),
                TraitChangeKind::Stable => debug!(
                    outcome_id = %outcome.outcome_id,
                    trait_type = ?trait_type,
                    updated_strength = updated.strength,
                    "trait remained stable after bounded update"
                ),
            }

            if let Some(index) = existing_index {
                updated_traits[index] = updated;
            } else {
                updated_traits.push(updated);
            }
            updates.push(update);
        }

        let refreshed_self_model =
            self.refresh_self_model(existing_self_model, &updated_traits, lessons);

        PersonalityUpdateResult {
            updated_traits,
            updates,
            refreshed_self_model,
        }
    }

    #[must_use]
    pub fn refresh_self_model(
        &self,
        existing_self_model: Option<&SelfModel>,
        traits: &[TraitState],
        lessons: &[Lesson],
    ) -> Option<SelfModel> {
        let stable_traits = stable_traits(traits, &self.policy);
        let stable_lessons = stable_lessons(lessons, &self.policy);
        if stable_traits.is_empty() && stable_lessons.is_empty() {
            return existing_self_model.cloned();
        }

        let recurring_strengths = summarize_recurring_strengths(&stable_traits, &self.policy);
        let user_interaction_preferences =
            summarize_user_preferences(&stable_lessons, &self.policy);
        let behavioral_tendencies =
            summarize_behavioral_tendencies(&stable_traits, &stable_lessons, &self.policy);
        let active_domains = summarize_active_domains(&stable_lessons, &self.policy);
        let supporting_lesson_ids = stable_lessons
            .iter()
            .map(|lesson| lesson.id)
            .collect::<Vec<_>>();
        let supporting_trait_ids = stable_traits
            .iter()
            .map(|trait_state| trait_state.id)
            .collect::<Vec<_>>();

        let candidate = SelfModel {
            id: SelfModelId(Uuid::new_v4()),
            version: existing_self_model.map_or(1, |model| model.version.saturating_add(1)),
            recurring_strengths,
            user_interaction_preferences,
            behavioral_tendencies,
            active_domains,
            supporting_lesson_ids,
            supporting_trait_ids,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        if existing_self_model.is_some_and(|model| same_self_model_content(model, &candidate)) {
            return existing_self_model.cloned();
        }

        info!(
            version = candidate.version,
            recurring_strengths = candidate.recurring_strengths.len(),
            interaction_preferences = candidate.user_interaction_preferences.len(),
            tendencies = candidate.behavioral_tendencies.len(),
            active_domains = candidate.active_domains.len(),
            "self-model refresh updated"
        );

        Some(candidate)
    }

    fn apply_signal(
        &self,
        trait_state: &TraitState,
        signal: f32,
        supporting_lesson_ids: &[LessonId],
        subject_node_id: Option<NodeId>,
    ) -> TraitState {
        let bounded_signal = signal.clamp(-1.0, 1.0);
        let delta = (bounded_signal * self.policy.learning_rate).clamp(
            -self.policy.max_step_per_outcome,
            self.policy.max_step_per_outcome,
        );
        let now = Utc::now();

        TraitState {
            id: trait_state.id,
            trait_type: trait_state.trait_type,
            status: MemoryStatus::Active,
            label: trait_state.label.clone(),
            description: trait_state.description.clone(),
            strength: (trait_state.strength + delta).clamp(0.0, 1.0),
            confidence: (trait_state.confidence + self.policy.confidence_increment).clamp(0.0, 1.0),
            supporting_lesson_ids: merge_lesson_ids(
                &trait_state.supporting_lesson_ids,
                supporting_lesson_ids,
            ),
            supporting_node_ids: merge_node_ids(
                &trait_state.supporting_node_ids,
                &subject_node_id.into_iter().collect::<Vec<_>>(),
            ),
            created_at: trait_state.created_at,
            updated_at: now,
        }
    }
}

impl Default for PersonalityService {
    fn default() -> Self {
        Self::new(PersonalityPolicy::default())
    }
}

#[derive(Debug, Clone)]
struct LessonSignal {
    signal: f32,
    lesson_ids: Vec<LessonId>,
}

fn trait_signals(outcome: &OutcomeRecord) -> Vec<(TraitType, f32)> {
    let success_signal = if outcome.success { 1.0 } else { -1.0 };
    let usefulness_signal = (outcome.usefulness * 2.0 - 1.0).clamp(-1.0, 1.0);
    let acceptance_signal = if outcome.user_accepted { 1.0 } else { -1.0 };
    let prediction_signal = if outcome.prediction_correct {
        1.0
    } else {
        -1.0
    };

    vec![
        (
            TraitType::Practicality,
            usefulness_signal * 0.7 + success_signal * 0.3,
        ),
        (
            TraitType::EvidenceReliance,
            prediction_signal * 0.6 + usefulness_signal * 0.4,
        ),
        (
            TraitType::Proactivity,
            success_signal * 0.7 + acceptance_signal * 0.3,
        ),
        (
            TraitType::Caution,
            if outcome.success {
                -0.2
            } else if outcome.prediction_correct {
                0.4
            } else {
                0.8
            },
        ),
        (
            TraitType::Curiosity,
            if outcome.usefulness > 0.6 && outcome.success {
                0.4
            } else if !outcome.success && !outcome.user_accepted {
                -0.2
            } else {
                0.1
            },
        ),
        (
            TraitType::NoveltySeeking,
            if outcome.success && outcome.usefulness > 0.7 {
                0.3
            } else if !outcome.success {
                -0.2
            } else {
                0.0
            },
        ),
        (
            TraitType::Verbosity,
            if outcome.user_accepted && outcome.usefulness > 0.6 {
                0.1
            } else if !outcome.user_accepted {
                -0.2
            } else {
                0.0
            },
        ),
        (
            TraitType::Reliability,
            success_signal * 0.5 + prediction_signal * 0.5,
        ),
    ]
}

fn stable_lessons<'a>(lessons: &'a [Lesson], policy: &PersonalityPolicy) -> Vec<&'a Lesson> {
    lessons
        .iter()
        .filter(|lesson| {
            matches!(
                lesson.status,
                MemoryStatus::Active | MemoryStatus::Reinforced
            ) && lesson.confidence >= policy.minimum_lesson_confidence
                && lesson
                    .evidence_count
                    .saturating_add(lesson.reinforcement_count)
                    >= policy.minimum_lesson_evidence
        })
        .collect()
}

fn stable_traits<'a>(traits: &'a [TraitState], policy: &PersonalityPolicy) -> Vec<&'a TraitState> {
    traits
        .iter()
        .filter(|trait_state| {
            matches!(
                trait_state.status,
                MemoryStatus::Active | MemoryStatus::Reinforced
            ) && trait_state.strength >= policy.self_model_trait_threshold
                && trait_state.confidence >= policy.self_model_trait_confidence
        })
        .collect()
}

fn lesson_trait_signals(lessons: &[&Lesson]) -> HashMap<TraitType, LessonSignal> {
    let mut signals: HashMap<TraitType, LessonSignal> = HashMap::new();
    for lesson in lessons {
        for (trait_type, direction) in lesson_alignment(lesson) {
            let entry = signals.entry(trait_type).or_insert(LessonSignal {
                signal: 0.0,
                lesson_ids: Vec::new(),
            });
            entry.signal += direction * lesson_weight(lesson);
            if !entry.lesson_ids.contains(&lesson.id) {
                entry.lesson_ids.push(lesson.id);
            }
        }
    }

    for signal in signals.values_mut() {
        signal.signal = signal.signal.clamp(-1.0, 1.0);
    }
    signals
}

fn lesson_alignment(lesson: &Lesson) -> Vec<(TraitType, f32)> {
    let text = format!("{} {}", lesson.title, lesson.statement).to_lowercase();
    let mut alignments = Vec::new();

    if has_any(&text, &["practical", "workable", "useful", "actionable"]) {
        alignments.push((TraitType::Practicality, 1.0));
    }
    if has_any(
        &text,
        &[
            "curious",
            "explore",
            "follow-up",
            "ask questions",
            "investigate",
        ],
    ) {
        alignments.push((TraitType::Curiosity, 1.0));
    }
    if has_any(
        &text,
        &["careful", "verify", "double-check", "safe", "cautious"],
    ) {
        alignments.push((TraitType::Caution, 1.0));
        alignments.push((TraitType::EvidenceReliance, 0.8));
    }
    if has_any(&text, &["concise", "brief", "short answers"]) {
        alignments.push((TraitType::Verbosity, -1.0));
    }
    if has_any(&text, &["detailed", "comprehensive", "explain fully"]) {
        alignments.push((TraitType::Verbosity, 1.0));
    }
    if has_any(&text, &["novel", "creative", "new approach", "experiment"]) {
        alignments.push((TraitType::NoveltySeeking, 1.0));
    }
    if has_any(&text, &["evidence", "source", "validated", "grounded"]) {
        alignments.push((TraitType::EvidenceReliance, 1.0));
    }
    if has_any(
        &text,
        &["proactive", "anticipate", "next step", "take initiative"],
    ) {
        alignments.push((TraitType::Proactivity, 1.0));
    }
    if has_any(&text, &["reliable", "consistent", "dependable"]) {
        alignments.push((TraitType::Reliability, 1.0));
    }

    if lesson.lesson_type == LessonType::Personality && alignments.is_empty() {
        alignments.push((TraitType::Reliability, 0.4));
    }

    alignments
}

fn lesson_weight(lesson: &Lesson) -> f32 {
    let evidence = lesson
        .evidence_count
        .saturating_add(lesson.reinforcement_count)
        .min(8) as f32;
    (lesson.confidence.clamp(0.0, 1.0) * 0.6 + (evidence / 8.0) * 0.4).clamp(0.0, 1.0)
}

fn classify_change(delta: f32) -> TraitChangeKind {
    if delta > 0.001 {
        TraitChangeKind::Reinforced
    } else if delta < -0.001 {
        TraitChangeKind::Weakened
    } else {
        TraitChangeKind::Stable
    }
}

fn update_reason(
    outcome: &OutcomeRecord,
    trait_type: TraitType,
    lesson_signal: f32,
    supporting_lessons: &[LessonId],
) -> String {
    let outcome_basis = if outcome.success {
        "validated success"
    } else if outcome.prediction_correct {
        "validated failure with correct prediction"
    } else {
        "validated failure with prediction error"
    };
    let lesson_basis = if supporting_lessons.is_empty() {
        "without stable lesson support".to_owned()
    } else if lesson_signal >= 0.0 {
        format!("with {} supporting lessons", supporting_lessons.len())
    } else {
        format!("with {} counterbalancing lessons", supporting_lessons.len())
    };

    format!(
        "{outcome_basis} updated {trait_type:?} {lesson_basis}; user_accepted={}, usefulness={:.2}",
        outcome.user_accepted, outcome.usefulness
    )
}

fn summarize_recurring_strengths(
    traits: &[&TraitState],
    policy: &PersonalityPolicy,
) -> Vec<String> {
    top_traits(traits, policy)
        .into_iter()
        .map(|trait_state| match trait_state.trait_type {
            TraitType::Practicality => "Practical and outcome-focused".to_owned(),
            TraitType::EvidenceReliance => "Grounds decisions in validated evidence".to_owned(),
            TraitType::Proactivity => "Anticipates useful next steps".to_owned(),
            TraitType::Reliability => "Maintains consistent execution".to_owned(),
            TraitType::Caution => "Applies careful verification under uncertainty".to_owned(),
            TraitType::Curiosity => "Explores follow-up questions when useful".to_owned(),
            TraitType::NoveltySeeking => "Looks for novel approaches when they pay off".to_owned(),
            TraitType::Verbosity => {
                if trait_state.strength >= 0.72 {
                    "Provides fuller explanations when needed".to_owned()
                } else {
                    "Keeps explanations compact by default".to_owned()
                }
            }
        })
        .collect()
}

fn summarize_user_preferences(lessons: &[&Lesson], policy: &PersonalityPolicy) -> Vec<String> {
    let mut items = lessons
        .iter()
        .filter(|lesson| lesson.lesson_type == LessonType::User)
        .filter_map(|lesson| preference_summary(lesson))
        .collect::<Vec<_>>();
    items.sort();
    items.dedup();
    items.truncate(policy.max_self_model_items);
    items
}

fn preference_summary(lesson: &Lesson) -> Option<String> {
    let text = format!("{} {}", lesson.title, lesson.statement).to_lowercase();
    if has_any(&text, &["concise", "brief", "short"]) {
        Some("User prefers concise responses".to_owned())
    } else if has_any(&text, &["detailed", "thorough", "comprehensive"]) {
        Some("User prefers thorough responses when needed".to_owned())
    } else if has_any(&text, &["source", "evidence", "cite", "verify"]) {
        Some("User prefers evidence-backed answers".to_owned())
    } else if has_any(&text, &["step by step", "steps", "actionable"]) {
        Some("User prefers actionable stepwise guidance".to_owned())
    } else {
        None
    }
}

fn summarize_behavioral_tendencies(
    traits: &[&TraitState],
    lessons: &[&Lesson],
    policy: &PersonalityPolicy,
) -> Vec<String> {
    let mut tendencies = top_traits(traits, policy)
        .into_iter()
        .map(|trait_state| match trait_state.trait_type {
            TraitType::Practicality => "Bias toward workable answers".to_owned(),
            TraitType::Curiosity => "Asks follow-up questions when ambiguity matters".to_owned(),
            TraitType::Caution => "Slows down under risk or uncertainty".to_owned(),
            TraitType::Verbosity => {
                if trait_state.strength >= 0.72 {
                    "Tends toward fuller explanations".to_owned()
                } else {
                    "Tends toward concise explanations".to_owned()
                }
            }
            TraitType::NoveltySeeking => "Explores novel options selectively".to_owned(),
            TraitType::EvidenceReliance => "Prefers verified evidence over speculation".to_owned(),
            TraitType::Reliability => "Favors consistency and follow-through".to_owned(),
            TraitType::Proactivity => "Offers next steps without waiting to be asked".to_owned(),
        })
        .collect::<Vec<_>>();

    tendencies.extend(
        lessons
            .iter()
            .filter(|lesson| lesson.lesson_type == LessonType::Personality)
            .filter_map(|lesson| {
                let text = format!("{} {}", lesson.title, lesson.statement).to_lowercase();
                if has_any(&text, &["verify", "careful"]) {
                    Some("Stable tendency toward careful verification".to_owned())
                } else if has_any(&text, &["proactive", "initiative"]) {
                    Some("Stable tendency toward proactive support".to_owned())
                } else {
                    None
                }
            }),
    );
    tendencies.sort();
    tendencies.dedup();
    tendencies.truncate(policy.max_self_model_items);
    tendencies
}

fn summarize_active_domains(lessons: &[&Lesson], policy: &PersonalityPolicy) -> Vec<String> {
    let mut counts = HashMap::<String, usize>::new();
    for lesson in lessons {
        for token in topic_tokens(&lesson.title) {
            *counts.entry(token).or_default() += 2;
        }
        for token in topic_tokens(&lesson.statement) {
            *counts.entry(token).or_default() += 1;
        }
    }

    let mut domains = counts.into_iter().collect::<Vec<_>>();
    domains.sort_by_key(|(token, count)| (Reverse(*count), token.clone()));
    domains
        .into_iter()
        .map(|(token, _)| title_case(&token))
        .take(policy.max_self_model_items)
        .collect()
}

fn top_traits<'a>(traits: &[&'a TraitState], policy: &PersonalityPolicy) -> Vec<&'a TraitState> {
    let mut ranked = traits.to_vec();
    ranked.sort_by(|left, right| {
        right
            .strength
            .total_cmp(&left.strength)
            .then_with(|| right.confidence.total_cmp(&left.confidence))
            .then_with(|| left.label.cmp(&right.label))
    });
    ranked.truncate(policy.max_self_model_items);
    ranked
}

fn topic_tokens(text: &str) -> Vec<String> {
    let stopwords = [
        "about",
        "after",
        "agent",
        "answers",
        "behavior",
        "context",
        "details",
        "during",
        "evidence",
        "fewer",
        "future",
        "helps",
        "improves",
        "memory",
        "needs",
        "next",
        "response",
        "responses",
        "stable",
        "steps",
        "strategy",
        "system",
        "task",
        "tasks",
        "their",
        "them",
        "this",
        "under",
        "user",
        "when",
        "with",
        "work",
        "works",
    ];
    let stopwords = stopwords.into_iter().collect::<HashSet<_>>();

    text.to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|token| token.len() >= 5 && !stopwords.contains(*token))
        .map(str::to_owned)
        .collect()
}

fn title_case(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn has_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn merge_lesson_ids(existing: &[LessonId], extra: &[LessonId]) -> Vec<LessonId> {
    let mut merged = existing.to_vec();
    for lesson_id in extra {
        if !merged.contains(lesson_id) {
            merged.push(*lesson_id);
        }
    }
    merged
}

fn merge_node_ids(existing: &[NodeId], extra: &[NodeId]) -> Vec<NodeId> {
    let mut merged = existing.to_vec();
    for node_id in extra {
        if !merged.contains(node_id) {
            merged.push(*node_id);
        }
    }
    merged
}

fn same_self_model_content(left: &SelfModel, right: &SelfModel) -> bool {
    left.recurring_strengths == right.recurring_strengths
        && left.user_interaction_preferences == right.user_interaction_preferences
        && left.behavioral_tendencies == right.behavioral_tendencies
        && left.active_domains == right.active_domains
        && left.supporting_lesson_ids == right.supporting_lesson_ids
        && left.supporting_trait_ids == right.supporting_trait_ids
}

fn default_trait_state(trait_type: TraitType, starting_strength: f32) -> TraitState {
    let now = Utc::now();

    TraitState {
        id: TraitId(Uuid::new_v4()),
        trait_type,
        status: MemoryStatus::Active,
        label: trait_label(trait_type).to_owned(),
        description: trait_description(trait_type).to_owned(),
        strength: starting_strength,
        confidence: 0.3,
        supporting_lesson_ids: Vec::new(),
        supporting_node_ids: Vec::new(),
        created_at: now,
        updated_at: now,
    }
}

fn trait_label(trait_type: TraitType) -> &'static str {
    match trait_type {
        TraitType::Curiosity => "Curiosity",
        TraitType::Caution => "Caution",
        TraitType::Verbosity => "Verbosity",
        TraitType::NoveltySeeking => "Novelty Seeking",
        TraitType::EvidenceReliance => "Evidence Reliance",
        TraitType::Reliability => "Reliability",
        TraitType::Practicality => "Practicality",
        TraitType::Proactivity => "Proactivity",
    }
}

fn trait_description(trait_type: TraitType) -> &'static str {
    match trait_type {
        TraitType::Curiosity => "Tendency to explore and ask follow-up questions.",
        TraitType::Caution => "Tendency to hedge, verify, and avoid risky assumptions.",
        TraitType::Verbosity => "Tendency to provide more or less detailed responses.",
        TraitType::NoveltySeeking => "Tendency to prefer new ideas or familiar solutions.",
        TraitType::EvidenceReliance => "Tendency to anchor decisions in validated evidence.",
        TraitType::Reliability => "Tendency to favor consistency and dependable execution.",
        TraitType::Practicality => "Tendency to optimize for useful, workable outcomes.",
        TraitType::Proactivity => "Tendency to anticipate next steps without prompting.",
    }
}

/// Marker preserved for lightweight crate wiring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersonalityMarker {
    pub core: CoreMarker,
}

#[cfg(test)]
mod tests {
    use super::{OutcomeRecord, PersonalityService};
    use memory_core::{Lesson, LessonType, MemoryStatus, TraitChangeKind, TraitType};
    use uuid::Uuid;

    #[test]
    fn repeated_validated_success_reinforces_traits_gradually() {
        let service = PersonalityService::default();
        let mut traits = Vec::new();

        for index in 0..5 {
            let result = service.record_outcome(
                &traits,
                &[],
                None,
                &OutcomeRecord {
                    outcome_id: format!("outcome-{index}"),
                    subject_node_id: None,
                    success: true,
                    usefulness: 0.9,
                    prediction_correct: true,
                    user_accepted: true,
                    validated: true,
                },
            );
            assert!(!result.updates.is_empty());
            traits = result.updated_traits;
        }

        let practicality = traits
            .iter()
            .find(|trait_state| trait_state.trait_type == TraitType::Practicality)
            .expect("practicality trait should exist");
        let proactivity = traits
            .iter()
            .find(|trait_state| trait_state.trait_type == TraitType::Proactivity)
            .expect("proactivity trait should exist");

        assert!(practicality.strength > 0.5);
        assert!(practicality.strength < 1.0);
        assert!(proactivity.strength > 0.5);
    }

    #[test]
    fn invalidated_outcomes_do_not_mutate_traits() {
        let service = PersonalityService::default();
        let result = service.record_outcome(
            &[],
            &[],
            None,
            &OutcomeRecord {
                outcome_id: "outcome-invalid".to_owned(),
                subject_node_id: None,
                success: false,
                usefulness: 0.1,
                prediction_correct: false,
                user_accepted: false,
                validated: false,
            },
        );

        assert!(result.updated_traits.is_empty());
        assert!(result.updates.is_empty());
    }

    #[test]
    fn contradictory_outcomes_weaken_a_trait_trend() {
        let service = PersonalityService::default();
        let positive = service.record_outcome(
            &[],
            &[],
            None,
            &OutcomeRecord {
                outcome_id: "outcome-positive".to_owned(),
                subject_node_id: None,
                success: true,
                usefulness: 0.95,
                prediction_correct: true,
                user_accepted: true,
                validated: true,
            },
        );
        let after_negative = service.record_outcome(
            &positive.updated_traits,
            &[],
            None,
            &OutcomeRecord {
                outcome_id: "outcome-negative".to_owned(),
                subject_node_id: None,
                success: false,
                usefulness: 0.1,
                prediction_correct: false,
                user_accepted: false,
                validated: true,
            },
        );

        let practicality = after_negative
            .updated_traits
            .iter()
            .find(|trait_state| trait_state.trait_type == TraitType::Practicality)
            .expect("practicality trait should exist");
        let weakening = after_negative
            .updates
            .iter()
            .find(|update| update.trait_type == TraitType::Practicality)
            .expect("practicality update should exist");

        assert!(practicality.strength < 0.58);
        assert_eq!(weakening.change_kind, TraitChangeKind::Weakened);
    }

    #[test]
    fn self_model_refreshes_from_repeated_validated_lessons() {
        let service = PersonalityService::default();
        let lessons = vec![
            sample_lesson(
                LessonType::User,
                "Concise answers",
                "The user prefers concise, evidence-backed answers.",
            ),
            sample_lesson(
                LessonType::Personality,
                "Practical verification",
                "Be practical, verify risky claims, and prefer actionable next steps.",
            ),
            sample_lesson(
                LessonType::Domain,
                "Release engineering",
                "Release engineering and deployment planning are recurring domains.",
            ),
        ];
        let result = service.record_outcome(
            &[],
            &lessons,
            None,
            &OutcomeRecord {
                outcome_id: "outcome-self-model".to_owned(),
                subject_node_id: None,
                success: true,
                usefulness: 0.85,
                prediction_correct: true,
                user_accepted: true,
                validated: true,
            },
        );

        let self_model = result
            .refreshed_self_model
            .expect("self-model should refresh");
        assert!(self_model
            .user_interaction_preferences
            .iter()
            .any(|entry| entry.contains("concise")));
        assert!(self_model
            .behavioral_tendencies
            .iter()
            .any(|entry| entry.contains("verification") || entry.contains("workable")));
        assert!(self_model
            .active_domains
            .iter()
            .any(|entry| entry.contains("Release") || entry.contains("Engineering")));
    }

    #[test]
    fn trait_updates_preserve_provenance_for_audit() {
        let service = PersonalityService::default();
        let lessons = vec![sample_lesson(
            LessonType::Personality,
            "Use evidence",
            "Prefer validated evidence before answering.",
        )];
        let result = service.record_outcome(
            &[],
            &lessons,
            None,
            &OutcomeRecord {
                outcome_id: "outcome-audit".to_owned(),
                subject_node_id: Some(memory_core::NodeId(Uuid::new_v4())),
                success: true,
                usefulness: 0.8,
                prediction_correct: true,
                user_accepted: true,
                validated: true,
            },
        );

        let evidence_update = result
            .updates
            .iter()
            .find(|update| update.trait_type == TraitType::EvidenceReliance)
            .expect("evidence reliance update should exist");

        assert_eq!(evidence_update.outcome_id, "outcome-audit");
        assert!(evidence_update.subject_node_id.is_some());
        assert!(!evidence_update.supporting_lesson_ids.is_empty());
    }

    fn sample_lesson(lesson_type: LessonType, title: &str, statement: &str) -> Lesson {
        Lesson {
            id: memory_core::LessonId(Uuid::new_v4()),
            lesson_type,
            status: MemoryStatus::Reinforced,
            title: title.to_owned(),
            statement: statement.to_owned(),
            confidence: 0.92,
            evidence_count: 2,
            reinforcement_count: 2,
            supporting_node_ids: Vec::new(),
            contradicting_node_ids: Vec::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}
