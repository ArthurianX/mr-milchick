use anyhow::Result;

use crate::context::model::*;
use crate::context::raw::RawCiEnv;
use crate::error::AppError;

pub fn build_ci_context() -> Result<CiContext> {
    let raw = RawCiEnv::load();

    let project_id = raw
        .project_id
        .ok_or(AppError::MissingEnvVar("CI_PROJECT_ID"))
        .map(ProjectId)?;

    let pipeline_source = parse_pipeline_source(raw.pipeline_source);

    let merge_request = raw
        .merge_request_iid
        .map(|iid| MergeRequestRef {
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

fn parse_pipeline_source(src: Option<String>) -> PipelineSource {
    match src.as_deref() {
        Some("merge_request_event") => PipelineSource::MergeRequestEvent,
        Some("push") => PipelineSource::Push,
        Some("schedule") => PipelineSource::Schedule,
        _ => PipelineSource::Unknown,
    }
}