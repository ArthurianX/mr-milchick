use crate::actions::model::Action;
use crate::domain::codeowners::context::CodeownersContext;
use crate::domain::codeowners::planner::plan_codeowners_assignments;
use crate::domain::reviewer_routing::{ReviewerRoutingConfig, recommend_reviewers};
use crate::domain::snapshot_analysis::summarize_areas;
use crate::gitlab::api::MergeRequestSnapshot;
use crate::rules::model::{RuleFinding, RuleOutcome};

pub fn enrich_with_reviewer_assignment(
    mut outcome: RuleOutcome,
    snapshot: &MergeRequestSnapshot,
    routing_config: &ReviewerRoutingConfig,
    codeowners_ctx: &CodeownersContext,
) -> RuleOutcome {
    let area_summary = summarize_areas(snapshot);
    let excluded_reviewers = vec![snapshot.details.author_username.clone()];
    let mut recommendation =
        recommend_reviewers(&area_summary, routing_config, &excluded_reviewers);

    if let Some(file) = &codeowners_ctx.file {
        let codeowners_plan = plan_codeowners_assignments(file, snapshot);

        if !codeowners_plan.matched_sections.is_empty() {
            recommendation.reviewers = codeowners_plan.assigned_reviewers.clone();
            recommendation.reasons = codeowners_plan.reasons.clone();

            if !codeowners_plan.uncovered_sections.is_empty() {
                outcome.push(RuleFinding::warning(format!(
                    "CODEOWNERS coverage is incomplete for {}.",
                    codeowners_plan
                        .uncovered_sections
                        .iter()
                        .map(|gap| gap.section_name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
    }

    if snapshot.details.is_draft {
        if !recommendation.reviewers.is_empty() {
            outcome.push(RuleFinding::info(
                "Reviewer assignment is deferred because the merge request is draft.",
            ));
        }

        return outcome;
    }

    let missing_reviewers: Vec<String> = recommendation
        .reviewers
        .into_iter()
        .filter(|reviewer| {
            !snapshot
                .details
                .reviewer_usernames
                .iter()
                .any(|existing| existing == reviewer)
        })
        .collect();

    if missing_reviewers.is_empty() {
        if !snapshot.details.reviewer_usernames.is_empty() {
            outcome.push(RuleFinding::info(
                "All recommended reviewers are already assigned.",
            ));
        }

        return outcome;
    }

    outcome.action_plan.push(Action::AssignReviewers {
        reviewers: missing_reviewers,
    });

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::codeowners::context::CodeownersContext;
    use crate::domain::reviewer_routing::ReviewerRoutingConfig;
    use crate::gitlab::api::{
        ChangedFile, MergeRequestDetails, MergeRequestSnapshot, MergeRequestState,
    };
    use crate::rules::model::RuleOutcome;

    fn sample_snapshot(is_draft: bool, existing_reviewers: Vec<&str>) -> MergeRequestSnapshot {
        MergeRequestSnapshot {
            details: MergeRequestDetails {
                iid: 1,
                title: "Frontend adjustments".to_string(),
                description: None,
                state: MergeRequestState::Opened,
                is_draft,
                web_url: "https://gitlab.example.com/mr/1".to_string(),
                author_username: "alice".to_string(),
                reviewer_usernames: existing_reviewers
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect(),
            },
            changed_files: vec![ChangedFile {
                old_path: "apps/frontend/button_old.tsx".to_string(),
                new_path: "apps/frontend/button.tsx".to_string(),
                is_new: false,
                is_renamed: false,
                is_deleted: false,
            }],
        }
    }

    #[test]
    fn adds_assign_reviewers_action_when_recommendation_exists_for_non_draft() {
        let outcome = RuleOutcome::new();
        let snapshot = sample_snapshot(false, vec![]);
        let config = ReviewerRoutingConfig::example();

        let enriched = enrich_with_reviewer_assignment(
            outcome,
            &snapshot,
            &config,
            &CodeownersContext::empty(),
        );

        assert_eq!(enriched.action_plan.actions.len(), 1);

        match &enriched.action_plan.actions[0] {
            Action::AssignReviewers { reviewers } => {
                assert_eq!(reviewers, &vec!["bob".to_string()]);
            }
            _ => panic!("expected AssignReviewers action"),
        }
    }

    #[test]
    fn does_not_assign_reviewers_for_draft_merge_request() {
        let outcome = RuleOutcome::new();
        let snapshot = sample_snapshot(true, vec![]);
        let config = ReviewerRoutingConfig::example();

        let enriched = enrich_with_reviewer_assignment(
            outcome,
            &snapshot,
            &config,
            &CodeownersContext::empty(),
        );

        assert!(enriched.action_plan.is_empty());
        assert_eq!(enriched.findings.len(), 1);
        assert!(
            enriched.findings[0]
                .message
                .contains("deferred because the merge request is draft")
        );
    }

    #[test]
    fn does_not_plan_assignment_when_recommended_reviewers_are_already_present() {
        let outcome = RuleOutcome::new();
        let snapshot = sample_snapshot(false, vec!["bob"]);
        let config = ReviewerRoutingConfig::example();

        let enriched = enrich_with_reviewer_assignment(
            outcome,
            &snapshot,
            &config,
            &CodeownersContext::empty(),
        );

        assert!(enriched.action_plan.is_empty());
        assert_eq!(enriched.findings.len(), 1);
        assert!(enriched.findings[0].message.contains("already assigned"));
    }

    #[test]
    fn only_plans_missing_reviewers() {
        let outcome = RuleOutcome::new();

        let snapshot = MergeRequestSnapshot {
            details: MergeRequestDetails {
                iid: 1,
                title: "Cross-area adjustments".to_string(),
                description: None,
                state: MergeRequestState::Opened,
                is_draft: false,
                web_url: "https://gitlab.example.com/mr/1".to_string(),
                author_username: "alice".to_string(),
                reviewer_usernames: vec!["carol".to_string()],
            },
            changed_files: vec![
                ChangedFile {
                    old_path: "services/api/old.rs".to_string(),
                    new_path: "services/api/main.rs".to_string(),
                    is_new: false,
                    is_renamed: false,
                    is_deleted: false,
                },
                ChangedFile {
                    old_path: "apps/frontend/old.tsx".to_string(),
                    new_path: "apps/frontend/app.tsx".to_string(),
                    is_new: false,
                    is_renamed: false,
                    is_deleted: false,
                },
            ],
        };

        let mut config = ReviewerRoutingConfig::example();
        config.max_reviewers = 2;

        let enriched = enrich_with_reviewer_assignment(
            outcome,
            &snapshot,
            &config,
            &CodeownersContext::empty(),
        );

        assert_eq!(enriched.action_plan.actions.len(), 1);

        match &enriched.action_plan.actions[0] {
            Action::AssignReviewers { reviewers } => {
                assert_eq!(reviewers, &vec!["bob".to_string()]);
            }
            _ => panic!("expected AssignReviewers action"),
        }
    }

    #[test]
    fn prefers_codeowners_reviewers_when_provided() {
        let outcome = RuleOutcome::new();
        let snapshot = sample_snapshot(false, vec![]);
        let config = ReviewerRoutingConfig::example();

        let codeowners = CodeownersContext {
            enabled: true,
            file: Some(
                crate::domain::codeowners::parser::parse_codeowners_str(
                    r#"
[Frontend][1] @daniel.andrei
/apps/frontend/
"#,
                )
                .expect("codeowners should parse"),
            ),
        };

        let enriched = enrich_with_reviewer_assignment(outcome, &snapshot, &config, &codeowners);

        assert_eq!(enriched.action_plan.actions.len(), 1);

        match &enriched.action_plan.actions[0] {
            Action::AssignReviewers { reviewers } => {
                assert_eq!(reviewers, &vec!["daniel.andrei".to_string()]);
            }
            _ => panic!("expected AssignReviewers action"),
        }
    }
}
