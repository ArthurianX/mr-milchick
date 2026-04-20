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

use self::api::{GitHubConfig, GitHubSnapshotData};
use self::client::GitHubClient;

pub const MR_MILCHICK_MARKER: &str = "<!-- mr-milchick:summary -->";
pub const MR_MILCHICK_EXPLAIN_MARKER: &str = "<!-- mr-milchick:explain -->";

pub struct GitHubPlatformConnector {
    client: GitHubClient,
    project_key: String,
    review_id: String,
    source_branch: String,
    target_branch: String,
    labels: Vec<String>,
}

impl GitHubPlatformConnector {
    pub fn new(
        config: GitHubConfig,
        project_key: impl Into<String>,
        review_id: impl Into<String>,
        source_branch: impl Into<String>,
        target_branch: impl Into<String>,
        labels: Vec<String>,
    ) -> Self {
        Self {
            client: GitHubClient::new(config),
            project_key: project_key.into(),
            review_id: review_id.into(),
            source_branch: source_branch.into(),
            target_branch: target_branch.into(),
            labels,
        }
    }
}

#[async_trait]
impl PlatformConnector for GitHubPlatformConnector {
    fn kind(&self) -> ReviewPlatformKind {
        ReviewPlatformKind::GitHub
    }

    async fn load_snapshot(&self) -> ConnectorResult<ReviewSnapshot> {
        let data = self
            .client
            .get_pull_request_snapshot(&self.project_key, &self.review_id)
            .await
            .map_err(map_request_error)?;

        Ok(map_snapshot(
            data,
            &self.project_key,
            &self.source_branch,
            &self.target_branch,
            &self.labels,
        ))
    }

    async fn load_managed_comment(
        &self,
        marker: &str,
    ) -> ConnectorResult<Option<ManagedReviewComment>> {
        let existing_comments = self
            .client
            .get_issue_comments(&self.project_key, &self.review_id)
            .await
            .map_err(map_request_error)?;

        Ok(
            find_managed_comment(&existing_comments, marker).map(|comment| ManagedReviewComment {
                body: comment.body.clone(),
            }),
        )
    }

    async fn apply_review_actions(
        &self,
        actions: &[ReviewAction],
    ) -> ConnectorResult<ReviewActionReport> {
        let existing_comments = self
            .client
            .get_issue_comments(&self.project_key, &self.review_id)
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
                        .get_pull_request(&self.project_key, &self.review_id)
                        .await
                        .map_err(map_request_error)?
                        .reviewer_usernames;
                    let final_reviewers = merge_reviewer_usernames(&existing, &requested_reviewers);

                    self.client
                        .request_reviewers(&self.project_key, &self.review_id, &final_reviewers)
                        .await
                        .map_err(map_request_error)?;

                    report.applied.push(AppliedReviewAction {
                        action: ReviewActionKind::AssignReviewers,
                        detail: Some(final_reviewers.join(", ")),
                    });
                }
                ReviewAction::UpsertSummary { markdown } => {
                    upsert_managed_comment(
                        &self.client,
                        &self.project_key,
                        &self.review_id,
                        &existing_comments,
                        MR_MILCHICK_MARKER,
                        markdown,
                        ReviewActionKind::UpsertSummary,
                        "summary unchanged",
                        &mut report,
                    )
                    .await?;
                }
                ReviewAction::UpsertExplain { markdown } => {
                    upsert_managed_comment(
                        &self.client,
                        &self.project_key,
                        &self.review_id,
                        &existing_comments,
                        MR_MILCHICK_EXPLAIN_MARKER,
                        markdown,
                        ReviewActionKind::UpsertExplain,
                        "explain unchanged",
                        &mut report,
                    )
                    .await?;
                }
                ReviewAction::AddLabels { .. } | ReviewAction::RemoveLabels { .. } => {
                    report.skipped.push(SkippedReviewAction {
                        action: action.kind(),
                        reason: "not implemented for GitHub yet".to_string(),
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

pub type GitHubReviewConnector = GitHubPlatformConnector;

fn map_snapshot(
    data: GitHubSnapshotData,
    project_key: &str,
    source_branch: &str,
    target_branch: &str,
    labels: &[String],
) -> ReviewSnapshot {
    let pull_request = data.pull_request;
    let web_url = Some(pull_request.web_url.clone());
    let repository = repository_from_project_key(project_key, &pull_request.web_url);

    ReviewSnapshot {
        review_ref: ReviewRef {
            platform: ReviewPlatformKind::GitHub,
            project_key: project_key.to_string(),
            review_id: pull_request.number.to_string(),
            web_url,
        },
        repository,
        title: pull_request.title,
        description: pull_request.body,
        author: Actor {
            username: pull_request.author_username,
            display_name: None,
        },
        participants: pull_request
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
                path: file.path,
                previous_path: file.previous_path,
                change_type: github_change_type(&file.status),
                additions: file.additions,
                deletions: file.deletions,
                patch: file.patch,
            })
            .collect(),
        labels: if labels.is_empty() {
            pull_request.labels
        } else {
            labels.to_vec()
        },
        is_draft: pull_request.is_draft,
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

fn github_change_type(status: &str) -> ChangeType {
    match status {
        "added" => ChangeType::Added,
        "removed" => ChangeType::Deleted,
        "renamed" => ChangeType::Renamed,
        "modified" => ChangeType::Modified,
        _ => ChangeType::Unknown,
    }
}

fn repository_from_project_key(project_key: &str, web_url: &str) -> RepositoryRef {
    let mut parts = project_key.splitn(2, '/');
    let namespace = parts.next().unwrap_or("owner").to_string();
    let name = parts.next().unwrap_or("repo").to_string();
    let repository_url = web_url
        .split("/pull/")
        .next()
        .unwrap_or(web_url)
        .trim_end_matches('/')
        .to_string();

    RepositoryRef {
        platform: ReviewPlatformKind::GitHub,
        namespace,
        name,
        web_url: Some(repository_url),
    }
}

pub fn render_github_markdown(markdown: &str) -> String {
    render_github_managed_markdown(MR_MILCHICK_MARKER, markdown)
}

pub fn render_github_explain_markdown(markdown: &str) -> String {
    render_github_managed_markdown(MR_MILCHICK_EXPLAIN_MARKER, markdown)
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

fn render_github_managed_markdown(marker: &str, markdown: &str) -> String {
    if markdown.trim().is_empty() {
        marker.to_string()
    } else {
        format!("{}\n\n{}", marker, markdown.trim())
    }
}

fn find_managed_comment<'a>(
    comments: &'a [self::api::PullRequestComment],
    marker: &str,
) -> Option<&'a self::api::PullRequestComment> {
    comments
        .iter()
        .find(|comment| comment.body.contains(marker))
}

async fn upsert_managed_comment(
    client: &GitHubClient,
    project_key: &str,
    review_id: &str,
    existing_comments: &[self::api::PullRequestComment],
    marker: &str,
    markdown: &str,
    action_kind: ReviewActionKind,
    unchanged_reason: &str,
    report: &mut ReviewActionReport,
) -> ConnectorResult<()> {
    let body = render_github_managed_markdown(marker, markdown);
    if let Some(existing_comment) = find_managed_comment(existing_comments, marker) {
        if existing_comment.body.trim() == body.trim() {
            report.skipped.push(SkippedReviewAction {
                action: action_kind,
                reason: unchanged_reason.to_string(),
            });
            return Ok(());
        }

        client
            .update_comment(project_key, existing_comment.id, &body)
            .await
            .map_err(map_request_error)?;
    } else {
        client
            .post_comment(project_key, review_id, &body)
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
    fn managed_comment_lookup_keeps_summary_and_explain_separate() {
        let comments = vec![
            self::api::PullRequestComment {
                id: 1,
                body: render_github_markdown("## Summary"),
            },
            self::api::PullRequestComment {
                id: 2,
                body: render_github_explain_markdown("## Explain"),
            },
        ];

        assert_eq!(
            find_managed_comment(&comments, MR_MILCHICK_MARKER)
                .expect("summary comment should exist")
                .id,
            1
        );
        assert_eq!(
            find_managed_comment(&comments, MR_MILCHICK_EXPLAIN_MARKER)
                .expect("explain comment should exist")
                .id,
            2
        );
    }
}
