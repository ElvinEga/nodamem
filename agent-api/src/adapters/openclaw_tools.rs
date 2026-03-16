use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenClawToolDescription {
    pub name: String,
    pub what_it_does: String,
    pub when_to_call: String,
    pub when_not_to_call: String,
    pub returns: String,
    pub example_request: JsonValue,
}

#[must_use]
pub fn openclaw_tool_descriptions() -> Vec<OpenClawToolDescription> {
    vec![
        OpenClawToolDescription {
            name: "recall_context".to_owned(),
            what_it_does: "Retrieves compact verified memory context relevant to the current task."
                .to_owned(),
            when_to_call: "Call before answering when prior context, ongoing projects, or stable preferences may matter.".to_owned(),
            when_not_to_call: "Do not call for trivial one-off responses where no prior context is relevant.".to_owned(),
            returns: "Compact summaries of verified nodes, lessons, an optional checkpoint summary, and a trait snapshot.".to_owned(),
            example_request: json!({"text":"Continue the architecture discussion","session_id":"session-7","topic":"architecture","nodes":[],"edges":[],"lessons":[],"checkpoints":[],"traits":[]}),
        },
        OpenClawToolDescription {
            name: "get_neighbors".to_owned(),
            what_it_does: "Retrieves nearby verified graph context for a specific node.".to_owned(),
            when_to_call: "Call when a recalled node seems important and you need nearby graph context.".to_owned(),
            when_not_to_call: "Do not call to dump the whole graph or when recall_context already gave enough context.".to_owned(),
            returns: "Compact neighboring node summaries and a connection count.".to_owned(),
            example_request: json!({"node_id":"00000000-0000-0000-0000-000000000000","nodes":[],"edges":[]}),
        },
        OpenClawToolDescription {
            name: "propose_memory".to_owned(),
            what_it_does: "Submits a candidate memory through ingestion and admission validation.".to_owned(),
            when_to_call: "Call after learning durable, connected, non-trivial information worth preserving.".to_owned(),
            when_not_to_call: "Do not call for ephemeral reasoning, isolated noise, or data that should stay only in working context.".to_owned(),
            returns: "Candidate counts plus validated admission decisions; it does not directly write raw tables.".to_owned(),
            example_request: json!({"event":{"UserMessage":{"event_id":"evt-1","session_id":"session-7","message_id":"msg-22","text":"I prefer design docs to include rollout notes."}},"context":{"existing_nodes":[],"existing_edges":[]}}),
        },
        OpenClawToolDescription {
            name: "propose_lesson".to_owned(),
            what_it_does: "Proposes reusable learned meaning from accepted memories.".to_owned(),
            when_to_call: "Call when the agent identifies a stable pattern, strategy, or reusable lesson.".to_owned(),
            when_not_to_call: "Do not call for one-off events that have not generalized into a reusable pattern.".to_owned(),
            returns: "Validated lesson outcomes such as create, reinforce, refine, or contradiction hook.".to_owned(),
            example_request: json!({"accepted_memories":[],"existing_lessons":[]}),
        },
        OpenClawToolDescription {
            name: "record_outcome".to_owned(),
            what_it_does: "Feeds validated task outcomes into the trait subsystem.".to_owned(),
            when_to_call: "Call after success, failure, user acceptance, rejection, or prediction mismatch.".to_owned(),
            when_not_to_call: "Do not call before an outcome is known or when the signal is not validated.".to_owned(),
            returns: "Trait update summaries and the count of updated traits.".to_owned(),
            example_request: json!({"existing_traits":[],"outcome":{"outcome_id":"out-7","subject_node_id":null,"success":false,"usefulness":0.2,"prediction_correct":false,"user_accepted":false,"validated":true}}),
        },
        OpenClawToolDescription {
            name: "generate_imagined_scenarios".to_owned(),
            what_it_does: "Generates hypothetical planning scenarios grounded in verified context.".to_owned(),
            when_to_call: "Call when planning or forecasting would benefit from speculative alternatives.".to_owned(),
            when_not_to_call: "Do not call when verified facts are sufficient or when the result might be mistaken for established memory.".to_owned(),
            returns: "Hypothetical scenarios explicitly labeled as non-verified.".to_owned(),
            example_request: json!({"planning_task":"Plan the next release","desired_scenarios":2,"context_packet":{"id":"00000000-0000-0000-0000-000000000000","request_id":null,"created_at":"2026-01-01T00:00:00Z","nodes":[],"edges":[],"lessons":[],"traits":[],"checkpoints":[],"imagined_scenarios":[]},"active_goal_node_ids":[]}),
        },
    ]
}
