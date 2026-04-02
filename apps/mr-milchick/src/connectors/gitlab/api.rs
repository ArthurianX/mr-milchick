use anyhow::{Result, anyhow};

#[derive(Debug, Clone)]
pub struct GitLabConfig {
    pub base_url: String,
    pub token: String,
}

impl GitLabConfig {
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("GITLAB_BASE_URL")
            .unwrap_or_else(|_| "https://gitlab.com/api/v4".to_string());

        let token = std::env::var("GITLAB_TOKEN")
            .map_err(|_| anyhow!("missing required environment variable: GITLAB_TOKEN"))?;

        Ok(Self { base_url, token })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabMergeRequest {
    pub iid: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub is_draft: bool,
    pub web_url: String,
    pub author_username: String,
    pub reviewer_usernames: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabChangedFile {
    pub old_path: String,
    pub new_path: String,
    pub is_new: bool,
    pub is_renamed: bool,
    pub is_deleted: bool,
    pub patch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabSnapshotData {
    pub merge_request: GitLabMergeRequest,
    pub changed_files: Vec<GitLabChangedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRequestNote {
    pub id: u64,
    pub body: String,
}
