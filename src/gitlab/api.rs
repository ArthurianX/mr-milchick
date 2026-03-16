use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct GitLabConfig {
    pub base_url: String,
    pub token: String,
}

impl GitLabConfig {
    pub fn from_env() -> Result<Self> {
        let base_url =
            std::env::var("GITLAB_BASE_URL").unwrap_or_else(|_| "https://gitlab.com/api/v4".to_string());

        let token = std::env::var("GITLAB_TOKEN")
            .map_err(|_| anyhow!("missing required environment variable: GITLAB_TOKEN"))?;

        Ok(Self { base_url, token })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRequestDetails {
    pub iid: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: MergeRequestState,
    pub is_draft: bool,
    pub web_url: String,
    pub author_username: String,
    pub reviewer_usernames: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeRequestState {
    Opened,
    Closed,
    Locked,
    Merged,
    Unknown(String),
}

impl MergeRequestState {
    pub fn as_str(&self) -> &str {
        match self {
            MergeRequestState::Opened => "opened",
            MergeRequestState::Closed => "closed",
            MergeRequestState::Locked => "locked",
            MergeRequestState::Merged => "merged",
            MergeRequestState::Unknown(value) => value.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub old_path: String,
    pub new_path: String,
    pub is_new: bool,
    pub is_renamed: bool,
    pub is_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRequestSnapshot {
    pub details: MergeRequestDetails,
    pub changed_files: Vec<ChangedFile>,
}

impl MergeRequestSnapshot {
    pub fn changed_file_count(&self) -> usize {
        self.changed_files.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRequestNote {
    pub id: u64,
    pub body: String,
}