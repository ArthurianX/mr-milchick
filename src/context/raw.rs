use std::env;

#[derive(Debug)]
pub struct RawCiEnv {
    pub project_id: Option<String>,
    pub merge_request_iid: Option<String>,
    pub pipeline_source: Option<String>,
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub labels: Option<String>,
}

impl RawCiEnv {
    pub fn load() -> Self {
        Self {
            project_id: env::var("CI_PROJECT_ID").ok(),
            merge_request_iid: env::var("CI_MERGE_REQUEST_IID").ok(),
            pipeline_source: env::var("CI_PIPELINE_SOURCE").ok(),
            source_branch: env::var("CI_MERGE_REQUEST_SOURCE_BRANCH_NAME").ok(),
            target_branch: env::var("CI_MERGE_REQUEST_TARGET_BRANCH_NAME").ok(),
            labels: env::var("CI_MERGE_REQUEST_LABELS").ok(),
        }
    }
}
