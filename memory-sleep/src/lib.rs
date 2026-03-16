//! Sleep-cycle scaffolding for Nodamem.

use memory_core::CoreMarker;
use memory_lessons::LessonsMarker;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SleepMarker {
    pub core: CoreMarker,
    pub lessons: LessonsMarker,
}

