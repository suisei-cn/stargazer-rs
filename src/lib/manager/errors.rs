use actix_web::http::StatusCode;
use actix_web::ResponseError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CrudError {
    #[error("database error: {0}")]
    DBError(#[from] mongodb::error::Error),
    #[error("missing vtuber")]
    MissingVtuber,
    #[error("missing field")]
    MissingField,
    #[error("FATAL: internal inconsistency")]
    Inconsistency,
}

impl ResponseError for CrudError {
    fn status_code(&self) -> StatusCode {
        match self {
            CrudError::MissingVtuber | CrudError::MissingField => StatusCode::NOT_FOUND,
            CrudError::DBError(_) | CrudError::Inconsistency => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
