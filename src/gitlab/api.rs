#[derive(Debug, Clone)]
pub struct GitLabConfig {
    pub base_url: String,
    pub token: String,
}

impl GitLabConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let base_url = std::env::var("GITLAB_BASE_URL")
            .unwrap_or_else(|_| "https://gitlab.com/api/v4".to_string());

        let token = std::env::var("GITLAB_TOKEN")
            .map_err(|_| anyhow::anyhow!("missing required environment variable: GITLAB_TOKEN"))?;

        Ok(Self { base_url, token })
    }
}