//! Environment-backed configuration for the local-first store.

use std::env;
use std::path::PathBuf;

const DEFAULT_LOCAL_DATABASE_PATH: &str = "data/nodamem.db";
const DEFAULT_SYNC_ENABLED: bool = false;
const DEFAULT_TURSO_READ_YOUR_WRITES: bool = true;

/// Optional Turso Cloud sync configuration.
///
/// Nodamem keeps local embedded storage as the default. This struct only carries the remote sync
/// settings needed at the storage bootstrap boundary so core memory logic remains unaware of sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TursoSyncConfig {
    pub database_url: String,
    pub auth_token: String,
    pub read_your_writes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreConfig {
    pub local_database_path: PathBuf,
    pub sync_enabled: bool,
    pub turso_database_url: Option<String>,
    pub turso_auth_token: Option<String>,
    pub turso_read_your_writes: bool,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            local_database_path: PathBuf::from(DEFAULT_LOCAL_DATABASE_PATH),
            sync_enabled: DEFAULT_SYNC_ENABLED,
            turso_database_url: None,
            turso_auth_token: None,
            turso_read_your_writes: DEFAULT_TURSO_READ_YOUR_WRITES,
        }
    }
}

impl StoreConfig {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            local_database_path: env::var_os("NODAMEM_DB_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(DEFAULT_LOCAL_DATABASE_PATH)),
            sync_enabled: env::var("NODAMEM_TURSO_SYNC_ENABLED")
                .ok()
                .and_then(|value| parse_bool(&value))
                .unwrap_or(DEFAULT_SYNC_ENABLED),
            turso_database_url: read_optional_env("TURSO_DATABASE_URL"),
            turso_auth_token: read_optional_env("TURSO_AUTH_TOKEN"),
            turso_read_your_writes: env::var("NODAMEM_TURSO_READ_YOUR_WRITES")
                .ok()
                .and_then(|value| parse_bool(&value))
                .unwrap_or(DEFAULT_TURSO_READ_YOUR_WRITES),
        }
    }

    #[must_use]
    pub fn turso_sync_config(&self) -> Option<TursoSyncConfig> {
        if !self.sync_enabled {
            return None;
        }

        Some(TursoSyncConfig {
            database_url: self.turso_database_url.clone()?,
            auth_token: self.turso_auth_token.clone()?,
            read_your_writes: self.turso_read_your_writes,
        })
    }

    #[must_use]
    pub fn sync_requested_without_credentials(&self) -> bool {
        self.sync_enabled
            && (self.turso_database_url.is_none() || self.turso_auth_token.is_none())
    }
}

fn read_optional_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{StoreConfig, TursoSyncConfig};

    #[test]
    fn defaults_to_local_only_mode() {
        let config = StoreConfig::default();

        assert!(!config.sync_enabled);
        assert!(config.turso_sync_config().is_none());
    }

    #[test]
    fn exposes_turso_sync_config_only_when_enabled_and_complete() {
        let config = StoreConfig {
            sync_enabled: true,
            turso_database_url: Some("libsql://example-org.turso.io".to_owned()),
            turso_auth_token: Some("secret".to_owned()),
            turso_read_your_writes: false,
            ..StoreConfig::default()
        };

        assert_eq!(
            config.turso_sync_config(),
            Some(TursoSyncConfig {
                database_url: "libsql://example-org.turso.io".to_owned(),
                auth_token: "secret".to_owned(),
                read_your_writes: false,
            })
        );
    }

    #[test]
    fn flags_incomplete_sync_configuration() {
        let config = StoreConfig {
            sync_enabled: true,
            turso_database_url: Some("libsql://example-org.turso.io".to_owned()),
            ..StoreConfig::default()
        };

        assert!(config.sync_requested_without_credentials());
        assert!(config.turso_sync_config().is_none());
    }
}
