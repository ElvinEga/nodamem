//! Agent-facing API scaffolding for Nodamem.

use memory_core::CoreMarker;
use memory_imagination::ImaginationMarker;
use memory_ingest::IngestMarker;
use memory_lessons::LessonsMarker;
use memory_personality::PersonalityMarker;
use memory_retrieval::RetrievalMarker;
use memory_sleep::SleepMarker;
use memory_store::StoreMarker;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentApi {
    pub core: CoreMarker,
    pub imagination: ImaginationMarker,
    pub ingest: IngestMarker,
    pub lessons: LessonsMarker,
    pub personality: PersonalityMarker,
    pub retrieval: RetrievalMarker,
    pub sleep: SleepMarker,
    pub store: StoreMarker,
}
