use anyhow::{Context, Result};
use reqwest::{Client, Method, RequestBuilder};
use tracing::{debug, instrument};

use super::api::{
    GitHubChangedFile, GitHubConfig, GitHubPullRequest, GitHubSnapshotData, PullRequestComment,
};
use super::dto::{IssueCommentDto, PullRequestDto, PullRequestFileDto};

const PAGE_SIZE: usize = 100;

#[derive(Debug, Clone)]
pub struct GitHubClient {
    http: Client,
    config: GitHubConfig,
}

impl GitHubClient {
    pub fn new(config: GitHubConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    #[instrument(skip(self), fields(project_key = %project_key, review_id = %review_id))]
    pub async fn get_pull_request(
        &self,
        project_key: &str,
        review_id: &str,
    ) -> Result<GitHubPullRequest> {
        let url = self.pull_request_url(project_key, review_id);

        let dto = self
            .request(Method::GET, url)
            .send()
            .await
            .context("failed to send request to GitHub")?
            .error_for_status()
            .context("GitHub returned an error status while fetching pull request")?
            .json::<PullRequestDto>()
            .await
            .context("failed to deserialize pull request response")?;

        Ok(map_pull_request(dto))
    }

    #[instrument(skip(self), fields(project_key = %project_key, review_id = %review_id))]
    pub async fn get_pull_request_files(
        &self,
        project_key: &str,
        review_id: &str,
    ) -> Result<Vec<GitHubChangedFile>> {
        let url = format!("{}/files", self.pull_request_url(project_key, review_id));
        let files = self
            .paginate::<PullRequestFileDto>(&url, "failed to fetch pull request files")
            .await?;

        Ok(files.into_iter().map(map_changed_file).collect())
    }

    #[instrument(skip(self), fields(project_key = %project_key, review_id = %review_id))]
    pub async fn get_pull_request_snapshot(
        &self,
        project_key: &str,
        review_id: &str,
    ) -> Result<GitHubSnapshotData> {
        let pull_request = self.get_pull_request(project_key, review_id).await?;
        let changed_files = self.get_pull_request_files(project_key, review_id).await?;

        debug!(
            changed_files = changed_files.len(),
            "assembled pull request snapshot"
        );

        Ok(GitHubSnapshotData {
            pull_request,
            changed_files,
        })
    }

    #[instrument(skip(self), fields(project_key = %project_key, review_id = %review_id, reviewer_count = reviewers.len()))]
    pub async fn request_reviewers(
        &self,
        project_key: &str,
        review_id: &str,
        reviewers: &[String],
    ) -> Result<()> {
        let url = format!(
            "{}/requested_reviewers",
            self.pull_request_url(project_key, review_id)
        );

        let response = self
            .request(Method::POST, url)
            .json(&serde_json::json!({ "reviewers": reviewers }))
            .send()
            .await
            .context("failed to send reviewer assignment request")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("GitHub reviewer assignment failed: {} - {}", status, body);
        }

        Ok(())
    }

    #[instrument(skip(self), fields(project_key = %project_key, review_id = %review_id))]
    pub async fn get_issue_comments(
        &self,
        project_key: &str,
        review_id: &str,
    ) -> Result<Vec<PullRequestComment>> {
        let url = format!(
            "{}/repos/{}/issues/{}/comments",
            self.config.base_url.trim_end_matches('/'),
            project_key,
            review_id
        );

        let comments = self
            .paginate::<IssueCommentDto>(&url, "failed to fetch pull request comments")
            .await?;

        Ok(comments
            .into_iter()
            .map(|comment| PullRequestComment {
                id: comment.id,
                body: comment.body,
            })
            .collect())
    }

    #[instrument(skip(self, body), fields(project_key = %project_key, review_id = %review_id, body_len = body.len()))]
    pub async fn post_comment(&self, project_key: &str, review_id: &str, body: &str) -> Result<()> {
        let url = format!(
            "{}/repos/{}/issues/{}/comments",
            self.config.base_url.trim_end_matches('/'),
            project_key,
            review_id
        );

        self.request(Method::POST, url)
            .json(&serde_json::json!({ "body": body }))
            .send()
            .await
            .context("failed to send comment request to GitHub")?
            .error_for_status()
            .context("GitHub returned an error status while posting comment")?;

        Ok(())
    }

    #[instrument(skip(self, body), fields(project_key = %project_key, comment_id = comment_id, body_len = body.len()))]
    pub async fn update_comment(&self, project_key: &str, comment_id: u64, body: &str) -> Result<()> {
        let url = format!(
            "{}/repos/{}/issues/comments/{}",
            self.config.base_url.trim_end_matches('/'),
            project_key,
            comment_id
        );

        let response = self
            .request(Method::PATCH, url)
            .json(&serde_json::json!({ "body": body }))
            .send()
            .await
            .context("failed to send comment update request to GitHub")?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("GitHub comment update failed: {} - {}", status, body_text);
        }

        Ok(())
    }

    async fn paginate<T>(&self, url: &str, context: &str) -> Result<Vec<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut page = 1;
        let mut collected = Vec::new();

        loop {
            let page_items = self
                .request(Method::GET, url.to_string())
                .query(&[
                    ("per_page", PAGE_SIZE.to_string()),
                    ("page", page.to_string()),
                ])
                .send()
                .await
                .with_context(|| format!("{context} page {page}"))?
                .error_for_status()
                .with_context(|| format!("GitHub returned an error status while {context}"))?
                .json::<Vec<T>>()
                .await
                .with_context(|| format!("failed to deserialize GitHub page {page}"))?;

            let count = page_items.len();
            collected.extend(page_items);

            if count < PAGE_SIZE {
                break;
            }

            page += 1;
        }

        Ok(collected)
    }

    fn request(&self, method: Method, url: String) -> RequestBuilder {
        self.http
            .request(method, url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "mr-milchick")
    }

    fn pull_request_url(&self, project_key: &str, review_id: &str) -> String {
        format!(
            "{}/repos/{}/pulls/{}",
            self.config.base_url.trim_end_matches('/'),
            project_key,
            review_id
        )
    }
}

fn map_pull_request(dto: PullRequestDto) -> GitHubPullRequest {
    GitHubPullRequest {
        number: dto.number,
        title: dto.title,
        body: dto.body,
        state: dto.state,
        is_draft: dto.draft,
        web_url: dto.html_url,
        author_username: dto.user.login,
        reviewer_usernames: dto
            .requested_reviewers
            .into_iter()
            .map(|reviewer| reviewer.login)
            .collect(),
        labels: dto
            .labels
            .into_iter()
            .map(|label| label.name)
            .filter(|name| !name.trim().is_empty())
            .collect(),
    }
}

fn map_changed_file(dto: PullRequestFileDto) -> GitHubChangedFile {
    GitHubChangedFile {
        path: dto.filename,
        previous_path: dto.previous_filename,
        status: dto.status,
        additions: dto.additions,
        deletions: dto.deletions,
    }
}
