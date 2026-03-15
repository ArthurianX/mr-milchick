#[derive(Debug, Clone)]
pub struct CiContext {
    pub project_id: ProjectId,
    pub merge_request: Option<MergeRequestRef>,
    pub pipeline: PipelineInfo,
    pub branches: BranchInfo,
    pub labels: Vec<Label>,
}

#[derive(Debug, Clone)]
pub struct ProjectId(pub String);

#[derive(Debug, Clone)]
pub struct MergeRequestRef {
    pub iid: MergeRequestIid,
}

#[derive(Debug, Clone)]
pub struct MergeRequestIid(pub String);

#[derive(Debug, Clone)]
pub struct Label(pub String);

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub source: BranchName,
    pub target: BranchName,
}

#[derive(Debug, Clone)]
pub struct BranchName(pub String);

#[derive(Debug, Clone)]
pub struct PipelineInfo {
    pub source: PipelineSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineSource {
    MergeRequestEvent,
    Push,
    Schedule,
    Unknown,
}