pub mod api;
pub mod client;
pub mod dto;

use crate::core::model::{
    Actor, AppliedReviewAction, ChangeType, ChangedFile, ManagedReviewComment, RepositoryRef,
    ReviewAction, ReviewActionKind, ReviewActionReport, ReviewMetadata, ReviewPlatformKind,
    ReviewRef, ReviewSnapshot, SkippedReviewAction,
};
use crate::runtime::{ConnectorError, ConnectorResult, PlatformConnector};
use async_trait::async_trait;

use self::api::{GitLabConfig, GitLabSnapshotData};
use self::client::GitLabClient;

pub const MR_MILCHICK_MARKER: &str = "<!-- mr-milchick:summary -->";
pub const MR_MILCHICK_EXPLAIN_MARKER: &str = "<!-- mr-milchick:explain -->";

pub struct GitLabPlatformConnector {
    client: GitLabClient,
    project_id: String,
    merge_request_iid: String,
    source_branch: String,
    target_branch: String,
    labels: Vec<String>,
}

impl GitLabPlatformConnector {
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
impl PlatformConnector for GitLabPlatformConnector {
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

    async fn load_managed_comment(
        &self,
        marker: &str,
    ) -> ConnectorResult<Option<ManagedReviewComment>> {
        let existing_notes = self
            .client
            .get_merge_request_notes(&self.project_id, &self.merge_request_iid)
            .await
            .map_err(map_request_error)?;

        Ok(
            find_managed_note(&existing_notes, marker).map(|note| ManagedReviewComment {
                body: note.body.clone(),
            }),
        )
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
        let mut current_labels: Option<Vec<String>> = None;

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
                ReviewAction::UpsertSummary { markdown } => {
                    upsert_managed_note(
                        &self.client,
                        &self.project_id,
                        &self.merge_request_iid,
                        &existing_notes,
                        MR_MILCHICK_MARKER,
                        markdown,
                        ReviewActionKind::UpsertSummary,
                        "summary unchanged",
                        &mut report,
                    )
                    .await?;
                }
                ReviewAction::UpsertExplain { markdown } => {
                    upsert_managed_note(
                        &self.client,
                        &self.project_id,
                        &self.merge_request_iid,
                        &existing_notes,
                        MR_MILCHICK_EXPLAIN_MARKER,
                        markdown,
                        ReviewActionKind::UpsertExplain,
                        "explain unchanged",
                        &mut report,
                    )
                    .await?;
                }
                ReviewAction::AddLabels { labels } => {
                    let existing_labels = match &current_labels {
                        Some(labels) => labels.clone(),
                        None => {
                            let labels = self
                                .client
                                .get_merge_request(&self.project_id, &self.merge_request_iid)
                                .await
                                .map_err(map_request_error)?
                                .labels;
                            current_labels = Some(labels.clone());
                            labels
                        }
                    };
                    let labels_to_add = labels
                        .iter()
                        .filter(|label| !existing_labels.iter().any(|existing| existing == *label))
                        .cloned()
                        .collect::<Vec<_>>();
                    if labels_to_add.is_empty() {
                        report.skipped.push(SkippedReviewAction {
                            action: ReviewActionKind::AddLabels,
                            reason: "labels already present".to_string(),
                        });
                        continue;
                    }

                    self.client
                        .add_labels(&self.project_id, &self.merge_request_iid, &labels_to_add)
                        .await
                        .map_err(map_request_error)?;

                    if let Some(existing) = current_labels.as_mut() {
                        for label in &labels_to_add {
                            if !existing.iter().any(|current| current == label) {
                                existing.push(label.clone());
                            }
                        }
                    }

                    report.applied.push(AppliedReviewAction {
                        action: ReviewActionKind::AddLabels,
                        detail: Some(labels_to_add.join(", ")),
                    });
                }
                ReviewAction::RemoveLabels { labels } => {
                    let existing_labels = match &current_labels {
                        Some(labels) => labels.clone(),
                        None => {
                            let labels = self
                                .client
                                .get_merge_request(&self.project_id, &self.merge_request_iid)
                                .await
                                .map_err(map_request_error)?
                                .labels;
                            current_labels = Some(labels.clone());
                            labels
                        }
                    };
                    let labels_to_remove = labels
                        .iter()
                        .filter(|label| existing_labels.iter().any(|existing| existing == *label))
                        .cloned()
                        .collect::<Vec<_>>();
                    if labels_to_remove.is_empty() {
                        report.skipped.push(SkippedReviewAction {
                            action: ReviewActionKind::RemoveLabels,
                            reason: "labels already absent".to_string(),
                        });
                        continue;
                    }

                    self.client
                        .remove_labels(&self.project_id, &self.merge_request_iid, &labels_to_remove)
                        .await
                        .map_err(map_request_error)?;

                    if let Some(existing) = current_labels.as_mut() {
                        existing.retain(|label| {
                            !labels_to_remove.iter().any(|removed| removed == label)
                        });
                    }

                    report.applied.push(AppliedReviewAction {
                        action: ReviewActionKind::RemoveLabels,
                        detail: Some(labels_to_remove.join(", ")),
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

pub type GitLabReviewConnector = GitLabPlatformConnector;

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
            .map(|file| {
                let previous_path =
                    (file.old_path != file.new_path).then_some(file.old_path.clone());

                ChangedFile {
                    path: file.new_path,
                    previous_path,
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
                    patch: file.patch,
                }
            })
            .collect(),
        labels: if merge_request.labels.is_empty() {
            labels.to_vec()
        } else {
            merge_request.labels
        },
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

pub fn render_gitlab_markdown(markdown: &str) -> String {
    render_gitlab_managed_markdown(MR_MILCHICK_MARKER, markdown)
}

pub fn render_gitlab_explain_markdown(markdown: &str) -> String {
    render_gitlab_managed_markdown(MR_MILCHICK_EXPLAIN_MARKER, markdown)
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

fn render_gitlab_managed_markdown(marker: &str, markdown: &str) -> String {
    if markdown.trim().is_empty() {
        marker.to_string()
    } else {
        format!("{}\n\n{}", marker, markdown.trim())
    }
}

fn find_managed_note<'a>(
    notes: &'a [self::api::MergeRequestNote],
    marker: &str,
) -> Option<&'a self::api::MergeRequestNote> {
    notes.iter().find(|note| note.body.contains(marker))
}

async fn upsert_managed_note(
    client: &GitLabClient,
    project_id: &str,
    merge_request_iid: &str,
    existing_notes: &[self::api::MergeRequestNote],
    marker: &str,
    markdown: &str,
    action_kind: ReviewActionKind,
    unchanged_reason: &str,
    report: &mut ReviewActionReport,
) -> ConnectorResult<()> {
    let body = render_gitlab_managed_markdown(marker, markdown);
    if let Some(existing_note) = find_managed_note(existing_notes, marker) {
        if existing_note.body.trim() == body.trim() {
            report.skipped.push(SkippedReviewAction {
                action: action_kind,
                reason: unchanged_reason.to_string(),
            });
            return Ok(());
        }

        client
            .update_comment(project_id, merge_request_iid, existing_note.id, &body)
            .await
            .map_err(map_request_error)?;
    } else {
        client
            .post_comment(project_id, merge_request_iid, &body)
            .await
            .map_err(map_request_error)?;
    }

    report.applied.push(AppliedReviewAction {
        action: action_kind,
        detail: Some("comment-posted".to_string()),
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_note_lookup_keeps_summary_and_explain_separate() {
        let notes = vec![
            self::api::MergeRequestNote {
                id: 1,
                body: render_gitlab_markdown("## Summary"),
            },
            self::api::MergeRequestNote {
                id: 2,
                body: render_gitlab_explain_markdown("## Explain"),
            },
        ];

        assert_eq!(
            find_managed_note(&notes, MR_MILCHICK_MARKER)
                .expect("summary note should exist")
                .id,
            1
        );
        assert_eq!(
            find_managed_note(&notes, MR_MILCHICK_EXPLAIN_MARKER)
                .expect("explain note should exist")
                .id,
            2
        );
    }
}
