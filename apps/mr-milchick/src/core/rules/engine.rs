use crate::core::context::model::CiContext;
use crate::core::rules::branch_policy::evaluate_branch_policy;
use crate::core::rules::model::RuleOutcome;

pub fn evaluate_rules(ctx: &CiContext) -> RuleOutcome {
    let mut combined = RuleOutcome::new();

    let outcomes = [evaluate_branch_policy(ctx)];

    for mut outcome in outcomes {
        combined.findings.append(&mut outcome.findings);
        combined
            .action_plan
            .actions
            .append(&mut outcome.action_plan.actions);
    }

    combined
}
