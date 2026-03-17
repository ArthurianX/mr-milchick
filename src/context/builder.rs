use anyhow::Result;

use crate::context::model::*;
use crate::context::raw::RawCiEnv;
use crate::error::AppError;

pub fn build_ci_context() -> Result<CiContext> {
    build_ci_context_from(RawCiEnv::load())
}

pub fn build_ci_context_from(raw: RawCiEnv) -> Result<CiContext> {
    let project_id = raw
        .project_id
        .ok_or(AppError::MissingEnvVar("CI_PROJECT_ID"))
        .map(ProjectId)?;

    let pipeline_source = parse_pipeline_source(raw.pipeline_source);

    let merge_request = raw.merge_request_iid.map(|iid| MergeRequestRef {
        iid: MergeRequestIid(iid),
    });

    let branches = BranchInfo {
        source: BranchName(raw.source_branch.unwrap_or_default()),
        target: BranchName(raw.target_branch.unwrap_or_default()),
    };

    let labels = raw
        .labels
        .unwrap_or_default()
        .split(',')
        .filter(|l| !l.trim().is_empty())
        .map(|l| Label(l.trim().to_string()))
        .collect();

    Ok(CiContext {
        project_id,
        merge_request,
        pipeline: PipelineInfo {
            source: pipeline_source,
        },
        branches,
        labels,
    })
}

pub fn parse_pipeline_source(src: Option<String>) -> PipelineSource {
    match src.as_deref() {
        Some("merge_request_event") => PipelineSource::MergeRequestEvent,
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
            PipelineSource::MergeRequestEvent
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
    fn builds_context_without_merge_request() {
        let raw = RawCiEnv {
            project_id: Some("123".to_string()),
            merge_request_iid: None,
            pipeline_source: Some("push".to_string()),
            source_branch: Some("feat/test".to_string()),
            target_branch: Some("develop".to_string()),
            labels: Some("backend, needs-review".to_string()),
        };

        let ctx = build_ci_context_from(raw).expect("context should build");

        assert_eq!(ctx.project_id.0, "123");
        assert!(ctx.merge_request.is_none());
        assert_eq!(ctx.pipeline.source, PipelineSource::Push);
        assert_eq!(ctx.branches.source.0, "feat/test");
        assert_eq!(ctx.branches.target.0, "develop");
        assert_eq!(ctx.labels.len(), 2);
        assert_eq!(ctx.labels[0].0, "backend");
        assert_eq!(ctx.labels[1].0, "needs-review");
    }

    #[test]
    fn missing_project_id_fails() {
        let raw = RawCiEnv {
            project_id: None,
            merge_request_iid: Some("456".to_string()),
            pipeline_source: Some("merge_request_event".to_string()),
            source_branch: Some("feat/test".to_string()),
            target_branch: Some("develop".to_string()),
            labels: None,
        };

        let err = build_ci_context_from(raw).expect_err("should fail without project id");
        assert!(err.to_string().contains("CI_PROJECT_ID"));
    }
}
