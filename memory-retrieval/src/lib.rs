//! Retrieval scaffolding for Nodamem.

use memory_core::CoreMarker;
use memory_store::StoreMarker;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetrievalMarker {
    pub core: CoreMarker,
    pub store: StoreMarker,
}

