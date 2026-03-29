pub mod api;
pub mod client;
pub mod dto;

use crate::core::model::{
    Actor, AppliedReviewAction, ChangeType, ChangedFile, RepositoryRef, ReviewAction,
    ReviewActionKind, ReviewActionReport, ReviewMetadata, ReviewPlatformKind, ReviewRef,
    ReviewSnapshot, SkippedReviewAction,
};
use crate::runtime::{ConnectorError, ConnectorResult, ReviewConnector};
use async_trait::async_trait;

use self::api::{GitHubConfig, GitHubSnapshotData};
use self::client::GitHubClient;

pub const MR_MILCHICK_MARKER: &str = "<!-- mr-milchick:summary -->";

pub struct GitHubReviewConnector {
    client: GitHubClient,
    project_key: String,
    review_id: String,
    source_branch: String,
    target_branch: String,
    labels: Vec<String>,
}

impl GitHubReviewConnector {
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
impl ReviewConnector for GitHubReviewConnector {
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
                    let body = render_github_markdown(markdown);
                    if let Some(existing_comment) = existing_comments
                        .iter()
                        .find(|comment| comment.body.contains(MR_MILCHICK_MARKER))
                    {
                        if existing_comment.body.trim() == body.trim() {
                            report.skipped.push(SkippedReviewAction {
                                action: ReviewActionKind::UpsertSummary,
                                reason: "summary unchanged".to_string(),
                            });
                            continue;
                        }

                        self.client
                            .update_comment(&self.project_key, existing_comment.id, &body)
                            .await
                            .map_err(map_request_error)?;
                    } else {
                        self.client
                            .post_comment(&self.project_key, &self.review_id, &body)
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
                change_type: github_change_type(&file.status),
                additions: file.additions,
                deletions: file.deletions,
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
    if markdown.trim().is_empty() {
        MR_MILCHICK_MARKER.to_string()
    } else {
        format!("{}\n\n{}", MR_MILCHICK_MARKER, markdown.trim())
    }
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
