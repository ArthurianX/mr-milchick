use std::collections::{BTreeMap, BTreeSet};

use crate::core::context::model::{CiContext, PipelineState};
use crate::core::message_templates::{PipelineStatusState, PipelineStatusTemplateEntry};
use crate::core::model::{
    ApprovalRuleState, GitLabLabelRule, LabelRuleCondition, LabelRulePredicate, ReviewAction,
    ReviewPlatformKind, ReviewSnapshot,
};
use crate::core::rules::model::{RuleFinding, RuleOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LabelDecision {
    Add,
    Remove,
}

pub fn enrich_with_gitlab_label_rules(
    mut outcome: RuleOutcome,
    ctx: &CiContext,
    snapshot: &ReviewSnapshot,
    rules: &[GitLabLabelRule],
    pipeline_statuses: &[PipelineStatusTemplateEntry],
) -> RuleOutcome {
    if rules.is_empty() || snapshot.review_ref.platform != ReviewPlatformKind::GitLab {
        return outcome;
    }

    let facts = LabelRuleFacts::from_inputs(ctx, snapshot, pipeline_statuses);
    let current_labels = snapshot.labels.iter().cloned().collect::<BTreeSet<_>>();
    let mut decisions = BTreeMap::new();
    let mut matched_rules = Vec::new();

    for rule in rules {
        if !matches_condition(&rule.when, &facts) {
            continue;
        }

        matched_rules.push(rule.name.clone());
        for label in &rule.add {
            decisions.insert(label.clone(), LabelDecision::Add);
        }
        for label in &rule.remove {
            decisions.insert(label.clone(), LabelDecision::Remove);
        }
    }

    if matched_rules.is_empty() {
        return outcome;
    }

    let labels_to_add = decisions
        .iter()
        .filter_map(|(label, decision)| {
            (*decision == LabelDecision::Add && !current_labels.contains(label))
                .then(|| label.clone())
        })
        .collect::<Vec<_>>();
    let labels_to_remove = decisions
        .iter()
        .filter_map(|(label, decision)| {
            (*decision == LabelDecision::Remove && current_labels.contains(label))
                .then(|| label.clone())
        })
        .collect::<Vec<_>>();

    let planned_label_action = !labels_to_add.is_empty() || !labels_to_remove.is_empty();

    if !labels_to_add.is_empty() {
        outcome.action_plan.push(ReviewAction::AddLabels {
            labels: labels_to_add,
        });
    }
    if !labels_to_remove.is_empty() {
        outcome.action_plan.push(ReviewAction::RemoveLabels {
            labels: labels_to_remove,
        });
    }

    if planned_label_action {
        outcome.push(RuleFinding::info(format!(
            "GitLab label rules matched: {}.",
            matched_rules.join(", ")
        )));
    }

    outcome
}

#[derive(Debug)]
struct LabelRuleFacts<'a> {
    ctx: &'a CiContext,
    snapshot: &'a ReviewSnapshot,
    pipeline_state: PipelineState,
}

impl<'a> LabelRuleFacts<'a> {
    fn from_inputs(
        ctx: &'a CiContext,
        snapshot: &'a ReviewSnapshot,
        pipeline_statuses: &[PipelineStatusTemplateEntry],
    ) -> Self {
        Self {
            ctx,
            snapshot,
            pipeline_state: effective_pipeline_state(ctx.pipeline.state, pipeline_statuses),
        }
    }

    fn approval_state(&self) -> ApprovalRuleState {
        match (
            self.snapshot.metadata.approvals_required,
            self.snapshot.metadata.approvals_given,
        ) {
            (Some(required), Some(given)) if required > 0 && given >= required => {
                ApprovalRuleState::Satisfied
            }
            (Some(_), Some(_)) => ApprovalRuleState::Missing,
            _ => ApprovalRuleState::Unavailable,
        }
    }
}

fn matches_condition(condition: &LabelRuleCondition, facts: &LabelRuleFacts<'_>) -> bool {
    let all_matches = condition
        .all
        .iter()
        .all(|predicate| matches_predicate(predicate, facts));
    let any_matches = condition.any.is_empty()
        || condition
            .any
            .iter()
            .any(|predicate| matches_predicate(predicate, facts));

    all_matches && any_matches
}

fn matches_predicate(predicate: &LabelRulePredicate, facts: &LabelRuleFacts<'_>) -> bool {
    match predicate {
        LabelRulePredicate::Draft(expected) => facts.snapshot.is_draft == *expected,
        LabelRulePredicate::MergeRequestState(expected) => {
            facts
                .snapshot
                .metadata
                .merge_request_state
                .as_deref()
                .map(normalize_state)
                .as_deref()
                == Some(expected.as_str())
        }
        LabelRulePredicate::PipelineState(expected) => facts.pipeline_state == *expected,
        LabelRulePredicate::Approvals(expected) => facts.approval_state() == *expected,
        LabelRulePredicate::HasLabel(expected) => {
            facts.snapshot.labels.iter().any(|label| label == expected)
        }
        LabelRulePredicate::SourceBranch(expected) => facts.ctx.source_branch() == expected,
        LabelRulePredicate::TargetBranch(expected) => facts.ctx.target_branch() == expected,
        LabelRulePredicate::SourceBranchKind(expected) => {
            facts.ctx.source_branch_kind() == *expected
        }
    }
}

fn effective_pipeline_state(
    explicit: PipelineState,
    pipeline_statuses: &[PipelineStatusTemplateEntry],
) -> PipelineState {
    if explicit != PipelineState::Unknown {
        return explicit;
    }

    if pipeline_statuses.is_empty() {
        return PipelineState::Unknown;
    }

    if pipeline_statuses
        .iter()
        .any(|entry| entry.state == PipelineStatusState::Failed)
    {
        return PipelineState::Failed;
    }

    if pipeline_statuses
        .iter()
        .all(|entry| entry.state == PipelineStatusState::Passed)
    {
        return PipelineState::Passed;
    }

    PipelineState::Unknown
}

fn normalize_state(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::context::model::{
        BranchInfo, BranchName, PipelineInfo, PipelineSource, ProjectKey, ReviewContextRef,
        ReviewId,
    };
    use crate::core::model::{
        Actor, ChangedFile, LabelRuleCondition, RepositoryRef, ReviewMetadata, ReviewRef,
    };
    use crate::core::rules::model::RuleOutcome;

    fn ctx(pipeline_state: PipelineState) -> CiContext {
        CiContext {
            project_key: ProjectKey("123".to_string()),
            review: Some(ReviewContextRef {
                id: ReviewId("456".to_string()),
            }),
            pipeline: PipelineInfo {
                source: PipelineSource::ReviewEvent,
                state: pipeline_state,
            },
            branches: BranchInfo {
                source: BranchName("feat/label-rules".to_string()),
                target: BranchName("develop".to_string()),
            },
            labels: vec![],
        }
    }

    fn snapshot(
        labels: &[&str],
        draft: bool,
        required: Option<u32>,
        given: Option<u32>,
    ) -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitLab,
                project_key: "123".to_string(),
                review_id: "456".to_string(),
                web_url: None,
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitLab,
                namespace: "group".to_string(),
                name: "project".to_string(),
                web_url: None,
            },
            title: "Label rules".to_string(),
            description: None,
            author: Actor {
                username: "author".to_string(),
                display_name: None,
            },
            participants: vec![],
            changed_files: Vec::<ChangedFile>::new(),
            labels: labels.iter().map(|label| (*label).to_string()).collect(),
            is_draft: draft,
            default_branch: Some("develop".to_string()),
            metadata: ReviewMetadata {
                source_branch: Some("feat/label-rules".to_string()),
                target_branch: Some("develop".to_string()),
                merge_request_state: Some("opened".to_string()),
                commit_count: None,
                approvals_required: required,
                approvals_given: given,
            },
        }
    }

    fn rule(
        name: &str,
        add: &[&str],
        remove: &[&str],
        when: LabelRuleCondition,
    ) -> GitLabLabelRule {
        GitLabLabelRule {
            name: name.to_string(),
            add: add.iter().map(|label| (*label).to_string()).collect(),
            remove: remove.iter().map(|label| (*label).to_string()).collect(),
            when,
        }
    }

    #[test]
    fn draft_rule_removes_ready_labels() {
        let outcome = enrich_with_gitlab_label_rules(
            RuleOutcome::new(),
            &ctx(PipelineState::Unknown),
            &snapshot(&["Ready for review"], true, None, None),
            &[rule(
                "draft-cleanup",
                &[],
                &["Ready for review"],
                LabelRuleCondition {
                    all: vec![LabelRulePredicate::Draft(true)],
                    any: vec![],
                },
            )],
            &[],
        );

        assert!(matches!(
            outcome.action_plan.actions.as_slice(),
            [ReviewAction::RemoveLabels { labels }] if labels == &vec!["Ready for review".to_string()]
        ));
    }

    #[test]
    fn failed_pipeline_adds_failure_label() {
        let outcome = enrich_with_gitlab_label_rules(
            RuleOutcome::new(),
            &ctx(PipelineState::Failed),
            &snapshot(&[], false, None, None),
            &[rule(
                "tests-failing",
                &["Tests are failing"],
                &["Ready for Testing"],
                LabelRuleCondition {
                    all: vec![LabelRulePredicate::PipelineState(PipelineState::Failed)],
                    any: vec![],
                },
            )],
            &[],
        );

        assert!(matches!(
            outcome.action_plan.actions.as_slice(),
            [ReviewAction::AddLabels { labels }] if labels == &vec!["Tests are failing".to_string()]
        ));
    }

    #[test]
    fn approvals_satisfied_adds_ready_for_testing() {
        let outcome = enrich_with_gitlab_label_rules(
            RuleOutcome::new(),
            &ctx(PipelineState::Passed),
            &snapshot(&["Ready for review"], false, Some(2), Some(2)),
            &[rule(
                "ready-for-testing",
                &["Ready for Testing"],
                &["Ready for review"],
                LabelRuleCondition {
                    all: vec![
                        LabelRulePredicate::PipelineState(PipelineState::Passed),
                        LabelRulePredicate::Approvals(ApprovalRuleState::Satisfied),
                    ],
                    any: vec![],
                },
            )],
            &[],
        );

        assert_eq!(outcome.action_plan.actions.len(), 2);
        assert!(outcome.action_plan.actions.iter().any(|action| {
            matches!(action, ReviewAction::AddLabels { labels } if labels == &vec!["Ready for Testing".to_string()])
        }));
        assert!(outcome.action_plan.actions.iter().any(|action| {
            matches!(action, ReviewAction::RemoveLabels { labels } if labels == &vec!["Ready for review".to_string()])
        }));
    }

    #[test]
    fn later_matching_rule_wins_same_label() {
        let rules = vec![
            rule(
                "add-ready",
                &["Ready for review"],
                &[],
                LabelRuleCondition {
                    all: vec![LabelRulePredicate::Draft(false)],
                    any: vec![],
                },
            ),
            rule(
                "remove-ready",
                &[],
                &["Ready for review"],
                LabelRuleCondition {
                    all: vec![LabelRulePredicate::PipelineState(PipelineState::Failed)],
                    any: vec![],
                },
            ),
        ];

        let outcome = enrich_with_gitlab_label_rules(
            RuleOutcome::new(),
            &ctx(PipelineState::Failed),
            &snapshot(&["Ready for review"], false, None, None),
            &rules,
            &[],
        );

        assert!(matches!(
            outcome.action_plan.actions.as_slice(),
            [ReviewAction::RemoveLabels { labels }] if labels == &vec!["Ready for review".to_string()]
        ));
    }

    #[test]
    fn status_files_supply_pipeline_state_when_env_is_unknown() {
        let outcome = enrich_with_gitlab_label_rules(
            RuleOutcome::new(),
            &ctx(PipelineState::Unknown),
            &snapshot(&[], false, None, None),
            &[rule(
                "status-file-green",
                &["Ready for Testing"],
                &[],
                LabelRuleCondition {
                    all: vec![LabelRulePredicate::PipelineState(PipelineState::Passed)],
                    any: vec![],
                },
            )],
            &[PipelineStatusTemplateEntry {
                label: "unit_tests".to_string(),
                state: PipelineStatusState::Passed,
                detail: None,
            }],
        );

        assert!(matches!(
            outcome.action_plan.actions.as_slice(),
            [ReviewAction::AddLabels { labels }] if labels == &vec!["Ready for Testing".to_string()]
        ));
    }
}
