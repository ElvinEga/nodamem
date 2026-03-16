//! Environment-backed configuration for the local-first store.

use std::env;
use std::path::PathBuf;

const DEFAULT_LOCAL_DATABASE_PATH: &str = "data/nodamem.db";
const DEFAULT_SYNC_ENABLED: bool = false;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreConfig {
    pub local_database_path: PathBuf,
    pub sync_enabled: bool,
    pub turso_database_url: Option<String>,
    pub turso_auth_token: Option<String>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            local_database_path: PathBuf::from(DEFAULT_LOCAL_DATABASE_PATH),
            sync_enabled: DEFAULT_SYNC_ENABLED,
            turso_database_url: None,
            turso_auth_token: None,
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
        }
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
