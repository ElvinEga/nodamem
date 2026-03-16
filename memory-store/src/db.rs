//! Database bootstrap for local-first libSQL storage.

use std::path::Path;

use libsql::{Builder, Connection, Database};
use tracing::{info, warn};

use crate::config::{StoreConfig, TursoSyncConfig};
use crate::error::StoreError;
use crate::migrations::run_migrations;
use crate::repository::StoreRepository;

#[derive(Debug)]
pub struct StoreRuntime {
    pub config: StoreConfig,
    pub database: Database,
    pub connection: Connection,
}

impl StoreRuntime {
    pub async fn open(config: StoreConfig) -> Result<Self, StoreError> {
        ensure_parent_dir(&config.local_database_path).await?;

        let database = open_database_with_optional_sync(&config).await?;
        let connection = database.connect()?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .await?;
        run_migrations(&connection).await?;

        Ok(Self {
            config,
            database,
            connection,
        })
    }

    pub async fn smoke_check(&self) -> Result<(), StoreError> {
        let mut rows = self.connection.query("SELECT 1", ()).await?;
        let _ = rows.next().await?;
        Ok(())
    }

    #[must_use]
    pub fn repository(&self) -> StoreRepository<'_> {
        StoreRepository::new(&self.connection)
    }
}

pub async fn open_database() -> Result<StoreRuntime, StoreError> {
    StoreRuntime::open(StoreConfig::from_env()).await
}

async fn open_database_with_optional_sync(config: &StoreConfig) -> Result<Database, StoreError> {
    let backend = select_backend(config);

    match backend {
        StoreBackend::LocalOnly => {
            info!(
                local_database_path = %config.local_database_path.display(),
                "opening local embedded libsql database"
            );
            Builder::new_local(&config.local_database_path)
                .build()
                .await
                .map_err(StoreError::from)
        }
        StoreBackend::Synced(sync_config) => {
            info!(
                local_database_path = %config.local_database_path.display(),
                turso_database_url = %sync_config.database_url,
                read_your_writes = sync_config.read_your_writes,
                "opening local database with optional Turso sync enabled"
            );
            Builder::new_synced_database(
                &config.local_database_path,
                sync_config.database_url,
                sync_config.auth_token,
            )
            .read_your_writes(sync_config.read_your_writes)
            .build()
            .await
            .map_err(StoreError::from)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StoreBackend {
    LocalOnly,
    Synced(TursoSyncConfig),
}

fn select_backend(config: &StoreConfig) -> StoreBackend {
    if let Some(sync_config) = config.turso_sync_config() {
        StoreBackend::Synced(sync_config)
    } else {
        if config.sync_requested_without_credentials() {
            warn!(
                local_database_path = %config.local_database_path.display(),
                sync_enabled = config.sync_enabled,
                has_database_url = config.turso_database_url.is_some(),
                has_auth_token = config.turso_auth_token.is_some(),
                "Turso sync requested without complete credentials; continuing in local-only mode"
            );
        }

        StoreBackend::LocalOnly
    }
}

async fn ensure_parent_dir(path: &Path) -> Result<(), StoreError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{select_backend, StoreBackend, StoreRuntime};
    use crate::config::StoreConfig;

    #[tokio::test]
    async fn opens_embedded_database_and_runs_smoke_check() {
        let tempdir = tempdir().expect("temporary directory should be created");
        let config = StoreConfig {
            local_database_path: tempdir.path().join("nodamem.db"),
            ..StoreConfig::default()
        };

        let runtime = StoreRuntime::open(config)
            .await
            .expect("database should open");

        runtime
            .smoke_check()
            .await
            .expect("smoke check query should succeed");

        let mut rows = runtime
            .connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'nodes'",
                (),
            )
            .await
            .expect("schema lookup should succeed");

        assert!(
            rows.next()
                .await
                .expect("rows should be readable")
                .is_some(),
            "nodes table should exist after migrations"
        );
    }

    #[test]
    fn keeps_local_backend_by_default() {
        assert_eq!(select_backend(&StoreConfig::default()), StoreBackend::LocalOnly);
    }

    #[test]
    fn only_selects_synced_backend_when_sync_is_enabled_and_complete() {
        let local_only = StoreConfig {
            sync_enabled: true,
            ..StoreConfig::default()
        };
        assert_eq!(select_backend(&local_only), StoreBackend::LocalOnly);

        let synced = StoreConfig {
            sync_enabled: true,
            turso_database_url: Some("libsql://example-org.turso.io".to_owned()),
            turso_auth_token: Some("secret".to_owned()),
            ..StoreConfig::default()
        };

        match select_backend(&synced) {
            StoreBackend::Synced(sync) => {
                assert_eq!(sync.database_url, "libsql://example-org.turso.io");
                assert_eq!(sync.auth_token, "secret");
                assert!(sync.read_your_writes);
            }
            StoreBackend::LocalOnly => panic!("expected synced backend selection"),
        }
    }
}
