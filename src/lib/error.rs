use std::result::Result as StdResult;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("config: {0}")]
    Config(#[from] ConfigError),
}

impl From<config::ConfigError> for Error {
    fn from(e: config::ConfigError) -> Self {
        Self::Config(e.into())
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("parse error - {0}")]
    Parse(#[from] config::ConfigError),
    #[error("missing configuration")]
    Missing,
}

pub type Result<T> = StdResult<T, Error>;
