//! Imagination scaffolding for Nodamem.

use memory_core::CoreMarker;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImaginationMarker {
    pub core: CoreMarker,
}
