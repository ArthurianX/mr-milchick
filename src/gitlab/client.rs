use anyhow::{Context, Result};
use reqwest::Client;

use crate::gitlab::api::GitLabConfig;
use crate::gitlab::dto::MergeRequestDto;

#[derive(Debug, Clone)]
pub struct GitLabClient {
    http: Client,
    config: GitLabConfig,
}

impl GitLabClient {
    pub fn new(config: GitLabConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    pub async fn get_merge_request(
        &self,
        project_id: &str,
        merge_request_iid: &str,
    ) -> Result<MergeRequestDto> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}",
            self.config.base_url.trim_end_matches('/'),
            project_id,
            merge_request_iid
        );

        let response = self
            .http
            .get(url)
            .header("PRIVATE-TOKEN", &self.config.token)
            .send()
            .await
            .context("failed to send request to GitLab")?;

        let response = response
            .error_for_status()
            .context("GitLab returned an error status while fetching merge request")?;

        let merge_request = response
            .json::<MergeRequestDto>()
            .await
            .context("failed to deserialize merge request response")?;

        Ok(merge_request)
    }
}