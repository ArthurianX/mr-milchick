use crate::actions::model::Action;
use crate::domain::reviewer_routing::{recommend_reviewers, ReviewerRoutingConfig};
use crate::domain::snapshot_analysis::summarize_areas;
use crate::gitlab::api::MergeRequestSnapshot;
use crate::rules::model::{RuleFinding, RuleOutcome};

pub fn enrich_with_reviewer_assignment(
    mut outcome: RuleOutcome,
    snapshot: &MergeRequestSnapshot,
    routing_config: &ReviewerRoutingConfig,
) -> RuleOutcome {
    let area_summary = summarize_areas(snapshot);
    let excluded_reviewers = vec![snapshot.details.author_username.clone()];
    let recommendation =
        recommend_reviewers(&area_summary, routing_config, &excluded_reviewers);

    if snapshot.details.is_draft {
        if !recommendation.reviewers.is_empty() {
            outcome.push(RuleFinding::info(
                "Reviewer assignment is deferred because the merge request is draft.",
            ));
        }

        return outcome;
    }

    if !recommendation.reviewers.is_empty() {
        outcome.action_plan.push(Action::AssignReviewers {
            reviewers: recommendation.reviewers,
        });
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::reviewer_routing::ReviewerRoutingConfig;
    use crate::gitlab::api::{
        ChangedFile, MergeRequestDetails, MergeRequestSnapshot, MergeRequestState,
    };
    use crate::rules::model::RuleOutcome;

    fn sample_snapshot(is_draft: bool) -> MergeRequestSnapshot {
        MergeRequestSnapshot {
            details: MergeRequestDetails {
                iid: 1,
                title: "Frontend adjustments".to_string(),
                description: None,
                state: MergeRequestState::Opened,
                is_draft,
                web_url: "https://gitlab.example.com/mr/1".to_string(),
                author_username: "alice".to_string(),
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
        let snapshot = sample_snapshot(false);
        let config = ReviewerRoutingConfig::example();

        let enriched = enrich_with_reviewer_assignment(outcome, &snapshot, &config);

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
        let snapshot = sample_snapshot(true);
        let config = ReviewerRoutingConfig::example();

        let enriched = enrich_with_reviewer_assignment(outcome, &snapshot, &config);

        assert!(enriched.action_plan.is_empty());
        assert_eq!(enriched.findings.len(), 1);
        assert!(enriched.findings[0]
            .message
            .contains("deferred because the merge request is draft"));
    }
}