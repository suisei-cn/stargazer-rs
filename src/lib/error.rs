use std::result::Result as StdResult;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("context: no such worker in context")]
    Context,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("parse error - {0}")]
    Parse(#[from] config::ConfigError),
    #[error("missing configuration")]
    Missing,
}

pub type Result<T> = StdResult<T, Error>;
