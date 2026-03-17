use anyhow::{Context, Result};
use reqwest::Client;

use crate::gitlab::api::{
    ChangedFile, GitLabConfig, MergeRequestDetails, MergeRequestNote, MergeRequestSnapshot,
    MergeRequestState,
};
use crate::gitlab::dto::{ChangedFileDto, MergeRequestChangesDto, MergeRequestDto};

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
            let user_id = self.resolve_user_id_by_username(username).await?;
            reviewer_ids.push(user_id);
        }

        let response = self
            .http
            .put(url)
            .header("PRIVATE-TOKEN", &self.config.token)
            .json(&serde_json::json!({
                "reviewer_ids": reviewer_ids
            }))
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
            .json(&serde_json::json!({
                "body": body
            }))
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    pub async fn get_merge_request(
        &self,
        project_id: &str,
        merge_request_iid: &str,
    ) -> Result<MergeRequestDetails> {
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

        let dto = response
            .json::<MergeRequestDto>()
            .await
            .context("failed to deserialize merge request response")?;

        Ok(map_merge_request(dto))
    }

    pub async fn get_merge_request_changes(
        &self,
        project_id: &str,
        merge_request_iid: &str,
    ) -> Result<Vec<ChangedFile>> {
        let url = format!(
            "{}/projects/{}/merge_requests/{}/changes",
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
            .context("failed to send changes request to GitLab")?;

        let response = response
            .error_for_status()
            .context("GitLab returned an error status while fetching merge request changes")?;

        let dto = response
            .json::<MergeRequestChangesDto>()
            .await
            .context("failed to deserialize merge request changes response")?;

        Ok(dto.changes.into_iter().map(map_changed_file).collect())
    }

    pub async fn get_merge_request_snapshot(
        &self,
        project_id: &str,
        merge_request_iid: &str,
    ) -> Result<MergeRequestSnapshot> {
        let details = self
            .get_merge_request(project_id, merge_request_iid)
            .await?;
        let changed_files = self
            .get_merge_request_changes(project_id, merge_request_iid)
            .await?;

        Ok(MergeRequestSnapshot {
            details,
            changed_files,
        })
    }

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
            .json::<Vec<crate::gitlab::dto::UserLookupDto>>()
            .await
            .context("failed to deserialize user lookup response")?;

        let user = users
            .into_iter()
            .find(|u| u.username.eq_ignore_ascii_case(username))
            .with_context(|| format!("no GitLab user found for username '{}'", username))?;

        Ok(user.id)
    }

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
            .json::<Vec<crate::gitlab::dto::MergeRequestNoteDto>>()
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
            .json(&serde_json::json!({
                "body": body
            }))
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

fn map_merge_request(dto: MergeRequestDto) -> MergeRequestDetails {
    MergeRequestDetails {
        iid: dto.iid,
        title: dto.title,
        description: dto.description,
        state: map_merge_request_state(dto.state),
        is_draft: dto.draft,
        web_url: dto.web_url,
        author_username: dto.author.username,
        reviewer_usernames: dto.reviewers.into_iter().map(|u| u.username).collect(),
    }
}

fn map_merge_request_state(state: String) -> MergeRequestState {
    match state.as_str() {
        "opened" => MergeRequestState::Opened,
        "closed" => MergeRequestState::Closed,
        "locked" => MergeRequestState::Locked,
        "merged" => MergeRequestState::Merged,
        _ => MergeRequestState::Unknown(state),
    }
}

fn map_changed_file(dto: ChangedFileDto) -> ChangedFile {
    ChangedFile {
        old_path: dto.old_path,
        new_path: dto.new_path,
        is_new: dto.new_file,
        is_renamed: dto.renamed_file,
        is_deleted: dto.deleted_file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitlab::dto::{ChangedFileDto, MergeRequestDto};

    #[test]
    fn maps_known_merge_request_state() {
        let state = map_merge_request_state("opened".to_string());
        assert_eq!(state, MergeRequestState::Opened);
    }

    #[test]
    fn preserves_unknown_merge_request_state() {
        let state = map_merge_request_state("mysterious".to_string());
        assert_eq!(state, MergeRequestState::Unknown("mysterious".to_string()));
    }

    #[test]
    fn maps_dto_into_domain_model() {
        let dto = MergeRequestDto {
            iid: 42,
            title: "Refine branch policy".to_string(),
            description: Some("A refinement opportunity has been identified.".to_string()),
            state: "opened".to_string(),
            draft: true,
            web_url: "https://gitlab.example.com/group/project/-/merge_requests/42".to_string(),
            author: crate::gitlab::dto::AuthorDto {
                username: "arthur".to_string(),
            },
            reviewers: vec![
                crate::gitlab::dto::UserDto {
                    username: "bob".to_string(),
                },
                crate::gitlab::dto::UserDto {
                    username: "carol".to_string(),
                },
            ],
        };

        let details = map_merge_request(dto);

        assert_eq!(details.iid, 42);
        assert_eq!(details.title, "Refine branch policy");
        assert_eq!(details.state, MergeRequestState::Opened);
        assert!(details.is_draft);
        assert_eq!(details.author_username, "arthur");
        assert_eq!(
            details.reviewer_usernames,
            vec!["bob".to_string(), "carol".to_string()]
        );
    }

    #[test]
    fn maps_changed_file_dto_into_domain_model() {
        let dto = ChangedFileDto {
            old_path: "src/old.rs".to_string(),
            new_path: "src/new.rs".to_string(),
            new_file: false,
            renamed_file: true,
            deleted_file: false,
        };

        let file = map_changed_file(dto);

        assert_eq!(file.old_path, "src/old.rs");
        assert_eq!(file.new_path, "src/new.rs");
        assert!(file.is_renamed);
        assert!(!file.is_new);
        assert!(!file.is_deleted);
    }
}
