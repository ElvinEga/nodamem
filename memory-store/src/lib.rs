//! Storage scaffolding and local-first database bootstrap for Nodamem.

use memory_core::CoreMarker;

pub mod config;
pub mod db;

pub use config::StoreConfig;
pub use db::{open_database, StoreError, StoreRuntime};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoreMarker {
    pub core: CoreMarker,
}
