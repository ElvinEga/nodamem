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

        let backend = select_backend(&config);
        let database = open_database_with_optional_sync(&config, &backend).await?;
        let connection = database.connect()?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .await?;
        if should_run_local_migrations(&backend) {
            run_migrations(&connection).await?;
        } else {
            info!(
                local_database_path = %config.local_database_path.display(),
                "skipping local schema bootstrap because synced storage mode was selected"
            );
        }

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

async fn open_database_with_optional_sync(
    config: &StoreConfig,
    backend: &StoreBackend,
) -> Result<Database, StoreError> {
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
            try_open_synced_database(config, sync_config).await
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

fn should_run_local_migrations(backend: &StoreBackend) -> bool {
    matches!(backend, StoreBackend::LocalOnly)
}

async fn try_open_synced_database(
    config: &StoreConfig,
    sync_config: &TursoSyncConfig,
) -> Result<Database, StoreError> {
    info!(
        local_database_path = %config.local_database_path.display(),
        turso_database_url = %sync_config.database_url,
        read_your_writes = sync_config.read_your_writes,
        "opening local database with optional Turso sync enabled"
    );
    let synced_result = Builder::new_synced_database(
        &config.local_database_path,
        sync_config.database_url.clone(),
        sync_config.auth_token.clone(),
    )
    .read_your_writes(sync_config.read_your_writes)
    .build()
    .await
    .map_err(StoreError::from);

    match handle_synced_open_result(config, sync_config, synced_result) {
        Ok(database) => Ok(database),
        Err(error) if !config.turso_sync_required => {
            info!(
                local_database_path = %config.local_database_path.display(),
                "falling back to local embedded libsql database"
            );
            Builder::new_local(&config.local_database_path)
                .build()
                .await
                .map_err(StoreError::from)
        }
        Err(error) => Err(error),
    }
}

fn handle_synced_open_result<T>(
    config: &StoreConfig,
    sync_config: &TursoSyncConfig,
    synced_result: Result<T, StoreError>,
) -> Result<T, StoreError> {
    match synced_result {
        Ok(value) => Ok(value),
        Err(error) if !config.turso_sync_required => {
            warn!(
                local_database_path = %config.local_database_path.display(),
                turso_database_url = %sync_config.database_url,
                error = %error,
                "failed to initialize Turso sync; continuing in local-only mode"
            );
            Err(StoreError::InvalidValue {
                field: "sync-fallback",
                value: "local-fallback".to_owned(),
            })
        }
        Err(error) => Err(error),
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

    use super::{
        handle_synced_open_result, select_backend, should_run_local_migrations, StoreBackend,
        StoreRuntime,
    };
    use crate::config::StoreConfig;
    use crate::error::StoreError;

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
        assert!(should_run_local_migrations(&StoreBackend::LocalOnly));
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
                assert!(!should_run_local_migrations(&StoreBackend::Synced(sync.clone())));
            }
            StoreBackend::LocalOnly => panic!("expected synced backend selection"),
        }
    }

    #[tokio::test]
    async fn falls_back_to_local_when_sync_is_optional() {
        let config = StoreConfig {
            sync_enabled: true,
            turso_database_url: Some("libsql://example-org.turso.io".to_owned()),
            turso_auth_token: Some("secret".to_owned()),
            ..StoreConfig::default()
        };
        let sync_config = config
            .turso_sync_config()
            .expect("sync configuration should be complete");

        let result = handle_synced_open_result::<()>(
            &config,
            &sync_config,
            Err(StoreError::InvalidValue {
                field: "sync",
                value: "boom".to_owned(),
            }),
        )
        .expect_err("optional sync should trigger local fallback marker");

        match result {
            StoreError::InvalidValue { field, value } => {
                assert_eq!(field, "sync-fallback");
                assert_eq!(value, "local-fallback");
            }
            other => panic!("unexpected fallback marker: {other}"),
        }
    }

    #[tokio::test]
    async fn returns_error_when_sync_is_required() {
        let config = StoreConfig {
            sync_enabled: true,
            turso_sync_required: true,
            turso_database_url: Some("libsql://example-org.turso.io".to_owned()),
            turso_auth_token: Some("secret".to_owned()),
            ..StoreConfig::default()
        };
        let sync_config = config
            .turso_sync_config()
            .expect("sync configuration should be complete");

        let error = handle_synced_open_result::<()>(
            &config,
            &sync_config,
            Err(StoreError::InvalidValue {
                field: "sync",
                value: "boom".to_owned(),
            }),
        )
        .expect_err("required sync should not fall back");

        match error {
            StoreError::InvalidValue { field, value } => {
                assert_eq!(field, "sync");
                assert_eq!(value, "boom");
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
