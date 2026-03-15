#[derive(Debug, Clone)]
pub struct CiContext {
    pub project_id: ProjectId,
    pub merge_request: Option<MergeRequestRef>,
    pub pipeline: PipelineInfo,
    pub branches: BranchInfo,
    pub labels: Vec<Label>,
}

impl CiContext {
    pub fn is_merge_request_pipeline(&self) -> bool {
        self.pipeline.source == PipelineSource::MergeRequestEvent && self.merge_request.is_some()
    }

    pub fn merge_request_iid(&self) -> Option<&str> {
        self.merge_request.as_ref().map(|mr| mr.iid.0.as_str())
    }

    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l.0 == label)
    }

    pub fn source_branch(&self) -> &str {
        self.branches.source.0.as_str()
    }

    pub fn target_branch(&self) -> &str {
        self.branches.target.0.as_str()
    }

    pub fn source_branch_kind(&self) -> BranchKind {
        BranchKind::from_branch_name(self.source_branch())
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    Epic,
    Feature,
    Fix,
    Chore,
    Other,
}

impl BranchKind {
    pub fn from_branch_name(name: &str) -> Self {
        if name.starts_with("epic/") {
            Self::Epic
        } else if name.starts_with("feat/") {
            Self::Feature
        } else if name.starts_with("fix/") {
            Self::Fix
        } else if name.starts_with("chore/") {
            Self::Chore
        } else {
            Self::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> CiContext {
        CiContext {
            project_id: ProjectId("123".to_string()),
            merge_request: Some(MergeRequestRef {
                iid: MergeRequestIid("456".to_string()),
            }),
            pipeline: PipelineInfo {
                source: PipelineSource::MergeRequestEvent,
            },
            branches: BranchInfo {
                source: BranchName("epic/test-thing".to_string()),
                target: BranchName("develop".to_string()),
            },
            labels: vec![
                Label("backend".to_string()),
                Label("qa-required".to_string()),
            ],
        }
    }

    #[test]
    fn detects_merge_request_pipeline() {
        let ctx = sample_context();
        assert!(ctx.is_merge_request_pipeline());
    }

    #[test]
    fn finds_merge_request_iid() {
        let ctx = sample_context();
        assert_eq!(ctx.merge_request_iid(), Some("456"));
    }

    #[test]
    fn checks_labels() {
        let ctx = sample_context();
        assert!(ctx.has_label("qa-required"));
        assert!(!ctx.has_label("missing-label"));
    }

    #[test]
    fn classifies_epic_branch() {
        let ctx = sample_context();
        assert_eq!(ctx.source_branch_kind(), BranchKind::Epic);
    }

    #[test]
    fn classifies_other_branch() {
        assert_eq!(BranchKind::from_branch_name("hotfix/urgent"), BranchKind::Other);
    }

    #[test]
    fn detects_non_mr_pipeline_when_mr_is_missing() {
        let mut ctx = sample_context();
        ctx.merge_request = None;
        assert!(!ctx.is_merge_request_pipeline());
    }
}