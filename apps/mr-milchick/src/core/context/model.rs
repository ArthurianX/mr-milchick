#[derive(Debug, Clone)]
pub struct CiContext {
    pub project_key: ProjectKey,
    pub review: Option<ReviewContextRef>,
    pub pipeline: PipelineInfo,
    pub branches: BranchInfo,
    pub labels: Vec<Label>,
}

impl CiContext {
    pub fn is_review_pipeline(&self) -> bool {
        self.pipeline.source == PipelineSource::ReviewEvent && self.review.is_some()
    }

    pub fn review_id(&self) -> Option<&str> {
        self.review.as_ref().map(|review| review.id.0.as_str())
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

    pub fn project_key(&self) -> &str {
        self.project_key.0.as_str()
    }
}

#[derive(Debug, Clone)]
pub struct ProjectKey(pub String);

#[derive(Debug, Clone)]
pub struct ReviewContextRef {
    pub id: ReviewId,
}

#[derive(Debug, Clone)]
pub struct ReviewId(pub String);

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
    ReviewEvent,
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
            project_key: ProjectKey("123".to_string()),
            review: Some(ReviewContextRef {
                id: ReviewId("456".to_string()),
            }),
            pipeline: PipelineInfo {
                source: PipelineSource::ReviewEvent,
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
    fn detects_review_pipeline() {
        let ctx = sample_context();
        assert!(ctx.is_review_pipeline());
    }

    #[test]
    fn finds_review_id() {
        let ctx = sample_context();
        assert_eq!(ctx.review_id(), Some("456"));
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
        assert_eq!(
            BranchKind::from_branch_name("hotfix/urgent"),
            BranchKind::Other
        );
    }

    #[test]
    fn detects_non_review_pipeline_when_review_is_missing() {
        let mut ctx = sample_context();
        ctx.review = None;
        assert!(!ctx.is_review_pipeline());
    }
}
