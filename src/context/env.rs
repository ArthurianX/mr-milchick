use anyhow::Result;

use crate::context::model::CiContext;
use crate::error::AppError;

pub fn load_ci_context() -> Result<CiContext> {
    let project_id = read_required("CI_PROJECT_ID")?;
    let merge_request_iid = read_required("CI_MERGE_REQUEST_IID")?;
    let pipeline_source = read_required("CI_PIPELINE_SOURCE")?;
    let source_branch = read_required("CI_MERGE_REQUEST_SOURCE_BRANCH_NAME")?;
    let target_branch = read_required("CI_MERGE_REQUEST_TARGET_BRANCH_NAME")?;
    let labels_raw = std::env::var("CI_MERGE_REQUEST_LABELS").unwrap_or_default();

    let labels = labels_raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    Ok(CiContext {
        project_id,
        merge_request_iid,
        pipeline_source,
        source_branch,
        target_branch,
        labels,
    })
}

fn read_required(name: &'static str) -> Result<String> {
    std::env::var(name).map_err(|_| AppError::MissingEnvVar(name).into())
}