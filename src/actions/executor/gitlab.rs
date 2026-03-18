use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashSet;
use tracing::{debug, info, instrument};

use crate::actions::executor::{ActionExecutor, ExecutedAction, ExecutionReport};
use crate::actions::model::{Action, ActionPlan};
use crate::comment::render::MR_MILCHICK_MARKER;
use crate::gitlab::client::GitLabClient;

pub struct GitLabExecutor<'a> {
    pub client: &'a GitLabClient,
    pub project_id: &'a str,
    pub merge_request_iid: &'a str,
}

#[async_trait]
impl<'a> ActionExecutor for GitLabExecutor<'a> {
    #[instrument(
        skip_all,
        fields(
            project_id = %self.project_id,
            merge_request_iid = %self.merge_request_iid,
            action_count = plan.actions.len()
        )
    )]
    async fn execute(&self, plan: &ActionPlan) -> Result<ExecutionReport> {
        let mut report = ExecutionReport::default();

        let existing_notes = self
            .client
            .get_merge_request_notes(self.project_id, self.merge_request_iid)
            .await?;
        debug!(
            existing_notes = existing_notes.len(),
            "loaded existing merge request notes"
        );

        for action in &plan.actions {
            match action {
                Action::AssignReviewers {
                    reviewers,
                    existing_reviewers,
                } => {
                    debug!(
                        reviewer_count = reviewers.len(),
                        "processing reviewer assignment action"
                    );
                    if reviewers.is_empty() {
                        report
                            .executed
                            .push(ExecutedAction::ReviewersSkippedAlreadyPresent {
                                reviewers: reviewers.clone(),
                            });
                        continue;
                    }

                    let final_reviewers = merge_reviewer_usernames(existing_reviewers, reviewers);

                    if final_reviewers.len() == existing_reviewers.len() {
                        debug!(
                            existing_reviewers = existing_reviewers.len(),
                            requested_reviewers = reviewers.len(),
                            "skipping reviewer assignment because all requested reviewers are already present"
                        );
                        report
                            .executed
                            .push(ExecutedAction::ReviewersSkippedAlreadyPresent {
                                reviewers: reviewers.clone(),
                            });
                        continue;
                    }

                    self.client
                        .assign_reviewers(self.project_id, self.merge_request_iid, &final_reviewers)
                        .await?;
                    info!(
                        added_reviewer_count = reviewers.len(),
                        final_reviewer_count = final_reviewers.len(),
                        "assigned reviewers in GitLab"
                    );

                    report.executed.push(ExecutedAction::ReviewersAssigned {
                        reviewers: final_reviewers,
                    });
                }
                Action::PostComment { body } => {
                    debug!(
                        structured_summary = body.contains(MR_MILCHICK_MARKER),
                        body_len = body.len(),
                        "processing comment action"
                    );
                    if body.contains(MR_MILCHICK_MARKER) {
                        if let Some(existing_note) = existing_notes
                            .iter()
                            .find(|note| note.body.contains(MR_MILCHICK_MARKER))
                        {
                            if existing_note.body.trim() == body.trim() {
                                debug!(
                                    note_id = existing_note.id,
                                    "skipping unchanged structured summary comment"
                                );
                                report.executed.push(
                                    ExecutedAction::CommentSkippedAlreadyPresent {
                                        body: body.clone(),
                                    },
                                );
                                continue;
                            }

                            self.client
                                .update_comment(
                                    self.project_id,
                                    self.merge_request_iid,
                                    existing_note.id,
                                    body,
                                )
                                .await?;
                            info!(
                                note_id = existing_note.id,
                                "updated existing structured summary comment"
                            );

                            report
                                .executed
                                .push(ExecutedAction::CommentPosted { body: body.clone() });
                            continue;
                        }
                    }

                    let already_present = existing_notes
                        .iter()
                        .any(|note| note.body.trim() == body.trim());

                    if already_present {
                        debug!("skipping duplicate comment body already present on merge request");
                        report
                            .executed
                            .push(ExecutedAction::CommentSkippedAlreadyPresent {
                                body: body.clone(),
                            });
                        continue;
                    }

                    self.client
                        .post_comment(self.project_id, self.merge_request_iid, body)
                        .await?;
                    info!(
                        structured_summary = body.contains(MR_MILCHICK_MARKER),
                        "posted merge request comment"
                    );

                    report
                        .executed
                        .push(ExecutedAction::CommentPosted { body: body.clone() });
                }
                Action::FailPipeline { reason } => {
                    info!(reason, "recorded pipeline failure action");
                    report
                        .executed
                        .push(ExecutedAction::PipelineFailurePlanned {
                            reason: reason.clone(),
                        });
                }
            }
        }
        debug!(
            executed_actions = report.executed.len(),
            "GitLab action execution finished"
        );

        Ok(report)
    }
}

fn merge_reviewer_usernames(
    existing_reviewers: &[String],
    reviewers_to_add: &[String],
) -> Vec<String> {
    let mut selected = HashSet::new();
    let mut merged = Vec::with_capacity(existing_reviewers.len() + reviewers_to_add.len());

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

#[cfg(test)]
mod tests {
    use super::merge_reviewer_usernames;

    #[test]
    fn preserves_existing_reviewers_and_appends_missing_ones() {
        let merged = merge_reviewer_usernames(
            &["alice".to_string(), "bob".to_string()],
            &["carol".to_string(), "dora".to_string()],
        );

        assert_eq!(
            merged,
            vec![
                "alice".to_string(),
                "bob".to_string(),
                "carol".to_string(),
                "dora".to_string()
            ]
        );
    }

    #[test]
    fn deduplicates_requested_reviewers_against_existing_assignments() {
        let merged = merge_reviewer_usernames(
            &["alice".to_string(), "bob".to_string()],
            &["bob".to_string(), "carol".to_string(), "alice".to_string()],
        );

        assert_eq!(
            merged,
            vec!["alice".to_string(), "bob".to_string(), "carol".to_string(),]
        );
    }
}
