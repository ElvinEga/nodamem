//! Slow-moving personality and outcome-learning subsystem for Nodamem.

use chrono::Utc;
use memory_core::{CoreMarker, MemoryStatus, NodeId, TraitId, TraitState, TraitType};
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
    pub trait_type: TraitType,
    pub previous_strength: f32,
    pub updated_strength: f32,
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
}

impl Default for PersonalityPolicy {
    fn default() -> Self {
        Self {
            learning_rate: 0.08,
            max_step_per_outcome: 0.05,
            confidence_increment: 0.02,
            minimum_start_strength: 0.5,
        }
    }
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

    pub fn record_outcome(
        &self,
        traits: &[TraitState],
        outcome: &OutcomeRecord,
    ) -> (Vec<TraitState>, Vec<TraitUpdate>) {
        if !outcome.validated {
            debug!(outcome_id = %outcome.outcome_id, "skipping trait update for unvalidated outcome");
            return (traits.to_vec(), Vec::new());
        }

        let mut updated_traits = traits.to_vec();
        let mut updates = Vec::new();

        for (trait_type, signal) in trait_signals(outcome) {
            let existing_index = updated_traits
                .iter()
                .position(|trait_state| trait_state.trait_type == trait_type);

            let trait_state = existing_index
                .map(|index| updated_traits[index].clone())
                .unwrap_or_else(|| {
                    default_trait_state(trait_type, self.policy.minimum_start_strength)
                });

            let updated = self.apply_signal(&trait_state, signal);

            updates.push(TraitUpdate {
                trait_type,
                previous_strength: trait_state.strength,
                updated_strength: updated.strength,
            });

            info!(
                outcome_id = %outcome.outcome_id,
                subject_node_id = ?outcome.subject_node_id.map(|id| id.0.to_string()),
                trait_type = ?trait_type,
                signal,
                previous_strength = trait_state.strength,
                updated_strength = updated.strength,
                updated_confidence = updated.confidence,
                "applied trait update from validated outcome"
            );

            if let Some(index) = existing_index {
                updated_traits[index] = updated;
            } else {
                updated_traits.push(updated);
            }
        }

        (updated_traits, updates)
    }

    fn apply_signal(&self, trait_state: &TraitState, signal: f32) -> TraitState {
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
            supporting_lesson_ids: trait_state.supporting_lesson_ids.clone(),
            supporting_node_ids: trait_state.supporting_node_ids.clone(),
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
    ]
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
    use memory_core::TraitType;

    #[test]
    fn repeated_validated_success_reinforces_traits_gradually() {
        let service = PersonalityService::default();
        let mut traits = Vec::new();

        for index in 0..5 {
            let (updated_traits, updates) = service.record_outcome(
                &traits,
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
            assert!(!updates.is_empty());
            traits = updated_traits;
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
        let (traits, updates) = service.record_outcome(
            &[],
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

        assert!(traits.is_empty());
        assert!(updates.is_empty());
    }

    #[test]
    fn repeated_failures_raise_caution_slowly() {
        let service = PersonalityService::default();
        let mut traits = Vec::new();

        for index in 0..4 {
            let (updated_traits, _) = service.record_outcome(
                &traits,
                &OutcomeRecord {
                    outcome_id: format!("outcome-failure-{index}"),
                    subject_node_id: None,
                    success: false,
                    usefulness: 0.2,
                    prediction_correct: false,
                    user_accepted: false,
                    validated: true,
                },
            );
            traits = updated_traits;
        }

        let caution = traits
            .iter()
            .find(|trait_state| trait_state.trait_type == TraitType::Caution)
            .expect("caution trait should exist");

        assert!(caution.strength > 0.5);
        assert!(caution.strength < 1.0);
    }
}
