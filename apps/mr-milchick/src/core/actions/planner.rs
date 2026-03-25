use crate::core::actions::model::Action;
use crate::core::domain::codeowners::context::CodeownersContext;
use crate::core::domain::codeowners::planner::plan_codeowners_assignments;
use crate::core::domain::reviewer_routing::{
    ReviewerRoutingConfig, prepend_mandatory_reviewers, recommend_reviewers,
};
use crate::core::domain::snapshot_analysis::summarize_areas;
use crate::core::model::{Actor, ReviewSnapshot};
use crate::core::rules::model::{RuleFinding, RuleOutcome};
use tracing::{debug, info, instrument};

#[instrument(
    skip_all,
    fields(
        merge_request_iid = snapshot.review_ref.review_id,
        changed_files = snapshot.changed_file_count(),
        draft = snapshot.is_draft,
        existing_reviewers = snapshot.participants.len()
    )
)]
pub fn enrich_with_reviewer_assignment(
    mut outcome: RuleOutcome,
    snapshot: &ReviewSnapshot,
    routing_config: &ReviewerRoutingConfig,
    codeowners_ctx: &CodeownersContext,
) -> RuleOutcome {
    let area_summary = summarize_areas(snapshot);
    let excluded_reviewers = vec![snapshot.author.username.clone()];
    let mut recommendation =
        recommend_reviewers(&area_summary, routing_config, &excluded_reviewers);
    debug!(
        significant_areas = area_summary.significant_areas().len(),
        recommendation_count = recommendation.reviewers.len(),
        reasons = recommendation.reasons.len(),
        "base reviewer recommendation computed"
    );

    if let Some(file) = &codeowners_ctx.file {
        let codeowners_plan = plan_codeowners_assignments(file, snapshot);
        debug!(
            matched_sections = codeowners_plan.matched_sections.len(),
            assigned_reviewers = codeowners_plan.assigned_reviewers.len(),
            uncovered_sections = codeowners_plan.uncovered_sections.len(),
            reasons = codeowners_plan.reasons.len(),
            "CODEOWNERS assignment plan computed"
        );

        if !codeowners_plan.matched_sections.is_empty() {
            recommendation = prepend_mandatory_reviewers(
                routing_config,
                &excluded_reviewers,
                &codeowners_plan.assigned_reviewers,
                &codeowners_plan.reasons,
            );

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
                info!(
                    uncovered_sections = codeowners_plan.uncovered_sections.len(),
                    "CODEOWNERS coverage gaps detected"
                );
            }
        }
    }

    if snapshot.is_draft {
        if !recommendation.reviewers.is_empty() {
            outcome.push(RuleFinding::info(
                "Reviewer assignment is deferred because the merge request is draft.",
            ));
        }
        info!(
            deferred_reviewer_count = recommendation.reviewers.len(),
            "reviewer assignment deferred for draft merge request"
        );

        return outcome;
    }

    let recommended_reviewers = recommendation.reviewers.len();
    let existing_reviewers = snapshot.reviewer_usernames();
    let missing_reviewers: Vec<Actor> = recommendation
        .reviewers
        .into_iter()
        .filter(|reviewer| {
            !existing_reviewers
                .iter()
                .any(|existing| existing == reviewer)
        })
        .map(|username| Actor {
            username,
            display_name: None,
        })
        .collect();
    debug!(
        recommended_reviewers,
        missing_reviewers = missing_reviewers.len(),
        "filtered already-assigned reviewers from recommendation"
    );

    if missing_reviewers.is_empty() {
        if !existing_reviewers.is_empty() {
            outcome.push(RuleFinding::info(
                "All recommended reviewers are already assigned.",
            ));
        }
        info!("no reviewer assignment action needed");

        return outcome;
    }

    outcome.action_plan.push(Action::AssignReviewers {
        reviewers: missing_reviewers,
    });
    info!(
        action_count = outcome.action_plan.actions.len(),
        "planned reviewer assignment action"
    );

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::codeowners::context::CodeownersContext;
    use crate::core::domain::reviewer_routing::ReviewerRoutingConfig;
    use crate::core::model::{
        ChangeType, ChangedFile, RepositoryRef, ReviewMetadata, ReviewPlatformKind, ReviewRef,
    };
    use crate::core::rules::model::RuleOutcome;

    fn sample_snapshot(is_draft: bool, existing_reviewers: Vec<&str>) -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitLab,
                project_key: "123".to_string(),
                review_id: "1".to_string(),
                web_url: Some("https://gitlab.example.com/mr/1".to_string()),
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitLab,
                namespace: "group".to_string(),
                name: "project".to_string(),
                web_url: Some("https://gitlab.example.com/group/project".to_string()),
            },
            title: "Frontend adjustments".to_string(),
            description: None,
            author: Actor {
                username: "alice".to_string(),
                display_name: None,
            },
            participants: existing_reviewers
                .into_iter()
                .map(|username| Actor {
                    username: username.to_string(),
                    display_name: None,
                })
                .collect(),
            changed_files: vec![ChangedFile {
                path: "apps/frontend/button.tsx".to_string(),
                change_type: ChangeType::Modified,
                additions: None,
                deletions: None,
            }],
            labels: vec![],
            is_draft,
            default_branch: Some("develop".to_string()),
            metadata: ReviewMetadata::default(),
        }
    }

    fn reviewer_usernames(reviewers: &[Actor]) -> Vec<String> {
        reviewers
            .iter()
            .map(|reviewer| reviewer.username.clone())
            .collect()
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
                assert_eq!(
                    reviewer_usernames(reviewers),
                    vec!["principal-reviewer".to_string(), "bob".to_string()]
                );
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
    }

    #[test]
    fn does_not_plan_assignment_when_recommended_reviewers_are_already_present() {
        let outcome = RuleOutcome::new();
        let snapshot = sample_snapshot(false, vec!["principal-reviewer", "bob"]);
        let config = ReviewerRoutingConfig::example();

        let enriched = enrich_with_reviewer_assignment(
            outcome,
            &snapshot,
            &config,
            &CodeownersContext::empty(),
        );

        assert!(enriched.action_plan.is_empty());
        assert_eq!(enriched.findings.len(), 1);
    }

    #[test]
    fn prefers_codeowners_reviewers_when_provided() {
        let outcome = RuleOutcome::new();
        let snapshot = sample_snapshot(false, vec![]);
        let config = ReviewerRoutingConfig::example();
        let codeowners = CodeownersContext {
            enabled: true,
            file: Some(
                crate::core::domain::codeowners::parser::parse_codeowners_str(
                    r#"
[Frontend][1] @anon03
/apps/frontend/
"#,
                )
                .expect("codeowners should parse"),
            ),
        };

        let enriched = enrich_with_reviewer_assignment(outcome, &snapshot, &config, &codeowners);

        match &enriched.action_plan.actions[0] {
            Action::AssignReviewers { reviewers } => {
                assert_eq!(
                    reviewer_usernames(reviewers),
                    vec!["principal-reviewer".to_string(), "anon03".to_string()]
                );
            }
            _ => panic!("expected AssignReviewers action"),
        }
    }
}
