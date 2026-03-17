use anyhow::Result;
use tracing::{debug, info, instrument, warn};

use crate::actions::executor::{ActionExecutor, DryRunExecutor, ExecutedAction, ExecutionReport};
use crate::actions::model::Action;
use crate::actions::planner::enrich_with_reviewer_assignment;
use crate::actions::runtime::ExecutionStrategy;
use crate::cli::Cli;
use crate::comment::render::{MR_MILCHICK_MARKER, render_summary_comment};
use crate::config::loader::{load_config, resolve_codeowners_path};
use crate::config::model::SlackConfig;
use crate::context::builder::build_ci_context;
use crate::domain::codeowners::context::CodeownersContext;
use crate::domain::codeowners::matcher::{collect_matched_rules_for_snapshot, match_usernames};
use crate::domain::codeowners::parser::parse_codeowners_file;
use crate::domain::codeowners::planner::plan_codeowners_assignments;
use crate::domain::reviewer_routing::{
    ReviewerRoutingConfig, prepend_mandatory_reviewers, recommend_reviewers,
};
use crate::domain::snapshot_analysis::summarize_areas;
use crate::gitlab::api::{GitLabConfig, MergeRequestSnapshot};
use crate::gitlab::client::GitLabClient;
use crate::notifications::slack::{SlackNotifier, render_review_request_message};
use crate::rules::engine::evaluate_rules;
use crate::rules::model::RuleOutcome;
use crate::tone::{ToneCategory, ToneSelector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Observe,
    Refine,
    Explain,
}

#[derive(Debug, Clone)]
struct AppConfigContext {
    routing_config: ReviewerRoutingConfig,
    codeowners: CodeownersContext,
    slack: SlackConfig,
}

fn load_app_config_context() -> Result<AppConfigContext> {
    let config = load_config()?;
    let routing_config = ReviewerRoutingConfig::from_config(&config.reviewers);
    let codeowners = CodeownersContext {
        enabled: config.codeowners.enabled,
        file: resolve_codeowners_path(&config.codeowners)
            .and_then(|path| parse_codeowners_file(&path).ok()),
    };

    Ok(AppConfigContext {
        routing_config,
        codeowners,
        slack: config.slack,
    })
}

fn has_non_comment_actions(plan: &crate::actions::model::ActionPlan) -> bool {
    plan.actions
        .iter()
        .any(|action| !matches!(action, Action::PostComment { .. }))
}

fn has_reviewer_assignment_action(plan: &crate::actions::model::ActionPlan) -> bool {
    plan.actions
        .iter()
        .any(|action| matches!(action, Action::AssignReviewers { .. }))
}

pub async fn run(cli: Cli) -> Result<()> {
    if matches!(cli.command, crate::cli::Command::Version) {
        crate::cli::print_version();
        return Ok(());
    }

    let mode: ExecutionMode = cli.command.into();
    run_mode(mode).await
}

#[instrument(skip_all, fields(mode = ?mode))]
pub async fn run_mode(mode: ExecutionMode) -> Result<()> {
    let ctx = build_ci_context()?;
    let selector = ToneSelector::default();
    info!(
        project_id = %ctx.project_id(),
        merge_request_iid = ctx.merge_request_iid().unwrap_or("none"),
        pipeline_source = ?ctx.pipeline.source,
        source_branch = %ctx.source_branch(),
        target_branch = %ctx.target_branch(),
        labels = ctx.labels.len(),
        "starting command execution"
    );

    println!("{}", selector.select(ToneCategory::Observation, &ctx));

    if !ctx.is_merge_request_pipeline() {
        info!("pipeline is not a merge request pipeline; exiting early");
        println!("This pipeline does not currently present merge request responsibilities.");
        return Ok(());
    }

    let mut outcome = evaluate_rules(&ctx);
    debug!(
        findings = outcome.findings.len(),
        planned_actions = outcome.action_plan.actions.len(),
        "rule evaluation completed"
    );
    let app_config = load_app_config_context()?;
    debug!(
        codeowners_enabled = app_config.codeowners.enabled,
        codeowners_file_loaded = app_config.codeowners.file.is_some(),
        slack_enabled = app_config.slack.enabled,
        reviewer_area_pools = app_config.routing_config.reviewers_by_area.len(),
        fallback_reviewers = app_config.routing_config.fallback_reviewers.len(),
        mandatory_reviewers = app_config.routing_config.mandatory_reviewers.len(),
        max_reviewers = app_config.routing_config.max_reviewers,
        "application configuration loaded"
    );

    let snapshot = maybe_fetch_snapshot(&ctx).await?;

    if let Some(snapshot) = &snapshot {
        outcome = enrich_with_reviewer_assignment(
            outcome,
            snapshot,
            &app_config.routing_config,
            &app_config.codeowners,
        );
        debug!(
            findings = outcome.findings.len(),
            planned_actions = outcome.action_plan.actions.len(),
            draft = snapshot.details.is_draft,
            changed_files = snapshot.changed_file_count(),
            "reviewer planning completed"
        );
    }

    let summary_comment = render_summary_comment(&outcome, &ctx, &selector);
    outcome.action_plan.push(Action::PostComment {
        body: summary_comment,
    });
    let has_meaningful_actions = has_non_comment_actions(&outcome.action_plan);
    info!(
        findings = outcome.findings.len(),
        action_count = outcome.action_plan.actions.len(),
        has_meaningful_actions,
        "final outcome assembled"
    );

    match mode {
        ExecutionMode::Observe => {
            print_outcome(&outcome);
            print_observe_action_plan(&outcome);

            if outcome.is_empty() && !has_meaningful_actions {
                println!("{}", selector.select(ToneCategory::Resolution, &ctx));
            } else if outcome.findings.is_empty()
                && has_reviewer_assignment_action(&outcome.action_plan)
            {
                println!("{}", selector.select(ToneCategory::ReviewerAssigned, &ctx));
            }
        }
        ExecutionMode::Refine => {
            if outcome.action_plan.has_fail_pipeline() || outcome.has_blocking_findings() {
                println!("{}", selector.select(ToneCategory::Blocking, &ctx));
            } else if outcome.is_empty() && !has_meaningful_actions {
                println!("{}", selector.select(ToneCategory::Resolution, &ctx));
            } else if outcome.findings.is_empty()
                && has_reviewer_assignment_action(&outcome.action_plan)
            {
                println!("{}", selector.select(ToneCategory::ReviewerAssigned, &ctx));
            } else {
                println!("{}", selector.select(ToneCategory::Refinement, &ctx));
            }

            print_outcome(&outcome);
            print_action_plan(&outcome);

            let strategy = ExecutionStrategy::from_env();
            info!(execution_strategy = ?strategy, "executing action plan");
            let report = execute_action_plan(strategy, &ctx, &outcome.action_plan).await?;
            debug!(
                executed_actions = report.executed.len(),
                "action execution completed"
            );
            print_execution_report(&report);
            maybe_notify_slack(
                strategy,
                &ctx,
                &outcome,
                &report,
                snapshot.as_ref(),
                &app_config.slack,
                &selector,
            )
            .await?;

            if outcome.action_plan.has_fail_pipeline() || outcome.has_blocking_findings() {
                warn!("failing command because blocking findings or fail-pipeline action remain");
                anyhow::bail!("merge request policy requirements were not satisfied");
            }
        }
        ExecutionMode::Explain => {
            info!("rendering explain output");
            println!("Decision explanation:");
            print_outcome(&outcome);
            print_action_plan(&outcome);
            let summary_comment = render_summary_comment(&outcome, &ctx, &selector);
            println!("Structured summary comment preview:");
            println!("---");
            println!("{}", summary_comment);
            println!("---");

            if let Some(snapshot) = &snapshot {
                print_snapshot_details(snapshot);

                if snapshot.details.is_draft {
                    println!(
                        "Reviewer assignment is currently deferred because this merge request is draft."
                    );
                }

                let area_summary = summarize_areas(snapshot);

                println!("Area summary:");
                for (area, count) in &area_summary.counts {
                    println!("- [{}] {}", area.as_str(), count);
                }

                if let Some(dominant) = area_summary.dominant_area() {
                    println!("Dominant area: {}", dominant.as_str());
                }

                let excluded_reviewers = vec![snapshot.details.author_username.clone()];
                let fallback_recommendation = recommend_reviewers(
                    &area_summary,
                    &app_config.routing_config,
                    &excluded_reviewers,
                );

                let mut recommendation_reviewers = fallback_recommendation.reviewers;
                let mut recommendation_reasons = fallback_recommendation.reasons;

                if let Some(codeowners) = &app_config.codeowners.file {
                    let codeowners_plan = plan_codeowners_assignments(codeowners, snapshot);

                    if !codeowners_plan.matched_sections.is_empty() {
                        println!("CODEOWNERS approval plan:");

                        for section in &codeowners_plan.matched_sections {
                            println!(
                                "- {} => needs {}, eligible [{}], paths [{}]",
                                section.section_name,
                                section.required_approvals,
                                section.eligible_users.join(", "),
                                section.matched_paths.join(", ")
                            );
                        }

                        let recommendation = prepend_mandatory_reviewers(
                            &app_config.routing_config,
                            &excluded_reviewers,
                            &codeowners_plan.assigned_reviewers,
                            &codeowners_plan.reasons,
                        );
                        recommendation_reviewers = recommendation.reviewers;
                        recommendation_reasons = recommendation.reasons;

                        if !codeowners_plan.uncovered_sections.is_empty() {
                            println!("CODEOWNERS coverage gaps:");

                            for gap in &codeowners_plan.uncovered_sections {
                                println!(
                                    "- {} => reachable {}/{} with eligible [{}]",
                                    gap.section_name,
                                    gap.reachable_approvals,
                                    gap.required_approvals,
                                    gap.eligible_users.join(", ")
                                );
                            }
                        }
                    }
                }

                if recommendation_reviewers.is_empty() {
                    println!("No reviewer recommendation was produced.");
                } else {
                    println!("Recommended reviewers to add:");

                    for reviewer in &recommendation_reviewers {
                        println!("- {}", reviewer);
                    }

                    println!("Routing reasons:");

                    for reason in &recommendation_reasons {
                        println!("- {}", reason);
                    }
                }

                if let Some(codeowners) = &app_config.codeowners.file {
                    println!("CODEOWNERS matches:");

                    for matched in collect_matched_rules_for_snapshot(codeowners, snapshot) {
                        let owners = match_usernames(codeowners, &matched.path);

                        if owners.is_empty() {
                            println!(
                                "- {} => {} => no individual owners",
                                matched.path, matched.pattern
                            );
                        } else if let Some(section_name) = matched.section_name {
                            println!(
                                "- {} => {} [{} approvals in section '{}'] => {}",
                                matched.path,
                                matched.pattern,
                                matched.required_approvals,
                                section_name,
                                owners.join(", ")
                            );
                        } else {
                            println!(
                                "- {} => {} => {}",
                                matched.path,
                                matched.pattern,
                                owners.join(", ")
                            );
                        }
                    }
                }

                if app_config.codeowners.is_enabled() {
                    println!("CODEOWNERS integration is enabled.");
                } else {
                    println!("CODEOWNERS integration is disabled.");
                }
            }
        }
    }

    Ok(())
}

#[instrument(skip_all, fields(strategy = ?strategy, action_count = plan.actions.len()))]
async fn execute_action_plan(
    strategy: ExecutionStrategy,
    ctx: &crate::context::model::CiContext,
    plan: &crate::actions::model::ActionPlan,
) -> Result<ExecutionReport> {
    match strategy {
        ExecutionStrategy::DryRun => {
            debug!("using dry-run executor");
            let executor = DryRunExecutor;
            executor.execute(plan).await
        }
        ExecutionStrategy::Real => {
            let mr_iid = ctx
                .merge_request_iid()
                .ok_or_else(|| anyhow::anyhow!("missing merge request IID for execution"))?;

            let config = GitLabConfig::from_env()?;
            let client = GitLabClient::new(config);
            debug!(
                project_id = %ctx.project_id(),
                merge_request_iid = %mr_iid,
                "using GitLab executor"
            );

            let executor = crate::actions::executor::gitlab::GitLabExecutor {
                client: &client,
                project_id: ctx.project_id(),
                merge_request_iid: mr_iid,
            };

            executor.execute(plan).await
        }
    }
}

#[instrument(
    skip_all,
    fields(
        project_id = %ctx.project_id(),
        merge_request_iid = ctx.merge_request_iid().unwrap_or("none")
    )
)]
async fn maybe_fetch_snapshot(
    ctx: &crate::context::model::CiContext,
) -> Result<Option<MergeRequestSnapshot>> {
    let Some(mr_iid) = ctx.merge_request_iid() else {
        debug!("merge request IID missing; skipping snapshot fetch");
        return Ok(None);
    };

    let config = GitLabConfig::from_env()?;
    let client = GitLabClient::new(config);
    info!("fetching merge request snapshot from GitLab");
    let snapshot = client
        .get_merge_request_snapshot(ctx.project_id(), mr_iid)
        .await?;
    info!(
        changed_files = snapshot.changed_file_count(),
        existing_reviewers = snapshot.details.reviewer_usernames.len(),
        draft = snapshot.details.is_draft,
        state = snapshot.details.state.as_str(),
        "merge request snapshot fetched"
    );

    Ok(Some(snapshot))
}

fn print_snapshot_details(snapshot: &MergeRequestSnapshot) {
    println!("Merge request details:");
    println!("- [Title] {}", snapshot.details.title);
    println!("- [State] {}", snapshot.details.state.as_str());
    println!("- [Draft] {}", snapshot.details.is_draft);
    println!("- [WebUrl] {}", snapshot.details.web_url);
    println!("- [Author] {}", snapshot.details.author_username);
    if snapshot.details.reviewer_usernames.is_empty() {
        println!("- [Reviewers] none");
    } else {
        println!(
            "- [Reviewers] {}",
            snapshot.details.reviewer_usernames.join(", ")
        );
    }
    println!("- [ChangedFiles] {}", snapshot.changed_file_count());

    if let Some(description) = &snapshot.details.description {
        if !description.trim().is_empty() {
            println!("- [Description] {}", description);
        }
    }

    let max_files_to_print = 20;
    let total_files = snapshot.changed_files.len();

    if total_files == 0 {
        println!("No changed files were reported.");
    } else {
        println!("Changed files:");

        for file in snapshot.changed_files.iter().take(max_files_to_print) {
            println!(
                "- {}{}{}{}",
                file.new_path,
                if file.is_new { " [new]" } else { "" },
                if file.is_renamed { " [renamed]" } else { "" },
                if file.is_deleted { " [deleted]" } else { "" },
            );
        }

        if total_files > max_files_to_print {
            println!(
                "- ... and {} more file(s) not shown.",
                total_files - max_files_to_print
            );
        }
    }
}

fn print_outcome(outcome: &RuleOutcome) {
    if outcome.is_empty() {
        println!("No findings were produced.");
        return;
    }

    for finding in &outcome.findings {
        println!("- [{:?}] {}", finding.severity, finding.message);
    }
}

fn print_action_plan(outcome: &RuleOutcome) {
    let rendered_actions: Vec<String> = outcome
        .action_plan
        .actions
        .iter()
        .map(describe_planned_action)
        .collect();

    if rendered_actions.is_empty() {
        println!("No actions are currently planned.");
        return;
    }

    println!("Planned actions:");

    for action in rendered_actions {
        println!("- {}", action);
    }
}

fn print_execution_report(report: &ExecutionReport) {
    if report.is_empty() {
        println!("No actions were executed.");
        return;
    }

    println!("Execution report:");

    for executed in &report.executed {
        match executed {
            ExecutedAction::CommentPosted { body } => {
                println!("- {}", describe_posted_comment(body));
            }
            ExecutedAction::ReviewersAssigned { reviewers } => {
                println!("- [ReviewersAssigned] {}", reviewers.join(", "));
            }
            ExecutedAction::PipelineFailurePlanned { reason } => {
                println!("- [PipelineFailurePlanned] {}", reason);
            }
            ExecutedAction::CommentSkippedAlreadyPresent { body } => {
                println!("- {}", describe_skipped_comment(body));
            }
            ExecutedAction::ReviewersSkippedAlreadyPresent { reviewers } => {
                println!(
                    "- [ReviewersSkippedAlreadyPresent] {}",
                    reviewers.join(", ")
                );
            }
        }
    }
}

async fn maybe_notify_slack(
    strategy: ExecutionStrategy,
    ctx: &crate::context::model::CiContext,
    outcome: &RuleOutcome,
    report: &ExecutionReport,
    snapshot: Option<&MergeRequestSnapshot>,
    slack_config: &SlackConfig,
    selector: &ToneSelector,
) -> Result<()> {
    let Some(assigned_reviewers) =
        reviewers_assigned_for_notification(strategy, outcome, report, slack_config)
    else {
        debug!(
            execution_strategy = ?strategy,
            slack_enabled = slack_config.enabled,
            webhook_configured = slack_config.webhook_url.is_some(),
            channel_configured = slack_config.channel.is_some(),
            "slack notification skipped"
        );
        return Ok(());
    };

    let Some(snapshot) = snapshot else {
        warn!("reviewers were assigned but snapshot is unavailable; skipping Slack notification");
        return Ok(());
    };

    let tone_line = selector.select(ToneCategory::ReviewRequest, ctx);
    let message = render_review_request_message(
        tone_line,
        &snapshot.details.title,
        &snapshot.details.web_url,
        &assigned_reviewers,
    );

    let notifier = SlackNotifier::new(slack_config.clone());
    info!(
        reviewer_count = assigned_reviewers.len(),
        merge_request_url = %snapshot.details.web_url,
        "sending Slack review request"
    );
    notifier.send_review_request(&message).await
}

fn reviewers_assigned_for_notification(
    strategy: ExecutionStrategy,
    outcome: &RuleOutcome,
    report: &ExecutionReport,
    slack_config: &SlackConfig,
) -> Option<Vec<String>> {
    if strategy != ExecutionStrategy::Real {
        return None;
    }

    if !slack_config.enabled
        || slack_config.webhook_url.is_none()
        || slack_config.channel.is_none()
        || outcome.action_plan.has_fail_pipeline()
        || outcome.has_blocking_findings()
    {
        return None;
    }

    report.executed.iter().find_map(|executed| match executed {
        ExecutedAction::ReviewersAssigned { reviewers } if !reviewers.is_empty() => {
            Some(reviewers.clone())
        }
        _ => None,
    })
}

fn print_observe_action_plan(outcome: &RuleOutcome) {
    let rendered_actions: Vec<String> = outcome
        .action_plan
        .actions
        .iter()
        .filter(|action| !matches!(action, Action::PostComment { .. }))
        .map(describe_planned_action)
        .collect();

    if rendered_actions.is_empty() {
        println!("No follow-up actions would be taken by `refine`.");
        return;
    }

    println!("If you run `refine`, it would:");

    for action in rendered_actions {
        println!("- {}", action);
    }
}

fn describe_planned_action(action: &Action) -> String {
    match action {
        Action::PostComment { body } if body.contains(MR_MILCHICK_MARKER) => {
            "[PostComment] Update Mr. Milchick summary comment".to_string()
        }
        Action::PostComment { .. } => "[PostComment] Post comment".to_string(),
        Action::AssignReviewers { reviewers } => {
            format!("[AssignReviewers] {}", reviewers.join(", "))
        }
        Action::FailPipeline { reason } => format!("[FailPipeline] {}", reason),
    }
}

fn describe_posted_comment(body: &str) -> String {
    if body.contains(MR_MILCHICK_MARKER) {
        "[CommentPosted] Mr. Milchick summary comment".to_string()
    } else {
        "[CommentPosted] Comment posted".to_string()
    }
}

fn describe_skipped_comment(body: &str) -> String {
    if body.contains(MR_MILCHICK_MARKER) {
        "[CommentSkippedAlreadyPresent] Mr. Milchick summary comment".to_string()
    } else {
        "[CommentSkippedAlreadyPresent] Comment already present".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::SlackConfig;
    use crate::rules::model::RuleFinding;

    #[test]
    fn summarizes_structured_comment_in_planned_actions() {
        let action = Action::PostComment {
            body: format!("{MR_MILCHICK_MARKER}\nsummary"),
        };

        assert_eq!(
            describe_planned_action(&action),
            "[PostComment] Update Mr. Milchick summary comment"
        );
    }

    #[test]
    fn summarizes_structured_comment_in_execution_output() {
        let body = format!("{MR_MILCHICK_MARKER}\nsummary");

        assert_eq!(
            describe_posted_comment(&body),
            "[CommentPosted] Mr. Milchick summary comment"
        );
        assert_eq!(
            describe_skipped_comment(&body),
            "[CommentSkippedAlreadyPresent] Mr. Milchick summary comment"
        );
    }

    #[test]
    fn notifies_only_when_real_reviewer_assignment_occurs() {
        let outcome = RuleOutcome::new();
        let report = ExecutionReport {
            executed: vec![ExecutedAction::ReviewersAssigned {
                reviewers: vec!["alice".to_string(), "bob".to_string()],
            }],
        };

        let reviewers = reviewers_assigned_for_notification(
            ExecutionStrategy::Real,
            &outcome,
            &report,
            &SlackConfig {
                enabled: true,
                webhook_url: Some("https://hooks.slack.com/triggers/example".to_string()),
                channel: Some("C123".to_string()),
            },
        );

        assert_eq!(
            reviewers,
            Some(vec!["alice".to_string(), "bob".to_string()])
        );
    }

    #[test]
    fn does_not_notify_on_dry_run() {
        let outcome = RuleOutcome::new();
        let report = ExecutionReport {
            executed: vec![ExecutedAction::ReviewersAssigned {
                reviewers: vec!["alice".to_string()],
            }],
        };

        let reviewers = reviewers_assigned_for_notification(
            ExecutionStrategy::DryRun,
            &outcome,
            &report,
            &SlackConfig {
                enabled: true,
                webhook_url: Some("https://hooks.slack.com/triggers/example".to_string()),
                channel: Some("C123".to_string()),
            },
        );

        assert_eq!(reviewers, None);
    }

    #[test]
    fn does_not_notify_when_pipeline_will_fail() {
        let mut outcome = RuleOutcome::new();
        outcome.push(RuleFinding::blocking("stop"));
        let report = ExecutionReport {
            executed: vec![ExecutedAction::ReviewersAssigned {
                reviewers: vec!["alice".to_string()],
            }],
        };

        let reviewers = reviewers_assigned_for_notification(
            ExecutionStrategy::Real,
            &outcome,
            &report,
            &SlackConfig {
                enabled: true,
                webhook_url: Some("https://hooks.slack.com/triggers/example".to_string()),
                channel: Some("C123".to_string()),
            },
        );

        assert_eq!(reviewers, None);
    }

    #[test]
    fn does_not_notify_when_reviewers_were_skipped() {
        let outcome = RuleOutcome::new();
        let report = ExecutionReport {
            executed: vec![ExecutedAction::ReviewersSkippedAlreadyPresent {
                reviewers: vec!["alice".to_string()],
            }],
        };

        let reviewers = reviewers_assigned_for_notification(
            ExecutionStrategy::Real,
            &outcome,
            &report,
            &SlackConfig {
                enabled: true,
                webhook_url: Some("https://hooks.slack.com/triggers/example".to_string()),
                channel: Some("C123".to_string()),
            },
        );

        assert_eq!(reviewers, None);
    }
}
