#[derive(Debug, Clone)]
pub struct CiContext {
    pub project_id: String,
    pub merge_request_iid: String,
    pub pipeline_source: String,
    pub source_branch: String,
    pub target_branch: String,
    pub labels: Vec<String>,
}