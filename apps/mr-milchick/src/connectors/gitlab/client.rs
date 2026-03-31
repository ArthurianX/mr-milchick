use anyhow::{Context, Result};
use reqwest::Client;
use tracing::{debug, instrument};

use super::api::{
    GitLabChangedFile, GitLabConfig, GitLabMergeRequest, GitLabSnapshotData, MergeRequestNote,
};
use super::dto::{ChangedFileDto, MergeRequestChangesDto, MergeRequestDto};

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

    #[instrument(skip(self, reviewers), fields(project_id = %project_id, merge_request_iid = %merge_request_iid, reviewer_count = reviewers.len()))]
    pub async fn assign_reviewers(
        &self,
        project_id: &str,
        merge_request_iid: &str,
        reviewers: &[String],
    ) -> Result<()> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}",
            self.config.base_url.trim_end_matches('/'),
            project_id,
            merge_request_iid
        );

        let mut reviewer_ids = Vec::with_capacity(reviewers.len());
        for username in reviewers {
            reviewer_ids.push(self.resolve_user_id_by_username(username).await?);
        }

        let response = self
            .http
            .put(url)
            .header("PRIVATE-TOKEN", &self.config.token)
            .json(&serde_json::json!({ "reviewer_ids": reviewer_ids }))
            .send()
            .await
            .context("failed to send reviewer assignment request")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("GitLab reviewer assignment failed: {} - {}", status, body);
        }

        Ok(())
    }

    #[instrument(skip(self, body), fields(project_id = %project_id, merge_request_iid = %merge_request_iid, body_len = body.len()))]
    pub async fn post_comment(
        &self,
        project_id: &str,
        merge_request_iid: &str,
        body: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}/notes",
            self.config.base_url.trim_end_matches('/'),
            project_id,
            merge_request_iid
        );

        self.http
            .post(url)
            .header("PRIVATE-TOKEN", &self.config.token)
            .json(&serde_json::json!({ "body": body }))
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    #[instrument(skip(self), fields(project_id = %project_id, merge_request_iid = %merge_request_iid))]
    pub async fn get_merge_request(
        &self,
        project_id: &str,
        merge_request_iid: &str,
    ) -> Result<GitLabMergeRequest> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}",
            self.config.base_url.trim_end_matches('/'),
            project_id,
            merge_request_iid
        );

        let dto = self
            .http
            .get(url)
            .header("PRIVATE-TOKEN", &self.config.token)
            .send()
            .await
            .context("failed to send request to GitLab")?
            .error_for_status()
            .context("GitLab returned an error status while fetching merge request")?
            .json::<MergeRequestDto>()
            .await
            .context("failed to deserialize merge request response")?;

        Ok(GitLabMergeRequest {
            iid: dto.iid,
            title: dto.title,
            description: dto.description,
            state: dto.state,
            is_draft: dto.draft,
            web_url: dto.web_url,
            author_username: dto.author.username,
            reviewer_usernames: dto.reviewers.into_iter().map(|u| u.username).collect(),
        })
    }

    #[instrument(skip(self), fields(project_id = %project_id, merge_request_iid = %merge_request_iid))]
    pub async fn get_merge_request_changes(
        &self,
        project_id: &str,
        merge_request_iid: &str,
    ) -> Result<Vec<GitLabChangedFile>> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}/changes",
            self.config.base_url.trim_end_matches('/'),
            project_id,
            merge_request_iid
        );

        let dto = self
            .http
            .get(url)
            .header("PRIVATE-TOKEN", &self.config.token)
            .send()
            .await
            .context("failed to send changes request to GitLab")?
            .error_for_status()
            .context("GitLab returned an error status while fetching merge request changes")?
            .json::<MergeRequestChangesDto>()
            .await
            .context("failed to deserialize merge request changes response")?;

        Ok(dto.changes.into_iter().map(map_changed_file).collect())
    }

    #[instrument(skip(self), fields(project_id = %project_id, merge_request_iid = %merge_request_iid))]
    pub async fn get_merge_request_snapshot(
        &self,
        project_id: &str,
        merge_request_iid: &str,
    ) -> Result<GitLabSnapshotData> {
        let merge_request = self
            .get_merge_request(project_id, merge_request_iid)
            .await?;
        let changed_files = self
            .get_merge_request_changes(project_id, merge_request_iid)
            .await?;
        debug!(
            changed_files = changed_files.len(),
            "assembled merge request snapshot"
        );

        Ok(GitLabSnapshotData {
            merge_request,
            changed_files,
        })
    }

    #[instrument(skip(self), fields(username = %username))]
    pub async fn resolve_user_id_by_username(&self, username: &str) -> Result<u64> {
        let url = format!("{}/users", self.config.base_url.trim_end_matches('/'));

        let users = self
            .http
            .get(url)
            .header("PRIVATE-TOKEN", &self.config.token)
            .query(&[("username", username)])
            .send()
            .await
            .context("failed to send user lookup request to GitLab")?
            .error_for_status()
            .context("GitLab returned an error status while looking up user by username")?
            .json::<Vec<super::dto::UserLookupDto>>()
            .await
            .context("failed to deserialize user lookup response")?;

        let user = users
            .into_iter()
            .find(|u| u.username.eq_ignore_ascii_case(username))
            .with_context(|| format!("no GitLab user found for username '{}'", username))?;

        Ok(user.id)
    }

    #[instrument(skip(self), fields(project_id = %project_id, merge_request_iid = %merge_request_iid))]
    pub async fn get_merge_request_notes(
        &self,
        project_id: &str,
        merge_request_iid: &str,
    ) -> Result<Vec<MergeRequestNote>> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}/notes",
            self.config.base_url.trim_end_matches('/'),
            project_id,
            merge_request_iid
        );

        let notes = self
            .http
            .get(url)
            .header("PRIVATE-TOKEN", &self.config.token)
            .send()
            .await
            .context("failed to send merge request notes request to GitLab")?
            .error_for_status()
            .context("GitLab returned an error status while fetching merge request notes")?
            .json::<Vec<super::dto::MergeRequestNoteDto>>()
            .await
            .context("failed to deserialize merge request notes response")?;

        Ok(notes
            .into_iter()
            .map(|n| MergeRequestNote {
                id: n.id,
                body: n.body,
            })
            .collect())
    }

    #[instrument(skip(self, body), fields(project_id = %project_id, merge_request_iid = %merge_request_iid, note_id = note_id, body_len = body.len()))]
    pub async fn update_comment(
        &self,
        project_id: &str,
        merge_request_iid: &str,
        note_id: u64,
        body: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}/notes/{}",
            self.config.base_url.trim_end_matches('/'),
            project_id,
            merge_request_iid,
            note_id
        );

        let response = self
            .http
            .put(url)
            .header("PRIVATE-TOKEN", &self.config.token)
            .json(&serde_json::json!({ "body": body }))
            .send()
            .await
            .context("failed to send comment update request to GitLab")?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("GitLab comment update failed: {} - {}", status, body_text);
        }

        Ok(())
    }
}

fn map_changed_file(dto: ChangedFileDto) -> GitLabChangedFile {
    GitLabChangedFile {
        old_path: dto.old_path,
        new_path: dto.new_path,
        is_new: dto.new_file,
        is_renamed: dto.renamed_file,
        is_deleted: dto.deleted_file,
        patch: dto.diff,
    }
}
