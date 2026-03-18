use crate::actions::model::Action;
use crate::context::model::CiContext;
use crate::rules::model::{FindingSeverity, RuleOutcome};
use crate::tone::{ToneCategory, ToneSelector};

pub const MR_MILCHICK_MARKER: &str = "<!-- mr-milchick:summary -->";

pub fn render_summary_comment(
    outcome: &RuleOutcome,
    ctx: &CiContext,
    selector: &ToneSelector,
) -> String {
    let mut lines = Vec::new();

    lines.push(MR_MILCHICK_MARKER.to_string());
    lines.push("## Mr. Milchick Review Summary".to_string());
    lines.push(String::new());
    lines.push(selector.select(ToneCategory::Observation, ctx).to_string());
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

    lines.push("### Planned Actions".to_string());
    lines.push(String::new());

    let mut rendered_action = false;

    for action in &outcome.action_plan.actions {
        let text = match action {
            Action::PostComment { .. } => continue,
            Action::AssignReviewers { reviewers } => {
                format!("Assign reviewers: {}", reviewers.join(", "))
            }
            Action::FailPipeline { reason } => format!("Fail pipeline: {}", reason),
        };

        rendered_action = true;
        lines.push(format!("- {}", text));
    }

    if !rendered_action {
        lines.push("- None".to_string());
    }

    lines.push(String::new());

    let closing_category =
        if outcome.has_blocking_findings() || outcome.action_plan.has_fail_pipeline() {
            ToneCategory::Blocking
        } else if outcome.is_empty() && outcome.action_plan.is_empty() {
            ToneCategory::NoAction
        } else if outcome.findings.is_empty() {
            ToneCategory::Praise
        } else {
            ToneCategory::Refinement
        };

    lines.push(format!("_{}_", selector.select(closing_category, ctx)));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::model::{Action, ActionPlan};
    use crate::context::model::{
        BranchInfo, BranchName, CiContext, Label, MergeRequestIid, MergeRequestRef, PipelineInfo,
        PipelineSource, ProjectId,
    };
    use crate::rules::model::{RuleFinding, RuleOutcome};
    use crate::tone::ToneSelector;

    fn sample_context() -> CiContext {
        CiContext {
            project_id: ProjectId("123".to_string()),
            merge_request: Some(MergeRequestRef {
                iid: MergeRequestIid("456".to_string()),
            }),
            pipeline: PipelineInfo {
                source: PipelineSource::MergeRequestEvent,
            },
            branches: BranchInfo {
                source: BranchName("feat/test".to_string()),
                target: BranchName("develop".to_string()),
            },
            labels: vec![Label("backend".to_string())],
        }
    }

    #[test]
    fn renders_structured_summary_comment() {
        let mut outcome = RuleOutcome {
            findings: vec![RuleFinding::blocking("Missing label.")],
            action_plan: ActionPlan::new(),
        };
        outcome.action_plan.push(Action::FailPipeline {
            reason: "Missing label.".to_string(),
        });
        let ctx = sample_context();
        let selector = ToneSelector::default();

        let comment = render_summary_comment(&outcome, &ctx, &selector);

        assert!(comment.contains(MR_MILCHICK_MARKER));
        assert!(comment.contains("## Mr. Milchick Review Summary"));
        assert!(comment.contains("**Blocking**: Missing label."));
        assert!(comment.contains("Fail pipeline: Missing label."));
        assert!(comment.contains("Mr. Milchick"));
        assert!(comment.contains("_"));
    }

    #[test]
    fn renders_no_action_closing_for_empty_outcome() {
        let outcome = RuleOutcome {
            findings: vec![],
            action_plan: ActionPlan::new(),
        };
        let ctx = sample_context();
        let selector = ToneSelector::default();

        let comment = render_summary_comment(&outcome, &ctx, &selector);

        assert!(comment.contains("No findings were produced."));
        assert!(comment.contains("### Planned Actions"));
        assert!(comment.contains("- None"));
    }
}
