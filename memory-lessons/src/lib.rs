//! Lessons scaffolding for Nodamem.

use memory_core::CoreMarker;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LessonsMarker {
    pub core: CoreMarker,
}
