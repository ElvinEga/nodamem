//! Ingestion scaffolding for Nodamem.

use memory_core::CoreMarker;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IngestMarker {
    pub core: CoreMarker,
}

