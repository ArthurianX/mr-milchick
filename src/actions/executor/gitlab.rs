use anyhow::Result;
use futures::executor::block_on;

use crate::actions::executor::{ActionExecutor, ExecutedAction, ExecutionReport};
use crate::actions::model::{Action, ActionPlan};
use crate::gitlab::client::GitLabClient;

pub struct GitLabExecutor<'a> {
    pub client: &'a GitLabClient,
    pub project_id: &'a str,
    pub merge_request_iid: &'a str,
}

impl<'a> ActionExecutor for GitLabExecutor<'a> {
    fn execute(&self, plan: &ActionPlan) -> Result<ExecutionReport> {
        let mut report = ExecutionReport::default();

        let existing_notes = block_on(
            self.client
                .get_merge_request_notes(self.project_id, self.merge_request_iid),
        )?;

        for action in &plan.actions {
            match action {
                Action::AssignReviewers { reviewers } => {
                    if reviewers.is_empty() {
                        report.executed.push(ExecutedAction::ReviewersSkippedAlreadyPresent {
                            reviewers: reviewers.clone(),
                        });
                        continue;
                    }

                    block_on(self.client.assign_reviewers(
                        self.project_id,
                        self.merge_request_iid,
                        reviewers,
                    ))?;

                    report.executed.push(ExecutedAction::ReviewersPlanned {
                        reviewers: reviewers.clone(),
                    });
                }
                Action::PostComment { body } => {
                    let already_present = existing_notes.iter().any(|note| note.trim() == body.trim());

                    if already_present {
                        report.executed.push(ExecutedAction::CommentSkippedAlreadyPresent {
                            body: body.clone(),
                        });
                        continue;
                    }

                    block_on(self.client.post_comment(
                        self.project_id,
                        self.merge_request_iid,
                        body,
                    ))?;

                    report.executed.push(ExecutedAction::CommentPlanned {
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