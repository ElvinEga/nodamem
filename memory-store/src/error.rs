//! Error types for the Nodamem storage layer.

use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Libsql(libsql::Error),
    SerdeJson(serde_json::Error),
    Uuid(uuid::Error),
    Chrono(chrono::ParseError),
    InvalidValue { field: &'static str, value: String },
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "io error: {error}"),
            Self::Libsql(error) => write!(formatter, "libsql error: {error}"),
            Self::SerdeJson(error) => write!(formatter, "json error: {error}"),
            Self::Uuid(error) => write!(formatter, "uuid parse error: {error}"),
            Self::Chrono(error) => write!(formatter, "timestamp parse error: {error}"),
            Self::InvalidValue { field, value } => {
                write!(formatter, "invalid value for {field}: {value}")
            }
        }
    }
}

impl StdError for StoreError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Libsql(error) => Some(error),
            Self::SerdeJson(error) => Some(error),
            Self::Uuid(error) => Some(error),
            Self::Chrono(error) => Some(error),
            Self::InvalidValue { .. } => None,
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

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::SerdeJson(error)
    }
}

impl From<uuid::Error> for StoreError {
    fn from(error: uuid::Error) -> Self {
        Self::Uuid(error)
    }
}

impl From<chrono::ParseError> for StoreError {
    fn from(error: chrono::ParseError) -> Self {
        Self::Chrono(error)
    }
}
