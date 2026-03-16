//! Personality scaffolding for Nodamem.

use memory_core::CoreMarker;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersonalityMarker {
    pub core: CoreMarker,
}

