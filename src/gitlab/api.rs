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
