use thiserror::Error;

use payplan_core::error::CoreError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("core: {0}")]
    Core(#[from] CoreError),

    #[error("validation: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("infra: {0}")]
    Infra(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;
