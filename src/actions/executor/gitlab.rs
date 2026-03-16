use anyhow::Result;
use async_trait::async_trait;

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
    async fn execute(&self, plan: &ActionPlan) -> Result<ExecutionReport> {
        let mut report = ExecutionReport::default();

        let existing_notes = self
            .client
            .get_merge_request_notes(self.project_id, self.merge_request_iid)
            .await?;

        for action in &plan.actions {
            match action {
                Action::AssignReviewers { reviewers } => {
                    if reviewers.is_empty() {
                        report
                            .executed
                            .push(ExecutedAction::ReviewersSkippedAlreadyPresent {
                                reviewers: reviewers.clone(),
                            });
                        continue;
                    }

                    self.client
                        .assign_reviewers(self.project_id, self.merge_request_iid, reviewers)
                        .await?;

                    report.executed.push(ExecutedAction::ReviewersAssigned {
                        reviewers: reviewers.clone(),
                    });
                }
                Action::PostComment { body } => {
                    if body.contains(MR_MILCHICK_MARKER) {
                        if let Some(existing_note) = existing_notes
                            .iter()
                            .find(|note| note.body.contains(MR_MILCHICK_MARKER))
                        {
                            if existing_note.body.trim() == body.trim() {
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

                            report.executed.push(ExecutedAction::CommentPosted {
                                body: body.clone(),
                            });
                            continue;
                        }
                    }

                    let already_present =
                        existing_notes.iter().any(|note| note.body.trim() == body.trim());

                    if already_present {
                        report.executed.push(ExecutedAction::CommentSkippedAlreadyPresent {
                            body: body.clone(),
                        });
                        continue;
                    }

                    self.client
                        .post_comment(self.project_id, self.merge_request_iid, body)
                        .await?;

                    report.executed.push(ExecutedAction::CommentPosted {
                        body: body.clone(),
                    });
                }
                Action::FailPipeline { reason } => {
                    report.executed.push(ExecutedAction::PipelineFailurePlanned {
                        reason: reason.clone(),
                    });
                }
            }
        }

        Ok(report)
    }
}