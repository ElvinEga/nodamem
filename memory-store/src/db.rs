//! Database bootstrap for local-first libSQL storage.

use std::path::Path;

use libsql::{Connection, Database};

use crate::config::StoreConfig;
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

        let database_path = config.local_database_path.to_string_lossy().into_owned();
        let database = Database::open(database_path)?;
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
        self.connection.execute("SELECT 1", ()).await?;
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

    use super::StoreRuntime;
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
}
