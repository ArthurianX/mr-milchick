use crate::context::model::CiContext;
use crate::rules::branch_policy::evaluate_branch_policy;
use crate::rules::model::RuleOutcome;

pub fn evaluate_rules(ctx: &CiContext) -> RuleOutcome {
    let mut combined = RuleOutcome::new();

    let outcomes = [evaluate_branch_policy(ctx)];

    for outcome in outcomes {
        combined.findings.extend(outcome.findings);
    }

    combined
}