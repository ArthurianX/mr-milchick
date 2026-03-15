use anyhow::{Context, Result};
use reqwest::Client;

use crate::gitlab::api::{
    ChangedFile, GitLabConfig, MergeRequestDetails, MergeRequestSnapshot, MergeRequestState,
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
        let details = self.get_merge_request(project_id, merge_request_iid).await?;
        let changed_files = self
            .get_merge_request_changes(project_id, merge_request_iid)
            .await?;

        Ok(MergeRequestSnapshot {
            details,
            changed_files,
        })
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
        };

        let details = map_merge_request(dto);

        assert_eq!(details.iid, 42);
        assert_eq!(details.title, "Refine branch policy");
        assert_eq!(details.state, MergeRequestState::Opened);
        assert!(details.is_draft);
        assert_eq!(details.author_username, "arthur");
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