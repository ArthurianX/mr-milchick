use anyhow::{Result, anyhow};

#[derive(Debug, Clone)]
pub struct GitHubConfig {
    pub base_url: String,
    pub token: String,
}

impl GitHubConfig {
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("GITHUB_API_BASE_URL")
            .unwrap_or_else(|_| "https://api.github.com".to_string());

        let token = std::env::var("GITHUB_TOKEN")
            .map_err(|_| anyhow!("missing required environment variable: GITHUB_TOKEN"))?;

        Ok(Self { base_url, token })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubPullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub is_draft: bool,
    pub web_url: String,
    pub author_username: String,
    pub reviewer_usernames: Vec<String>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubChangedFile {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    pub patch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubSnapshotData {
    pub pull_request: GitHubPullRequest,
    pub changed_files: Vec<GitHubChangedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestComment {
    pub id: u64,
    pub body: String,
}
