//! Storage scaffolding and local-first database bootstrap for Nodamem.

use memory_core::CoreMarker;

pub mod audit;
pub mod config;
pub mod db;
pub mod error;
pub mod mapper;
pub mod migrations;
pub mod repository;

pub use audit::{LessonAuditTrail, NodeAuditTrail};
pub use config::{StoreConfig, TursoSyncConfig};
pub use db::{open_database, StoreRuntime};
pub use error::StoreError;
pub use migrations::run_migrations;
pub use repository::{LessonSourceRole, NodeEmbeddingRecord, StoreRepository, VectorSearchMatch};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoreMarker {
    pub core: CoreMarker,
}
