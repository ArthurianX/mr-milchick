use std::time::Duration;

use async_trait::async_trait;
use tokio::time::timeout;

use crate::core::model::ReviewSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewInferenceStatus {
    Disabled,
    Ready,
    TimedOut,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewInsights {
    pub summary: Option<String>,
    pub recommendations: Vec<ReviewRecommendation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRecommendation {
    pub category: RecommendationCategory,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendationCategory {
    ReviewFocus,
    Risk,
    TestGap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewInferenceOutcome {
    pub status: ReviewInferenceStatus,
    pub insights: ReviewInsights,
    pub detail: Option<String>,
}

impl ReviewInferenceOutcome {
    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            status: ReviewInferenceStatus::Disabled,
            insights: ReviewInsights::default(),
            detail: Some(reason.into()),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            status: ReviewInferenceStatus::Unavailable,
            insights: ReviewInsights::default(),
            detail: Some(reason.into()),
        }
    }

    pub fn ready(insights: ReviewInsights) -> Self {
        Self {
            status: ReviewInferenceStatus::Ready,
            insights,
            detail: None,
        }
    }

    pub fn failed(reason: impl Into<String>) -> Self {
        Self {
            status: ReviewInferenceStatus::Failed,
            insights: ReviewInsights::default(),
            detail: Some(reason.into()),
        }
    }

    pub fn timed_out(timeout: Duration) -> Self {
        Self {
            status: ReviewInferenceStatus::TimedOut,
            insights: ReviewInsights::default(),
            detail: Some(format!(
                "review inference timed out after {} ms",
                timeout.as_millis()
            )),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewInferenceError {
    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("analysis failed: {0}")]
    Analysis(String),
}

#[async_trait]
pub trait ReviewInferenceEngine: Send + Sync {
    async fn analyze(
        &self,
        snapshot: &ReviewSnapshot,
    ) -> Result<ReviewInferenceOutcome, ReviewInferenceError>;
}

#[derive(Debug, Clone)]
pub struct NoopReviewInferenceEngine {
    outcome: ReviewInferenceOutcome,
}

impl NoopReviewInferenceEngine {
    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            outcome: ReviewInferenceOutcome::disabled(reason),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            outcome: ReviewInferenceOutcome::unavailable(reason),
        }
    }
}

#[async_trait]
impl ReviewInferenceEngine for NoopReviewInferenceEngine {
    async fn analyze(
        &self,
        _snapshot: &ReviewSnapshot,
    ) -> Result<ReviewInferenceOutcome, ReviewInferenceError> {
        Ok(self.outcome.clone())
    }
}

pub async fn analyze_with_timeout(
    engine: &(dyn ReviewInferenceEngine + Send + Sync),
    snapshot: &ReviewSnapshot,
    timeout_duration: Duration,
) -> ReviewInferenceOutcome {
    match timeout(timeout_duration, engine.analyze(snapshot)).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(err)) => ReviewInferenceOutcome::failed(err.to_string()),
        Err(_) => ReviewInferenceOutcome::timed_out(timeout_duration),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        Actor, ChangeType, ChangedFile, RepositoryRef, ReviewMetadata, ReviewPlatformKind,
        ReviewRef,
    };

    fn sample_snapshot() -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitLab,
                project_key: "123".to_string(),
                review_id: "456".to_string(),
                web_url: None,
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitLab,
                namespace: "group".to_string(),
                name: "project".to_string(),
                web_url: None,
            },
            title: "Test".to_string(),
            description: None,
            author: Actor {
                username: "alice".to_string(),
                display_name: None,
            },
            participants: Vec::new(),
            changed_files: vec![ChangedFile {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                change_type: ChangeType::Modified,
                additions: Some(12),
                deletions: Some(3),
                patch: Some("@@ -1,2 +1,2 @@".to_string()),
            }],
            labels: Vec::new(),
            is_draft: false,
            default_branch: Some("main".to_string()),
            metadata: ReviewMetadata::default(),
        }
    }

    struct SleepyEngine;

    #[async_trait]
    impl ReviewInferenceEngine for SleepyEngine {
        async fn analyze(
            &self,
            _snapshot: &ReviewSnapshot,
        ) -> Result<ReviewInferenceOutcome, ReviewInferenceError> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(ReviewInferenceOutcome::ready(ReviewInsights::default()))
        }
    }

    #[tokio::test]
    async fn noop_engine_returns_configured_disabled_outcome() {
        let engine = NoopReviewInferenceEngine::disabled("disabled by configuration");
        let outcome = engine
            .analyze(&sample_snapshot())
            .await
            .expect("noop engine should not fail");

        assert_eq!(outcome.status, ReviewInferenceStatus::Disabled);
        assert_eq!(outcome.detail.as_deref(), Some("disabled by configuration"));
    }

    #[tokio::test]
    async fn analyze_with_timeout_reports_timeout_when_engine_takes_too_long() {
        let outcome =
            analyze_with_timeout(&SleepyEngine, &sample_snapshot(), Duration::from_millis(5)).await;

        assert_eq!(outcome.status, ReviewInferenceStatus::TimedOut);
        assert!(
            outcome
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("timed out")
        );
    }
}
