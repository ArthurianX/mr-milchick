use crate::context::model::CiContext;
use crate::model::{MessageSection, RenderedMessage, ReviewAction};
use crate::rules::model::{FindingSeverity, RuleOutcome};
use crate::tone::{ToneCategory, ToneSelector};

pub fn build_summary_message(
    outcome: &RuleOutcome,
    ctx: &CiContext,
    selector: &ToneSelector,
) -> RenderedMessage {
    let mut message = RenderedMessage::new(Some("Mr. Milchick Review Summary".to_string()));
    message.sections.push(MessageSection::Paragraph(
        selector.select(ToneCategory::Observation, ctx).to_string(),
    ));

    if outcome.findings.is_empty() {
        message.sections.push(MessageSection::Paragraph(
            "No findings were produced.".to_string(),
        ));
    } else {
        message.sections.push(MessageSection::KeyValueList(
            outcome
                .findings
                .iter()
                .map(|finding| (finding_label(&finding.severity), finding.message.clone()))
                .collect(),
        ));
    }

    let actions = outcome
        .action_plan
        .actions
        .iter()
        .filter_map(describe_action)
        .collect::<Vec<_>>();

    message
        .sections
        .push(MessageSection::BulletList(if actions.is_empty() {
            vec!["None".to_string()]
        } else {
            actions
        }));

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

    message.footer = Some(selector.select(closing_category, ctx).to_string());
    message
}

fn finding_label(severity: &FindingSeverity) -> String {
    match severity {
        FindingSeverity::Info => "Info".to_string(),
        FindingSeverity::Warning => "Warning".to_string(),
        FindingSeverity::Blocking => "Blocking".to_string(),
    }
}

fn describe_action(action: &ReviewAction) -> Option<String> {
    match action {
        ReviewAction::AssignReviewers { reviewers } => Some(format!(
            "Assign reviewers: {}",
            reviewers
                .iter()
                .map(|reviewer| format!("@{}", reviewer.username))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        ReviewAction::UpsertSummary { .. } => None,
        ReviewAction::AddLabels { labels } => Some(format!("Add labels: {}", labels.join(", "))),
        ReviewAction::RemoveLabels { labels } => {
            Some(format!("Remove labels: {}", labels.join(", ")))
        }
        ReviewAction::FailPipeline { reason } => Some(format!("Fail pipeline: {}", reason)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::model::ActionPlan;
    use crate::context::model::{
        BranchInfo, BranchName, CiContext, Label, MergeRequestIid, MergeRequestRef, PipelineInfo,
        PipelineSource, ProjectId,
    };
    use crate::model::{Actor, ReviewAction};
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
    fn builds_structured_summary_message() {
        let mut outcome = RuleOutcome {
            findings: vec![RuleFinding::blocking("Missing label.")],
            action_plan: ActionPlan::new(),
        };
        outcome.action_plan.push(ReviewAction::AssignReviewers {
            reviewers: vec![
                Actor {
                    username: "principal-reviewer".to_string(),
                    display_name: None,
                },
                Actor {
                    username: "bob".to_string(),
                    display_name: None,
                },
            ],
        });
        outcome.action_plan.push(ReviewAction::FailPipeline {
            reason: "Missing label.".to_string(),
        });
        let ctx = sample_context();
        let selector = ToneSelector::default();

        let message = build_summary_message(&outcome, &ctx, &selector);

        assert_eq!(
            message.title.as_deref(),
            Some("Mr. Milchick Review Summary")
        );
        assert!(matches!(
            message.sections[1],
            MessageSection::KeyValueList(_)
        ));
        assert!(matches!(message.sections[2], MessageSection::BulletList(_)));
        assert!(message.footer.is_some());
    }
}
