use anyhow::Result;
use thiserror::Error;

pub type PipelineResult<T> = Result<T, PipelineError>;

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("")]
    Aggregated(Vec<PipelineError>),
    #[error(transparent)]
    Any(#[from] anyhow::Error),
}
