pub mod api;
pub mod client;
pub mod dto;

use async_trait::async_trait;
use crate::core::model::{
    Actor, AppliedReviewAction, ChangeType, ChangedFile, MessageSection, RenderedMessage,
    RepositoryRef, ReviewAction, ReviewActionKind, ReviewActionReport, ReviewMetadata,
    ReviewPlatformKind, ReviewRef, ReviewSnapshot, SkippedReviewAction,
};
use crate::runtime::{ConnectorError, ConnectorResult, ReviewConnector};

use self::api::{GitLabConfig, GitLabSnapshotData};
use self::client::GitLabClient;

pub const MR_MILCHICK_MARKER: &str = "<!-- mr-milchick:summary -->";

pub struct GitLabReviewConnector {
    client: GitLabClient,
    project_id: String,
    merge_request_iid: String,
    source_branch: String,
    target_branch: String,
    labels: Vec<String>,
}

impl GitLabReviewConnector {
    pub fn new(
        config: GitLabConfig,
        project_id: impl Into<String>,
        merge_request_iid: impl Into<String>,
        source_branch: impl Into<String>,
        target_branch: impl Into<String>,
        labels: Vec<String>,
    ) -> Self {
        Self {
            client: GitLabClient::new(config),
            project_id: project_id.into(),
            merge_request_iid: merge_request_iid.into(),
            source_branch: source_branch.into(),
            target_branch: target_branch.into(),
            labels,
        }
    }
}

#[async_trait]
impl ReviewConnector for GitLabReviewConnector {
    fn kind(&self) -> ReviewPlatformKind {
        ReviewPlatformKind::GitLab
    }

    async fn load_snapshot(&self) -> ConnectorResult<ReviewSnapshot> {
        let data = self
            .client
            .get_merge_request_snapshot(&self.project_id, &self.merge_request_iid)
            .await
            .map_err(map_request_error)?;

        Ok(map_snapshot(
            data,
            &self.project_id,
            &self.source_branch,
            &self.target_branch,
            &self.labels,
        ))
    }

    async fn apply_review_actions(
        &self,
        actions: &[ReviewAction],
    ) -> ConnectorResult<ReviewActionReport> {
        let existing_notes = self
            .client
            .get_merge_request_notes(&self.project_id, &self.merge_request_iid)
            .await
            .map_err(map_request_error)?;

        let mut report = ReviewActionReport::default();

        for action in actions {
            match action {
                ReviewAction::AssignReviewers { reviewers } => {
                    let requested_reviewers = reviewers
                        .iter()
                        .map(|reviewer| reviewer.username.clone())
                        .collect::<Vec<_>>();
                    if requested_reviewers.is_empty() {
                        report.skipped.push(SkippedReviewAction {
                            action: ReviewActionKind::AssignReviewers,
                            reason: "no reviewers requested".to_string(),
                        });
                        continue;
                    }

                    let existing = self
                        .client
                        .get_merge_request(&self.project_id, &self.merge_request_iid)
                        .await
                        .map_err(map_request_error)?
                        .reviewer_usernames;
                    let final_reviewers = merge_reviewer_usernames(&existing, &requested_reviewers);

                    self.client
                        .assign_reviewers(
                            &self.project_id,
                            &self.merge_request_iid,
                            &final_reviewers,
                        )
                        .await
                        .map_err(map_request_error)?;

                    report.applied.push(AppliedReviewAction {
                        action: ReviewActionKind::AssignReviewers,
                        detail: Some(final_reviewers.join(", ")),
                    });
                }
                ReviewAction::UpsertSummary { message } => {
                    let body = render_gitlab_markdown(message);
                    if let Some(existing_note) = existing_notes
                        .iter()
                        .find(|note| note.body.contains(MR_MILCHICK_MARKER))
                    {
                        if existing_note.body.trim() == body.trim() {
                            report.skipped.push(SkippedReviewAction {
                                action: ReviewActionKind::UpsertSummary,
                                reason: "summary unchanged".to_string(),
                            });
                            continue;
                        }

                        self.client
                            .update_comment(
                                &self.project_id,
                                &self.merge_request_iid,
                                existing_note.id,
                                &body,
                            )
                            .await
                            .map_err(map_request_error)?;
                    } else {
                        self.client
                            .post_comment(&self.project_id, &self.merge_request_iid, &body)
                            .await
                            .map_err(map_request_error)?;
                    }

                    report.applied.push(AppliedReviewAction {
                        action: ReviewActionKind::UpsertSummary,
                        detail: Some("comment-posted".to_string()),
                    });
                }
                ReviewAction::AddLabels { .. } | ReviewAction::RemoveLabels { .. } => {
                    report.skipped.push(SkippedReviewAction {
                        action: action.kind(),
                        reason: "not implemented for GitLab yet".to_string(),
                    });
                }
                ReviewAction::FailPipeline { reason } => {
                    report.applied.push(AppliedReviewAction {
                        action: ReviewActionKind::FailPipeline,
                        detail: Some(reason.clone()),
                    });
                }
            }
        }

        Ok(report)
    }
}

fn map_snapshot(
    data: GitLabSnapshotData,
    project_id: &str,
    source_branch: &str,
    target_branch: &str,
    labels: &[String],
) -> ReviewSnapshot {
    let project_key = project_id.to_string();
    let merge_request = data.merge_request;
    let web_url = Some(merge_request.web_url.clone());
    let repository = repository_from_web_url(&merge_request.web_url);

    ReviewSnapshot {
        review_ref: ReviewRef {
            platform: ReviewPlatformKind::GitLab,
            project_key: project_key.clone(),
            review_id: merge_request.iid.to_string(),
            web_url,
        },
        repository,
        title: merge_request.title,
        description: merge_request.description,
        author: Actor {
            username: merge_request.author_username,
            display_name: None,
        },
        participants: merge_request
            .reviewer_usernames
            .into_iter()
            .map(|username| Actor {
                username,
                display_name: None,
            })
            .collect(),
        changed_files: data
            .changed_files
            .into_iter()
            .map(|file| ChangedFile {
                path: file.new_path,
                change_type: if file.is_new {
                    ChangeType::Added
                } else if file.is_deleted {
                    ChangeType::Deleted
                } else if file.is_renamed {
                    ChangeType::Renamed
                } else {
                    ChangeType::Modified
                },
                additions: None,
                deletions: None,
            })
            .collect(),
        labels: labels.to_vec(),
        is_draft: merge_request.is_draft,
        default_branch: Some(target_branch.to_string()),
        metadata: ReviewMetadata {
            source_branch: Some(source_branch.to_string()),
            target_branch: Some(target_branch.to_string()),
            commit_count: None,
            approvals_required: None,
            approvals_given: None,
        },
    }
}

fn repository_from_web_url(web_url: &str) -> RepositoryRef {
    let trimmed = web_url
        .split("/-/merge_requests/")
        .next()
        .unwrap_or(web_url)
        .trim_end_matches('/');
    let parts = trimmed.split('/').collect::<Vec<_>>();
    let name = parts.last().copied().unwrap_or("project").to_string();
    let namespace = if parts.len() > 1 {
        parts[parts.len().saturating_sub(2)].to_string()
    } else {
        "group".to_string()
    };

    RepositoryRef {
        platform: ReviewPlatformKind::GitLab,
        namespace,
        name,
        web_url: Some(trimmed.to_string()),
    }
}

pub fn render_gitlab_markdown(message: &RenderedMessage) -> String {
    let mut lines = vec![MR_MILCHICK_MARKER.to_string()];

    if let Some(title) = &message.title {
        lines.push(format!("## {}", title));
        lines.push(String::new());
    }

    for section in &message.sections {
        match section {
            MessageSection::Paragraph(text) => {
                lines.push(text.clone());
                lines.push(String::new());
            }
            MessageSection::BulletList(items) => {
                for item in items {
                    lines.push(format!("- {}", item));
                }
                lines.push(String::new());
            }
            MessageSection::KeyValueList(items) => {
                for (key, value) in items {
                    lines.push(format!("- **{}**: {}", key, value));
                }
                lines.push(String::new());
            }
            MessageSection::CodeBlock { language, content } => {
                lines.push(format!("```{}", language.clone().unwrap_or_default()));
                lines.push(content.clone());
                lines.push("```".to_string());
                lines.push(String::new());
            }
        }
    }

    if let Some(footer) = &message.footer {
        lines.push(format!("_{}_", footer));
    }

    lines.join("\n").trim().to_string()
}

fn map_request_error(err: anyhow::Error) -> ConnectorError {
    ConnectorError::Request(err.to_string())
}

fn merge_reviewer_usernames(
    existing_reviewers: &[String],
    reviewers_to_add: &[String],
) -> Vec<String> {
    let mut selected = std::collections::BTreeSet::new();
    let mut merged = Vec::new();

    for reviewer in existing_reviewers {
        if selected.insert(reviewer.clone()) {
            merged.push(reviewer.clone());
        }
    }

    for reviewer in reviewers_to_add {
        if selected.insert(reviewer.clone()) {
            merged.push(reviewer.clone());
        }
    }

    merged
}
