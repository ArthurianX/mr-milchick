use anyhow::Result;

use crate::context::model::*;
use crate::context::raw::RawCiEnv;
use crate::error::AppError;

pub fn build_ci_context() -> Result<CiContext> {
    build_ci_context_from(RawCiEnv::load())
}

pub fn build_ci_context_from(raw: RawCiEnv) -> Result<CiContext> {
    let project_key = raw
        .project_key
        .ok_or(AppError::MissingReviewContext(
            "project key (CI_PROJECT_ID, GITHUB_REPOSITORY, or MR_MILCHICK_PROJECT_KEY)",
        ))
        .map(ProjectKey)?;

    let pipeline_source = parse_pipeline_source(raw.pipeline_source);

    let review = raw
        .review_id
        .map(|id| ReviewContextRef { id: ReviewId(id) });

    let source_branch = raw
        .source_branch
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_default();
    let target_branch = raw
        .target_branch
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_default();

    let labels = raw
        .labels
        .unwrap_or_default()
        .split(',')
        .filter_map(|label| {
            let label = label.trim();
            (!label.is_empty()).then(|| Label(label.to_string()))
        })
        .collect();

    Ok(CiContext {
        project_key,
        review,
        pipeline: PipelineInfo {
            source: pipeline_source,
        },
        branches: BranchInfo {
            source: BranchName(source_branch),
            target: BranchName(target_branch),
        },
        labels,
    })
}

pub fn parse_pipeline_source(src: Option<String>) -> PipelineSource {
    match src.as_deref() {
        Some("merge_request_event" | "pull_request" | "pull_request_target") => {
            PipelineSource::ReviewEvent
        }
        Some("push") => PipelineSource::Push,
        Some("schedule") => PipelineSource::Schedule,
        _ => PipelineSource::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_pipeline_sources() {
        assert_eq!(
            parse_pipeline_source(Some("merge_request_event".to_string())),
            PipelineSource::ReviewEvent
        );
        assert_eq!(
            parse_pipeline_source(Some("pull_request".to_string())),
            PipelineSource::ReviewEvent
        );
        assert_eq!(
            parse_pipeline_source(Some("push".to_string())),
            PipelineSource::Push
        );
        assert_eq!(
            parse_pipeline_source(Some("schedule".to_string())),
            PipelineSource::Schedule
        );
    }

    #[test]
    fn unknown_pipeline_source_becomes_unknown() {
        assert_eq!(
            parse_pipeline_source(Some("web".to_string())),
            PipelineSource::Unknown
        );
        assert_eq!(parse_pipeline_source(None), PipelineSource::Unknown);
    }

    #[test]
    fn builds_context_without_review() {
        let raw = RawCiEnv {
            project_key: Some("123".to_string()),
            review_id: None,
            pipeline_source: Some("push".to_string()),
            source_branch: Some("feat/test".to_string()),
            target_branch: Some("develop".to_string()),
            labels: Some("backend, needs-review".to_string()),
        };

        let ctx = build_ci_context_from(raw).expect("context should build");

        assert_eq!(ctx.project_key.0, "123");
        assert!(ctx.review.is_none());
        assert_eq!(ctx.pipeline.source, PipelineSource::Push);
        assert_eq!(ctx.branches.source.0, "feat/test");
        assert_eq!(ctx.branches.target.0, "develop");
        assert_eq!(ctx.labels.len(), 2);
        assert_eq!(ctx.labels[0].0, "backend");
        assert_eq!(ctx.labels[1].0, "needs-review");
    }

    #[test]
    fn missing_project_key_fails() {
        let raw = RawCiEnv {
            project_key: None,
            review_id: Some("456".to_string()),
            pipeline_source: Some("merge_request_event".to_string()),
            source_branch: Some("feat/test".to_string()),
            target_branch: Some("develop".to_string()),
            labels: None,
        };

        let err = build_ci_context_from(raw).expect_err("should fail without project key");
        assert!(err.to_string().contains("project key"));
    }
}
