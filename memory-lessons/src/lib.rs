//! Deterministic lesson proposal and reinforcement logic for Nodamem.

use chrono::Utc;
use memory_core::{CoreMarker, Lesson, LessonId, LessonType, MemoryStatus, Node, NodeId};
use std::collections::HashSet;
use tracing::{debug, info};
use uuid::Uuid;

/// Candidate lesson proposed from accepted memories before persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedLesson {
    pub lesson: Lesson,
    pub source_memory_ids: Vec<NodeId>,
}

/// Evidence link between a lesson and a supporting or contradicting source memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonEvidenceLink {
    pub lesson_id: LessonId,
    pub node_id: NodeId,
    pub role: EvidenceRole,
}

/// Role of a memory node when linked as lesson evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceRole {
    Supporting,
    Contradicting,
}

/// Similar lesson match used for reinforcement or refinement decisions.
#[derive(Debug, Clone, PartialEq)]
pub struct SimilarLessonMatch {
    pub lesson_id: LessonId,
    pub similarity: f32,
}

/// Outcome of evaluating a lesson against existing lessons.
#[derive(Debug, Clone, PartialEq)]
pub enum LessonOutcome {
    CreateNew(ProposedLesson),
    ReinforceExisting {
        updated_lesson: Lesson,
        evidence_links: Vec<LessonEvidenceLink>,
    },
    RefineExisting {
        updated_lesson: Lesson,
        evidence_links: Vec<LessonEvidenceLink>,
    },
    WeakenExisting {
        updated_lesson: Lesson,
        evidence_links: Vec<LessonEvidenceLink>,
    },
    ContradictionHook {
        target_lesson_id: LessonId,
        evidence_links: Vec<LessonEvidenceLink>,
    },
}

/// Deterministic behavior configuration for lesson handling.
#[derive(Debug, Clone, PartialEq)]
pub struct LessonPolicy {
    pub min_memory_importance: f32,
    pub min_similarity_for_reinforcement: f32,
    pub min_similarity_for_refinement: f32,
    pub confidence_increment: f32,
    pub refinement_confidence_increment: f32,
}

impl Default for LessonPolicy {
    fn default() -> Self {
        Self {
            min_memory_importance: 0.45,
            min_similarity_for_reinforcement: 0.75,
            min_similarity_for_refinement: 0.45,
            confidence_increment: 0.08,
            refinement_confidence_increment: 0.04,
        }
    }
}

/// Interface for future LLM-assisted lesson proposal.
pub trait LessonProposer {
    fn propose_from_memories(&self, accepted_memories: &[Node]) -> Vec<ProposedLesson>;
}

/// Interface for matching new lessons to existing lessons.
pub trait SimilarLessonMatcher {
    fn find_similar(
        &self,
        candidate: &Lesson,
        existing_lessons: &[Lesson],
    ) -> Option<SimilarLessonMatch>;
}

/// Interface for contradiction or refinement evaluation hooks.
pub trait ContradictionHandler {
    fn evaluate(&self, candidate: &Lesson, existing_lesson: &Lesson) -> ContradictionDisposition;
}

/// Contradiction/refinement hook result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContradictionDisposition {
    None,
    Refine,
    Contradiction,
}

/// Deterministic lesson proposal and maintenance service.
#[derive(Debug, Clone)]
pub struct LessonService<M, C> {
    pub policy: LessonPolicy,
    pub matcher: M,
    pub contradiction_handler: C,
}

impl<M, C> LessonService<M, C> {
    #[must_use]
    pub fn new(policy: LessonPolicy, matcher: M, contradiction_handler: C) -> Self {
        Self {
            policy,
            matcher,
            contradiction_handler,
        }
    }
}

impl Default for LessonService<DeterministicLessonMatcher, DeterministicContradictionHandler> {
    fn default() -> Self {
        Self::new(
            LessonPolicy::default(),
            DeterministicLessonMatcher,
            DeterministicContradictionHandler,
        )
    }
}

impl<M, C> LessonService<M, C>
where
    M: SimilarLessonMatcher,
    C: ContradictionHandler,
{
    pub fn process_memories(
        &self,
        accepted_memories: &[Node],
        existing_lessons: &[Lesson],
    ) -> Vec<LessonOutcome> {
        debug!(
            accepted_memories = accepted_memories.len(),
            existing_lessons = existing_lessons.len(),
            "processing accepted memories into lesson outcomes"
        );
        let proposals = DeterministicLessonProposer::new(self.policy.clone())
            .propose_from_memories(accepted_memories);

        proposals
            .into_iter()
            .map(|proposal| self.resolve_proposal(proposal, existing_lessons))
            .collect()
    }

    fn resolve_proposal(
        &self,
        proposal: ProposedLesson,
        existing_lessons: &[Lesson],
    ) -> LessonOutcome {
        let candidate = &proposal.lesson;

        if let Some(similar) = self.matcher.find_similar(candidate, existing_lessons) {
            let Some(existing_lesson) = existing_lessons
                .iter()
                .find(|lesson| lesson.id == similar.lesson_id)
            else {
                return LessonOutcome::CreateNew(proposal);
            };

            match self
                .contradiction_handler
                .evaluate(candidate, existing_lesson)
            {
                ContradictionDisposition::Contradiction => {
                    info!(
                        candidate_lesson_id = %candidate.id.0,
                        target_lesson_id = %existing_lesson.id.0,
                        similarity = similar.similarity,
                        evidence_increment = proposal.source_memory_ids.len(),
                        "lesson proposal weakened a contradicted lesson"
                    );
                    return LessonOutcome::WeakenExisting {
                        updated_lesson: weaken_lesson(
                            existing_lesson,
                            self.policy.refinement_confidence_increment,
                            &proposal.source_memory_ids,
                        ),
                        evidence_links: contradiction_links(existing_lesson.id, &proposal),
                    };
                }
                ContradictionDisposition::Refine
                    if similar.similarity >= self.policy.min_similarity_for_reinforcement =>
                {
                    info!(
                        candidate_lesson_id = %candidate.id.0,
                        target_lesson_id = %existing_lesson.id.0,
                        similarity = similar.similarity,
                        evidence_increment = proposal.source_memory_ids.len(),
                        "high-similarity lesson proposal reinforced instead of refining"
                    );
                    return LessonOutcome::ReinforceExisting {
                        updated_lesson: reinforce_lesson(
                            existing_lesson,
                            self.policy.confidence_increment,
                            &proposal.source_memory_ids,
                        ),
                        evidence_links: supporting_links(existing_lesson.id, &proposal),
                    };
                }
                ContradictionDisposition::Refine
                    if similar.similarity >= self.policy.min_similarity_for_refinement =>
                {
                    info!(
                        candidate_lesson_id = %candidate.id.0,
                        target_lesson_id = %existing_lesson.id.0,
                        similarity = similar.similarity,
                        evidence_increment = proposal.source_memory_ids.len(),
                        "lesson proposal refined an existing lesson"
                    );
                    return LessonOutcome::RefineExisting {
                        updated_lesson: refine_lesson(
                            existing_lesson,
                            candidate,
                            self.policy.refinement_confidence_increment,
                            &proposal.source_memory_ids,
                        ),
                        evidence_links: supporting_links(existing_lesson.id, &proposal),
                    };
                }
                ContradictionDisposition::Refine | ContradictionDisposition::None => {}
            }

            if similar.similarity >= self.policy.min_similarity_for_reinforcement {
                info!(
                    candidate_lesson_id = %candidate.id.0,
                    target_lesson_id = %existing_lesson.id.0,
                    similarity = similar.similarity,
                    evidence_increment = proposal.source_memory_ids.len(),
                    "lesson proposal reinforced an existing lesson"
                );
                return LessonOutcome::ReinforceExisting {
                    updated_lesson: reinforce_lesson(
                        existing_lesson,
                        self.policy.confidence_increment,
                        &proposal.source_memory_ids,
                    ),
                    evidence_links: supporting_links(existing_lesson.id, &proposal),
                };
            }
        }

        debug!(
            candidate_lesson_id = %candidate.id.0,
            lesson_type = ?candidate.lesson_type,
            "lesson proposal will create a new lesson"
        );
        LessonOutcome::CreateNew(proposal)
    }
}

/// Deterministic lesson proposer used until learned extraction is introduced.
#[derive(Debug, Clone)]
pub struct DeterministicLessonProposer {
    policy: LessonPolicy,
}

impl DeterministicLessonProposer {
    #[must_use]
    pub fn new(policy: LessonPolicy) -> Self {
        Self { policy }
    }
}

impl LessonProposer for DeterministicLessonProposer {
    fn propose_from_memories(&self, accepted_memories: &[Node]) -> Vec<ProposedLesson> {
        accepted_memories
            .iter()
            .filter(|memory| memory.importance >= self.policy.min_memory_importance)
            .filter_map(propose_lesson_from_memory)
            .collect()
    }
}

/// Deterministic lesson matcher based on token overlap and lesson type agreement.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicLessonMatcher;

impl SimilarLessonMatcher for DeterministicLessonMatcher {
    fn find_similar(
        &self,
        candidate: &Lesson,
        existing_lessons: &[Lesson],
    ) -> Option<SimilarLessonMatch> {
        existing_lessons
            .iter()
            .filter_map(|existing| {
                let type_bonus = if existing.lesson_type == candidate.lesson_type {
                    0.15
                } else {
                    0.0
                };
                let similarity = (text_overlap(&candidate.statement, &existing.statement)
                    + type_bonus)
                    .clamp(0.0, 1.0);

                (similarity > 0.3).then_some(SimilarLessonMatch {
                    lesson_id: existing.id,
                    similarity,
                })
            })
            .max_by(|left, right| left.similarity.total_cmp(&right.similarity))
    }
}

/// Deterministic contradiction/refinement hook based on simple negation cues.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicContradictionHandler;

impl ContradictionHandler for DeterministicContradictionHandler {
    fn evaluate(&self, candidate: &Lesson, existing_lesson: &Lesson) -> ContradictionDisposition {
        let candidate_lower = candidate.statement.to_ascii_lowercase();
        let existing_lower = existing_lesson.statement.to_ascii_lowercase();
        let negated = candidate_lower.contains(" not ")
            || candidate_lower.contains("never")
            || candidate_lower.contains("avoid");

        if negated && text_overlap(&candidate.statement, &existing_lesson.statement) > 0.45 {
            ContradictionDisposition::Contradiction
        } else if text_overlap(&candidate.statement, &existing_lesson.statement) > 0.45 {
            let changed = candidate_lower != existing_lower;
            if changed {
                ContradictionDisposition::Refine
            } else {
                ContradictionDisposition::None
            }
        } else {
            ContradictionDisposition::None
        }
    }
}

fn propose_lesson_from_memory(memory: &Node) -> Option<ProposedLesson> {
    let content = memory.content.as_deref().unwrap_or(&memory.summary);
    let lower = content.to_ascii_lowercase();
    let lesson_type = infer_lesson_type(memory, &lower)?;
    let statement = lesson_statement(content);
    let now = Utc::now();

    Some(ProposedLesson {
        lesson: Lesson {
            id: LessonId(Uuid::new_v4()),
            lesson_type,
            status: MemoryStatus::Candidate,
            title: truncate_title(&statement),
            statement,
            confidence: (memory.confidence * 0.7 + memory.importance * 0.3).clamp(0.0, 1.0),
            evidence_count: 1,
            reinforcement_count: 0,
            supporting_node_ids: vec![memory.id],
            contradicting_node_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        },
        source_memory_ids: vec![memory.id],
    })
}

fn infer_lesson_type(memory: &Node, lower: &str) -> Option<LessonType> {
    if lower.contains("user") || memory.tags.iter().any(|tag| tag == "user") {
        Some(LessonType::User)
    } else if lower.contains("system") || memory.tags.iter().any(|tag| tag == "system") {
        Some(LessonType::System)
    } else if lower.contains("strategy")
        || lower.contains("should")
        || lower.contains("best practice")
    {
        Some(LessonType::Strategy)
    } else if lower.contains("domain")
        || lower.contains("database")
        || lower.contains("rust")
        || lower.contains("sqlite")
    {
        Some(LessonType::Domain)
    } else if lower.contains("personality")
        || lower.contains("preference")
        || lower.contains("style")
    {
        Some(LessonType::Personality)
    } else if lower.contains("task")
        || lower.contains("workflow")
        || lower.contains("process")
        || memory.tags.iter().any(|tag| tag == "tool")
    {
        Some(LessonType::Task)
    } else {
        None
    }
}

fn reinforce_lesson(
    existing: &Lesson,
    confidence_increment: f32,
    new_supporting_node_ids: &[NodeId],
) -> Lesson {
    let mut lesson = existing.clone();
    let added_supporting_count =
        count_new_node_ids(&lesson.supporting_node_ids, new_supporting_node_ids);
    lesson.status = MemoryStatus::Reinforced;
    lesson.confidence = (lesson.confidence + confidence_increment).clamp(0.0, 1.0);
    lesson.supporting_node_ids =
        merge_node_ids(&lesson.supporting_node_ids, new_supporting_node_ids);
    lesson.evidence_count = lesson
        .evidence_count
        .saturating_add(added_supporting_count as u32);
    lesson.reinforcement_count = lesson.reinforcement_count.saturating_add(1);
    lesson.updated_at = Utc::now();
    lesson
}

fn refine_lesson(
    existing: &Lesson,
    candidate: &Lesson,
    confidence_increment: f32,
    new_supporting_node_ids: &[NodeId],
) -> Lesson {
    let mut lesson = existing.clone();
    let added_supporting_count =
        count_new_node_ids(&lesson.supporting_node_ids, new_supporting_node_ids);
    lesson.statement = candidate.statement.clone();
    lesson.title = candidate.title.clone();
    lesson.confidence = (lesson.confidence + confidence_increment).clamp(0.0, 1.0);
    lesson.supporting_node_ids =
        merge_node_ids(&lesson.supporting_node_ids, new_supporting_node_ids);
    lesson.evidence_count = lesson
        .evidence_count
        .saturating_add(added_supporting_count as u32);
    lesson.reinforcement_count = lesson.reinforcement_count.saturating_add(1);
    lesson.updated_at = Utc::now();
    lesson
}

fn weaken_lesson(
    existing: &Lesson,
    confidence_decrement: f32,
    contradicting_node_ids: &[NodeId],
) -> Lesson {
    let mut lesson = existing.clone();
    let added_contradicting_count =
        count_new_node_ids(&lesson.contradicting_node_ids, contradicting_node_ids);
    lesson.status = MemoryStatus::Contradicted;
    lesson.confidence = (lesson.confidence - confidence_decrement).clamp(0.0, 1.0);
    lesson.contradicting_node_ids =
        merge_node_ids(&lesson.contradicting_node_ids, contradicting_node_ids);
    lesson.evidence_count = lesson
        .evidence_count
        .saturating_add(added_contradicting_count as u32);
    lesson.updated_at = Utc::now();
    lesson
}

fn supporting_links(lesson_id: LessonId, proposal: &ProposedLesson) -> Vec<LessonEvidenceLink> {
    proposal
        .source_memory_ids
        .iter()
        .copied()
        .map(|node_id| LessonEvidenceLink {
            lesson_id,
            node_id,
            role: EvidenceRole::Supporting,
        })
        .collect()
}

fn contradiction_links(lesson_id: LessonId, proposal: &ProposedLesson) -> Vec<LessonEvidenceLink> {
    proposal
        .source_memory_ids
        .iter()
        .copied()
        .map(|node_id| LessonEvidenceLink {
            lesson_id,
            node_id,
            role: EvidenceRole::Contradicting,
        })
        .collect()
}

fn merge_node_ids(existing: &[NodeId], additional: &[NodeId]) -> Vec<NodeId> {
    let mut merged = existing.to_vec();
    let mut seen = existing.iter().copied().collect::<HashSet<_>>();
    for node_id in additional {
        if seen.insert(*node_id) {
            merged.push(*node_id);
        }
    }
    merged
}

fn count_new_node_ids(existing: &[NodeId], additional: &[NodeId]) -> usize {
    let seen = existing.iter().copied().collect::<HashSet<_>>();
    additional
        .iter()
        .filter(|node_id| !seen.contains(node_id))
        .count()
}

fn lesson_statement(content: &str) -> String {
    content
        .split_terminator(['.', '!', '?'])
        .next()
        .unwrap_or(content)
        .trim()
        .to_owned()
}

fn truncate_title(text: &str) -> String {
    const MAX_LEN: usize = 80;
    if text.chars().count() <= MAX_LEN {
        text.to_owned()
    } else {
        let truncated: String = text.chars().take(MAX_LEN - 3).collect();
        format!("{truncated}...")
    }
}

fn text_overlap(left: &str, right: &str) -> f32 {
    let left_terms = split_terms(left);
    let right_terms = split_terms(right);

    if left_terms.is_empty() || right_terms.is_empty() {
        return 0.0;
    }

    let overlap = left_terms
        .iter()
        .filter(|term| right_terms.contains(*term))
        .count() as f32;
    let denominator = left_terms.len().max(right_terms.len()) as f32;
    (overlap / denominator).clamp(0.0, 1.0)
}

fn split_terms(text: &str) -> Vec<String> {
    let mut terms = text
        .split_whitespace()
        .map(|term| {
            term.trim_matches(|character: char| !character.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|term| term.len() > 2)
        .collect::<Vec<_>>();
    terms.sort();
    terms.dedup();
    terms
}

/// Marker preserved for lightweight crate wiring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LessonsMarker {
    pub core: CoreMarker,
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use memory_core::{LessonType, MemoryStatus, Node, NodeId, NodeType};
    use uuid::Uuid;

    use super::{LessonOutcome, LessonService};

    #[test]
    fn proposes_lesson_from_accepted_memory() {
        let service = LessonService::default();
        let memory = sample_memory(
            "system migration strategy",
            "System strategy: we should run migrations before opening agent retrieval.",
            vec!["system".to_owned()],
            0.8,
            0.85,
        );

        let outcomes = service.process_memories(&[memory.clone()], &[]);
        assert_eq!(outcomes.len(), 1);

        match &outcomes[0] {
            LessonOutcome::CreateNew(proposal) => {
                assert_eq!(proposal.lesson.lesson_type, LessonType::System);
                assert_eq!(proposal.lesson.evidence_count, 1);
                assert_eq!(proposal.source_memory_ids, vec![memory.id]);
            }
            outcome => panic!("unexpected outcome: {outcome:?}"),
        }
    }

    #[test]
    fn reinforces_similar_existing_lesson() {
        let service = LessonService::default();
        let memory = sample_memory(
            "task workflow",
            "Task workflow: process steps should be stored deterministically.",
            vec!["tool".to_owned()],
            0.8,
            0.8,
        );
        let existing = memory_core::Lesson {
            id: memory_core::LessonId(Uuid::new_v4()),
            lesson_type: LessonType::Task,
            status: MemoryStatus::Active,
            title: "process steps".to_owned(),
            statement: "Task workflow process steps should be stored deterministically".to_owned(),
            confidence: 0.6,
            evidence_count: 2,
            reinforcement_count: 1,
            supporting_node_ids: vec![memory.id],
            contradicting_node_ids: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let outcomes = service.process_memories(&[memory], &[existing.clone()]);

        match &outcomes[0] {
            LessonOutcome::ReinforceExisting {
                updated_lesson,
                evidence_links,
            } => {
                assert!(updated_lesson.confidence > existing.confidence);
                assert_eq!(updated_lesson.evidence_count, existing.evidence_count);
                assert_eq!(evidence_links.len(), 1);
            }
            outcome => panic!("unexpected outcome: {outcome:?}"),
        }
    }

    #[test]
    fn triggers_contradiction_hook_for_negated_similar_lesson() {
        let service = LessonService::default();
        let memory = sample_memory(
            "strategy revision",
            "Strategy note: do not store every isolated memory.",
            vec!["system".to_owned()],
            0.9,
            0.9,
        );
        let existing = memory_core::Lesson {
            id: memory_core::LessonId(Uuid::new_v4()),
            lesson_type: LessonType::Strategy,
            status: MemoryStatus::Active,
            title: "store isolated memory".to_owned(),
            statement: "Strategy note: store every isolated memory.".to_owned(),
            confidence: 0.7,
            evidence_count: 1,
            reinforcement_count: 0,
            supporting_node_ids: vec![memory.id],
            contradicting_node_ids: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let outcomes = service.process_memories(&[memory], &[existing.clone()]);

        match &outcomes[0] {
            LessonOutcome::WeakenExisting {
                updated_lesson,
                evidence_links,
            } => {
                assert_eq!(updated_lesson.id, existing.id);
                assert!(updated_lesson.confidence < existing.confidence);
                assert_eq!(updated_lesson.status, MemoryStatus::Contradicted);
                assert_eq!(updated_lesson.contradicting_node_ids.len(), 1);
                assert_eq!(evidence_links.len(), 1);
            }
            outcome => panic!("unexpected outcome: {outcome:?}"),
        }
    }

    #[test]
    fn refines_lesson_and_preserves_provenance() {
        let service = LessonService::default();
        let prior_node_id = NodeId(Uuid::new_v4());
        let new_memory = Node {
            id: NodeId(Uuid::new_v4()),
            node_type: NodeType::Semantic,
            status: MemoryStatus::Active,
            title: "strategy detail".to_owned(),
            summary: "We should run migrations before retrieval and before opening the adapter."
                .to_owned(),
            content: Some(
                "We should run migrations before retrieval and before opening the adapter."
                    .to_owned(),
            ),
            tags: vec!["system".to_owned()],
            confidence: 0.85,
            importance: 0.9,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            source_event_id: Some("new-evidence".to_owned()),
        };
        let existing = memory_core::Lesson {
            id: memory_core::LessonId(Uuid::new_v4()),
            lesson_type: LessonType::System,
            status: MemoryStatus::Active,
            title: "run migrations".to_owned(),
            statement: "We should run migrations before retrieval.".to_owned(),
            confidence: 0.6,
            evidence_count: 1,
            reinforcement_count: 0,
            supporting_node_ids: vec![prior_node_id],
            contradicting_node_ids: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let outcomes = service.process_memories(&[new_memory.clone()], &[existing.clone()]);

        match &outcomes[0] {
            LessonOutcome::RefineExisting {
                updated_lesson,
                evidence_links,
            } => {
                assert!(updated_lesson
                    .statement
                    .contains("before opening the adapter"));
                assert_eq!(updated_lesson.supporting_node_ids.len(), 2);
                assert!(updated_lesson.supporting_node_ids.contains(&prior_node_id));
                assert!(updated_lesson.supporting_node_ids.contains(&new_memory.id));
                assert_eq!(updated_lesson.evidence_count, 2);
                assert_eq!(evidence_links.len(), 1);
            }
            outcome => panic!("unexpected outcome: {outcome:?}"),
        }
    }

    #[test]
    fn reinforcement_dedupes_evidence_ids() {
        let service = LessonService::default();
        let memory_id = NodeId(Uuid::new_v4());
        let memory = Node {
            id: memory_id,
            node_type: NodeType::Semantic,
            status: MemoryStatus::Active,
            title: "task workflow".to_owned(),
            summary: "Task workflow: process steps should be stored deterministically.".to_owned(),
            content: Some(
                "Task workflow: process steps should be stored deterministically.".to_owned(),
            ),
            tags: vec!["tool".to_owned()],
            confidence: 0.8,
            importance: 0.8,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            source_event_id: Some("accepted-memory".to_owned()),
        };
        let existing = memory_core::Lesson {
            id: memory_core::LessonId(Uuid::new_v4()),
            lesson_type: LessonType::Task,
            status: MemoryStatus::Active,
            title: "process steps".to_owned(),
            statement: "Task workflow process steps should be stored deterministically".to_owned(),
            confidence: 0.6,
            evidence_count: 1,
            reinforcement_count: 1,
            supporting_node_ids: vec![memory_id],
            contradicting_node_ids: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let outcomes = service.process_memories(&[memory], &[existing.clone()]);

        match &outcomes[0] {
            LessonOutcome::ReinforceExisting { updated_lesson, .. } => {
                assert_eq!(updated_lesson.supporting_node_ids, vec![memory_id]);
                assert_eq!(updated_lesson.evidence_count, 1);
            }
            outcome => panic!("unexpected outcome: {outcome:?}"),
        }
    }

    fn sample_memory(
        title: &str,
        content: &str,
        tags: Vec<String>,
        confidence: f32,
        importance: f32,
    ) -> Node {
        Node {
            id: NodeId(Uuid::new_v4()),
            node_type: NodeType::Semantic,
            status: MemoryStatus::Active,
            title: title.to_owned(),
            summary: content.to_owned(),
            content: Some(content.to_owned()),
            tags,
            confidence,
            importance,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_accessed_at: None,
            source_event_id: Some("accepted-memory".to_owned()),
        }
    }
}
