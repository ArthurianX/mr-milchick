pub mod gitlab;

use anyhow::Result;

use crate::actions::model::{Action, ActionPlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutedAction {
    CommentPlanned { body: String },
    ReviewersPlanned { reviewers: Vec<String> },
    PipelineFailurePlanned { reason: String },
    CommentSkippedAlreadyPresent { body: String },
    ReviewersSkippedAlreadyPresent { reviewers: Vec<String> },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionReport {
    pub executed: Vec<ExecutedAction>,
}

impl ExecutionReport {
    pub fn is_empty(&self) -> bool {
        self.executed.is_empty()
    }
}

pub trait ActionExecutor {
    fn execute(&self, plan: &ActionPlan) -> Result<ExecutionReport>;
}

#[derive(Debug, Default)]
pub struct DryRunExecutor;

impl ActionExecutor for DryRunExecutor {
    fn execute(&self, plan: &ActionPlan) -> Result<ExecutionReport> {
        let mut report = ExecutionReport::default();

        for action in &plan.actions {
            let executed = match action {
                Action::PostComment { body } => ExecutedAction::CommentPlanned {
                    body: body.clone(),
                },
                Action::AssignReviewers { reviewers } => ExecutedAction::ReviewersPlanned {
                    reviewers: reviewers.clone(),
                },
                Action::FailPipeline { reason } => ExecutedAction::PipelineFailurePlanned {
                    reason: reason.clone(),
                },
            };

            report.executed.push(executed);
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::model::{Action, ActionPlan};

    #[test]
    fn dry_run_executor_reports_actions_without_side_effects() {
        let mut plan = ActionPlan::new();
        plan.push(Action::PostComment {
            body: "A refinement opportunity has been identified.".to_string(),
        });
        plan.push(Action::FailPipeline {
            reason: "Label is missing.".to_string(),
        });

        let executor = DryRunExecutor;
        let report = executor.execute(&plan).expect("dry run should succeed");

        assert_eq!(report.executed.len(), 2);
        assert!(matches!(
            report.executed[0],
            ExecutedAction::CommentPlanned { .. }
        ));
        assert!(matches!(
            report.executed[1],
            ExecutedAction::PipelineFailurePlanned { .. }
        ));
    }
}