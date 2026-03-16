use crate::actions::model::Action;
use crate::rules::model::{FindingSeverity, RuleOutcome};

pub const MR_MILCHICK_MARKER: &str = "<!-- mr-milchick:summary -->";

pub fn render_summary_comment(outcome: &RuleOutcome) -> String {
    let mut lines = Vec::new();

    lines.push(MR_MILCHICK_MARKER.to_string());
    lines.push("## Mr. Milchick Review Summary".to_string());
    lines.push(String::new());

    if outcome.findings.is_empty() {
        lines.push("No findings were produced.".to_string());
    } else {
        lines.push("### Findings".to_string());
        lines.push(String::new());

        for finding in &outcome.findings {
            let severity = match finding.severity {
                FindingSeverity::Info => "Info",
                FindingSeverity::Warning => "Warning",
                FindingSeverity::Blocking => "Blocking",
            };

            lines.push(format!("- **{}**: {}", severity, finding.message));
        }
    }

    lines.push(String::new());

    if outcome.action_plan.actions.is_empty() {
        lines.push("### Planned Actions".to_string());
        lines.push(String::new());
        lines.push("- None".to_string());
    } else {
        lines.push("### Planned Actions".to_string());
        lines.push(String::new());

        for action in &outcome.action_plan.actions {
            let text = match action {
                Action::PostComment { body } => format!("Post comment: {}", body),
                Action::AssignReviewers { reviewers } => {
                    format!("Assign reviewers: {}", reviewers.join(", "))
                }
                Action::FailPipeline { reason } => format!("Fail pipeline: {}", reason),
            };

            lines.push(format!("- {}", text));
        }
    }

    lines.push(String::new());
    lines.push("_The matter has been handled pleasantly._".to_string());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::model::{Action, ActionPlan};
    use crate::rules::model::{RuleFinding, RuleOutcome};

    #[test]
    fn renders_structured_summary_comment() {
        let mut outcome = RuleOutcome {
            findings: vec![RuleFinding::blocking("Missing label.")],
            action_plan: ActionPlan::new(),
        };
        outcome.action_plan.push(Action::FailPipeline {
            reason: "Missing label.".to_string(),
        });

        let comment = render_summary_comment(&outcome);

        assert!(comment.contains(MR_MILCHICK_MARKER));
        assert!(comment.contains("## Mr. Milchick Review Summary"));
        assert!(comment.contains("**Blocking**: Missing label."));
        assert!(comment.contains("Fail pipeline: Missing label."));
    }
}