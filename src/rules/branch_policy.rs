use crate::actions::model::Action;
use crate::context::model::{BranchKind, CiContext};
use crate::rules::model::{RuleFinding, RuleOutcome};

const REQUIRED_EPIC_LABEL: &str = "0. run-tests";

pub fn evaluate_branch_policy(ctx: &CiContext) -> RuleOutcome {
    let mut outcome = RuleOutcome::new();

    if ctx.target_branch() == "develop" && ctx.source_branch_kind() == BranchKind::Epic {
        if !ctx.has_label(REQUIRED_EPIC_LABEL) {
            let message = format!(
                "Epic branches targeting develop must include the '{}' label.",
                REQUIRED_EPIC_LABEL
            );

            outcome.push(RuleFinding::blocking(message.clone()));
            outcome
                .action_plan
                .push(Action::PostComment { body: message.clone() });
            outcome
                .action_plan
                .push(Action::FailPipeline { reason: message });
        } else {
            outcome.push(RuleFinding::info(format!(
                "Required label '{}' is present for epic branch targeting develop.",
                REQUIRED_EPIC_LABEL
            )));
        }
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::model::Action;
    use crate::context::model::{
        BranchInfo, BranchName, CiContext, Label, MergeRequestIid, MergeRequestRef, PipelineInfo,
        PipelineSource, ProjectId,
    };

    fn base_context() -> CiContext {
        CiContext {
            project_id: ProjectId("123".to_string()),
            merge_request: Some(MergeRequestRef {
                iid: MergeRequestIid("456".to_string()),
            }),
            pipeline: PipelineInfo {
                source: PipelineSource::MergeRequestEvent,
            },
            branches: BranchInfo {
                source: BranchName("epic/big-thing".to_string()),
                target: BranchName("develop".to_string()),
            },
            labels: vec![],
        }
    }

    #[test]
    fn blocks_epic_targeting_develop_without_required_label() {
        let ctx = base_context();

        let outcome = evaluate_branch_policy(&ctx);

        assert!(outcome.has_blocking_findings());
        assert_eq!(outcome.findings.len(), 1);
        assert!(outcome.findings[0].message.contains("0. run-tests"));
        assert_eq!(outcome.action_plan.actions.len(), 2);
        assert!(matches!(
            outcome.action_plan.actions[0],
            Action::PostComment { .. }
        ));
        assert!(matches!(
            outcome.action_plan.actions[1],
            Action::FailPipeline { .. }
        ));
    }

    #[test]
    fn passes_epic_targeting_develop_with_required_label() {
        let mut ctx = base_context();
        ctx.labels.push(Label("0. run-tests".to_string()));

        let outcome = evaluate_branch_policy(&ctx);

        assert!(!outcome.has_blocking_findings());
        assert_eq!(outcome.findings.len(), 1);
        assert!(outcome.action_plan.is_empty());
    }

    #[test]
    fn ignores_non_epic_branch() {
        let mut ctx = base_context();
        ctx.branches.source = BranchName("feat/small-thing".to_string());

        let outcome = evaluate_branch_policy(&ctx);

        assert!(outcome.is_empty());
        assert!(outcome.action_plan.is_empty());
    }

    #[test]
    fn ignores_non_develop_target() {
        let mut ctx = base_context();
        ctx.branches.target = BranchName("main".to_string());

        let outcome = evaluate_branch_policy(&ctx);

        assert!(outcome.is_empty());
        assert!(outcome.action_plan.is_empty());
    }
}