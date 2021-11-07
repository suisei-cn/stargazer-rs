use thiserror::Error;

#[derive(Debug, Error)]
pub enum CrudError {
    #[error("database error: {0}")]
    DBError(#[from] mongodb::error::Error),
    #[error("missing vtuber")]
    MissingVtuber,
    #[error("missing field: {0}")]
    MissingField(&'static str),
    #[error("FATAL: internal inconsistency")]
    Inconsistency,
}
