use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("invariant violated: {0}")]
    Invariant(String),

    #[error("invalid id: {0}")]
    InvalidId(String),
}

pub type CoreResult<T> = Result<T, CoreError>;
