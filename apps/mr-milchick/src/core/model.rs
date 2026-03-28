#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewPlatformKind {
    GitLab,
    GitHub,
}

impl ReviewPlatformKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GitLab => "gitlab",
            Self::GitHub => "github",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerConfig {
    pub definitions: Vec<ReviewerDefinition>,
    pub max_reviewers: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerDefinition {
    pub username: String,
    pub areas: Vec<crate::core::domain::code_area::CodeArea>,
    pub is_fallback: bool,
    pub is_mandatory: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSinkKind {
    SlackApp,
    SlackWorkflow,
    Teams,
    Discord,
}

impl NotificationSinkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SlackApp => "slack-app",
            Self::SlackWorkflow => "slack-workflow",
            Self::Teams => "teams",
            Self::Discord => "discord",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSnapshot {
    pub review_ref: ReviewRef,
    pub repository: RepositoryRef,
    pub title: String,
    pub description: Option<String>,
    pub author: Actor,
    pub participants: Vec<Actor>,
    pub changed_files: Vec<ChangedFile>,
    pub labels: Vec<String>,
    pub is_draft: bool,
    pub default_branch: Option<String>,
    pub metadata: ReviewMetadata,
}

impl ReviewSnapshot {
    pub fn changed_file_count(&self) -> usize {
        self.changed_files.len()
    }

    pub fn reviewer_usernames(&self) -> Vec<String> {
        self.participants
            .iter()
            .map(|actor| actor.username.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRef {
    pub platform: ReviewPlatformKind,
    pub project_key: String,
    pub review_id: String,
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRef {
    pub platform: ReviewPlatformKind,
    pub namespace: String,
    pub name: String,
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub username: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub change_type: ChangeType,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewMetadata {
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub commit_count: Option<u32>,
    pub approvals_required: Option<u32>,
    pub approvals_given: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    AssignReviewers { reviewers: Vec<Actor> },
    UpsertSummary { markdown: String },
    AddLabels { labels: Vec<String> },
    RemoveLabels { labels: Vec<String> },
    FailPipeline { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewActionKind {
    AssignReviewers,
    UpsertSummary,
    AddLabels,
    RemoveLabels,
    FailPipeline,
}

impl ReviewAction {
    pub fn kind(&self) -> ReviewActionKind {
        match self {
            Self::AssignReviewers { .. } => ReviewActionKind::AssignReviewers,
            Self::UpsertSummary { .. } => ReviewActionKind::UpsertSummary,
            Self::AddLabels { .. } => ReviewActionKind::AddLabels,
            Self::RemoveLabels { .. } => ReviewActionKind::RemoveLabels,
            Self::FailPipeline { .. } => ReviewActionKind::FailPipeline,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationMessage {
    pub sink: NotificationSinkKind,
    pub subject: String,
    pub body: String,
    pub audience: NotificationAudience,
    pub severity: NotificationSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationAudience {
    Default,
    Channel(String),
    User(String),
    Group(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedMessage {
    pub title: Option<String>,
    pub sections: Vec<MessageSection>,
    pub footer: Option<String>,
}

impl RenderedMessage {
    pub fn new(title: Option<String>) -> Self {
        Self {
            title,
            sections: Vec::new(),
            footer: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageSection {
    Paragraph(String),
    BulletList(Vec<String>),
    KeyValueList(Vec<(String, String)>),
    CodeBlock {
        language: Option<String>,
        content: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewActionReport {
    pub applied: Vec<AppliedReviewAction>,
    pub skipped: Vec<SkippedReviewAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedReviewAction {
    pub action: ReviewActionKind,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedReviewAction {
    pub action: ReviewActionKind,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDeliveryReport {
    pub sink: NotificationSinkKind,
    pub delivered: bool,
    pub destination: Option<String>,
    pub detail: Option<String>,
}
