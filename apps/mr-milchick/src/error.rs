use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("missing required environment variable: {0}")]
    MissingEnvVar(&'static str),

    #[error("missing required review context: {0}")]
    MissingReviewContext(&'static str),
}
