//! Database bootstrap for local-first libSQL storage.

use std::error::Error as StdError;
use std::fmt;
use std::path::Path;

use libsql::{Connection, Database};

use crate::config::StoreConfig;

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Libsql(libsql::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "io error: {error}"),
            Self::Libsql(error) => write!(formatter, "libsql error: {error}"),
        }
    }
}

impl StdError for StoreError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Libsql(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<libsql::Error> for StoreError {
    fn from(error: libsql::Error) -> Self {
        Self::Libsql(error)
    }
}

#[derive(Debug)]
pub struct StoreRuntime {
    pub config: StoreConfig,
    pub database: Database,
    pub connection: Connection,
}

impl StoreRuntime {
    pub async fn open(config: StoreConfig) -> Result<Self, StoreError> {
        ensure_parent_dir(&config.local_database_path).await?;

        let database = Database::open(config.local_database_path.clone())?;
        let connection = database.connect()?;

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
    }
}
