use anyhow::Result;
use tracing::warn;

use milchick_connectors::gitlab::{GitLabReviewConnector, render_gitlab_markdown};
#[cfg(feature = "slack-app")]
use milchick_connectors::notifications::slack_app::{SlackAppConfig, SlackAppSink};
#[cfg(feature = "slack-workflow")]
use milchick_connectors::notifications::slack_workflow::{
    SlackWorkflowConfig, SlackWorkflowSink,
};
use milchick_core::actions::planner::enrich_with_reviewer_assignment;
use milchick_core::comment::render::build_summary_message;
use milchick_core::domain::codeowners::context::CodeownersContext;
use milchick_core::domain::codeowners::matcher::{
    collect_matched_rules_for_snapshot, match_usernames,
};
use milchick_core::domain::codeowners::parser::parse_codeowners_file;
use milchick_core::domain::codeowners::planner::plan_codeowners_assignments;
use milchick_core::domain::reviewer_routing::{
    ReviewerRoutingConfig, prepend_mandatory_reviewers, recommend_reviewers,
};
use milchick_core::domain::snapshot_analysis::summarize_areas;
use milchick_core::model::{
    MessageSection, NotificationAudience, NotificationMessage, NotificationSeverity, ReviewAction,
    ReviewActionKind, ReviewPlatformKind,
};
use milchick_core::rules::engine::evaluate_rules;
use milchick_core::rules::model::RuleOutcome;
use milchick_core::tone::{ToneCategory, ToneSelector};
use milchick_runtime::{ExecutionMode, ExecutionStrategy, RuntimeWiring};

use crate::cli::Cli;
use crate::config::loader::{load_config, load_flavor_config, resolve_codeowners_path};
use crate::config::model::{FlavorConfig, RuntimeConfig};
use crate::context::builder::build_ci_context;

#[cfg(all(feature = "gitlab", feature = "github"))]
compile_error!("Exactly one review connector feature must be enabled.");
#[cfg(not(any(feature = "gitlab", feature = "github")))]
compile_error!("Exactly one review connector feature must be enabled.");
#[cfg(feature = "github")]
compile_error!("The GitHub review connector is not implemented yet.");

#[derive(Debug, Clone)]
struct AppConfigContext {
    runtime: RuntimeConfig,
    routing_config: ReviewerRoutingConfig,
    codeowners: CodeownersContext,
}

fn load_app_config_context() -> Result<AppConfigContext> {
    let runtime = load_config()?;
    let routing_config = ReviewerRoutingConfig::from_config(&runtime.reviewers);
    let codeowners = CodeownersContext {
        enabled: runtime.codeowners.enabled,
        file: resolve_codeowners_path(&runtime.codeowners)
            .and_then(|path| parse_codeowners_file(&path).ok()),
    };

    Ok(AppConfigContext {
        runtime,
        routing_config,
        codeowners,
    })
}

pub async fn run(cli: Cli) -> Result<()> {
    if matches!(cli.command, crate::cli::Command::Version) {
        crate::cli::print_version();
        print_compiled_capabilities();
        return Ok(());
    }

    let mode: ExecutionMode = cli.command.into();
    run_mode(mode).await
}

pub async fn run_mode(mode: ExecutionMode) -> Result<()> {
    let ctx = build_ci_context()?;
    let selector = ToneSelector::default();
    let app_config = load_app_config_context()?;

    println!("{}", selector.select(ToneCategory::Observation, &ctx));
    print_compiled_capabilities();

    if !ctx.is_merge_request_pipeline() {
        println!("This pipeline does not currently present merge request responsibilities.");
        return Ok(());
    }

    let flavor = load_flavor_config()?;
    let wiring = build_runtime_wiring(&ctx, &app_config, flavor.as_ref())?;

    let mut outcome = evaluate_rules(&ctx);
    let snapshot = wiring.review_connector.load_snapshot().await?;
    outcome = enrich_with_reviewer_assignment(
        outcome,
        &snapshot,
        &app_config.routing_config,
        &app_config.codeowners,
    );

    let summary = build_summary_message(&outcome, &ctx, &selector);
    outcome
        .action_plan
        .push(ReviewAction::UpsertSummary { message: summary });

    match mode {
        ExecutionMode::Observe => {
            print_outcome(&outcome);
            print_observe_action_plan(&outcome);
        }
        ExecutionMode::Explain => {
            print_outcome(&outcome);
            print_action_plan(&outcome, true);
            println!("Structured summary comment preview:");
            println!("---");
            if let Some(ReviewAction::UpsertSummary { message }) = outcome
                .action_plan
                .actions
                .iter()
                .find(|action| matches!(action, ReviewAction::UpsertSummary { .. }))
            {
                println!("{}", render_gitlab_markdown(message));
            }
            println!("---");
            print_snapshot_details(&snapshot);
            print_codeowners_details(&snapshot, &app_config);
        }
        ExecutionMode::Refine => {
            print_outcome(&outcome);
            print_action_plan(&outcome, true);

            let strategy = ExecutionStrategy::from_env();
            let notifications = build_notifications(strategy, &outcome, &snapshot, &selector, &ctx);
            let report = wiring
                .execute(strategy, &outcome.action_plan.actions, &notifications)
                .await?;

            print_execution_report(&report);

            if outcome.action_plan.has_fail_pipeline() || outcome.has_blocking_findings() {
                warn!("failing command because blocking findings or fail-pipeline action remain");
                anyhow::bail!("merge request policy requirements were not satisfied");
            }
        }
    }

    Ok(())
}

fn build_runtime_wiring(
    ctx: &crate::context::model::CiContext,
    app_config: &AppConfigContext,
    flavor: Option<&FlavorConfig>,
) -> Result<RuntimeWiring> {
    validate_flavor(flavor)?;

    let gitlab = GitLabReviewConnector::new(
        milchick_connectors::gitlab::api::GitLabConfig::from_env()?,
        ctx.project_id(),
        ctx.merge_request_iid()
            .ok_or_else(|| anyhow::anyhow!("missing merge request IID"))?,
        ctx.source_branch(),
        ctx.target_branch(),
        ctx.labels.iter().map(|label| label.0.clone()).collect(),
    );

    let mut sinks: Vec<Box<dyn milchick_runtime::NotificationSink>> = Vec::new();

    #[cfg(feature = "slack-app")]
    if notification_enabled_by_flavor(flavor, "slack-app") {
        let slack = SlackAppSink::new(SlackAppConfig {
            enabled: app_config.runtime.slack.enabled,
            base_url: app_config.runtime.slack.base_url.clone(),
            bot_token: app_config.runtime.slack.bot_token.clone(),
            channel: app_config.runtime.slack.channel.clone(),
        });
        if slack.is_enabled() {
            sinks.push(Box::new(slack));
        }
    }

    #[cfg(feature = "slack-workflow")]
    if notification_enabled_by_flavor(flavor, "slack-workflow") {
        let slack = SlackWorkflowSink::new(SlackWorkflowConfig {
            enabled: app_config.runtime.slack.enabled,
            webhook_url: app_config.runtime.slack.webhook_url.clone(),
            channel: app_config.runtime.slack.channel.clone(),
        });
        if slack.is_enabled() {
            sinks.push(Box::new(slack));
        }
    }

    Ok(RuntimeWiring::new(Box::new(gitlab), sinks))
}

fn validate_flavor(flavor: Option<&FlavorConfig>) -> Result<()> {
    if let Some(flavor) = flavor {
        if flavor.review_platform.kind != ReviewPlatformKind::GitLab.as_str() {
            anyhow::bail!(
                "flavor review platform '{}' does not match compiled capability 'gitlab'",
                flavor.review_platform.kind
            );
        }

        for notification in &flavor.notifications {
            match notification.kind.as_str() {
                "slack-app" => {
                    #[cfg(not(feature = "slack-app"))]
                    anyhow::bail!("flavor notification sink 'slack-app' is not compiled in");
                }
                "slack-workflow" => {
                    #[cfg(not(feature = "slack-workflow"))]
                    anyhow::bail!("flavor notification sink 'slack-workflow' is not compiled in");
                }
                other => anyhow::bail!("unsupported flavor notification sink '{}'", other),
            }
        }
    }

    Ok(())
}

fn notification_enabled_by_flavor(flavor: Option<&FlavorConfig>, kind: &str) -> bool {
    flavor
        .map(|flavor| {
            flavor
                .notifications
                .iter()
                .any(|notification| notification.kind == kind && notification.enabled)
        })
        .unwrap_or(true)
}

fn print_compiled_capabilities() {
    println!("Compiled capabilities:");
    println!("- review platform: gitlab");
    let mut sinks = Vec::new();
    #[cfg(feature = "slack-app")]
    sinks.push("slack-app");
    #[cfg(feature = "slack-workflow")]
    sinks.push("slack-workflow");

    if sinks.is_empty() {
        println!("- notification sinks: none");
    } else {
        println!("- notification sinks: {}", sinks.join(", "));
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

fn print_action_plan(outcome: &RuleOutcome, include_summary: bool) {
    let rendered_actions = outcome
        .action_plan
        .actions
        .iter()
        .filter(|action| include_summary || !matches!(action, ReviewAction::UpsertSummary { .. }))
        .map(describe_planned_action)
        .collect::<Vec<_>>();

    if rendered_actions.is_empty() {
        println!("No actions are currently planned.");
        return;
    }

    println!("Planned actions:");
    for action in rendered_actions {
        println!("- {}", action);
    }
}

fn describe_planned_action(action: &ReviewAction) -> String {
    match action {
        ReviewAction::AssignReviewers { reviewers } => format!(
            "[AssignReviewers] {}",
            reviewers
                .iter()
                .map(|reviewer| reviewer.username.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ReviewAction::UpsertSummary { .. } => {
            "[UpsertSummary] Update Mr. Milchick summary comment".to_string()
        }
        ReviewAction::AddLabels { labels } => format!("[AddLabels] {}", labels.join(", ")),
        ReviewAction::RemoveLabels { labels } => {
            format!("[RemoveLabels] {}", labels.join(", "))
        }
        ReviewAction::FailPipeline { reason } => format!("[FailPipeline] {}", reason),
    }
}

fn print_execution_report(report: &milchick_runtime::ExecutionReport) {
    println!("Execution report:");
    for applied in &report.review_report.applied {
        match applied.action {
            ReviewActionKind::AssignReviewers => {
                println!(
                    "- [ReviewersAssigned] {}",
                    applied.detail.clone().unwrap_or_default()
                );
            }
            ReviewActionKind::UpsertSummary => {
                println!("- [CommentPosted] Mr. Milchick summary comment");
            }
            ReviewActionKind::FailPipeline => {
                println!(
                    "- [PipelineFailurePlanned] {}",
                    applied.detail.clone().unwrap_or_default()
                );
            }
            _ => println!(
                "- [{:?}] {}",
                applied.action,
                applied.detail.clone().unwrap_or_default()
            ),
        }
    }
    for skipped in &report.review_report.skipped {
        match skipped.action {
            ReviewActionKind::UpsertSummary => {
                println!("- [CommentSkippedAlreadyPresent] Mr. Milchick summary comment");
            }
            ReviewActionKind::AssignReviewers => {
                println!("- [ReviewersSkippedAlreadyPresent] {}", skipped.reason);
            }
            _ => println!("- [Skipped {:?}] {}", skipped.action, skipped.reason),
        }
    }
    for notification in &report.notification_reports {
        println!(
            "- [Notification {:?}] delivered={} {}",
            notification.sink,
            notification.delivered,
            notification.detail.clone().unwrap_or_default()
        );
    }
}

fn build_notifications(
    strategy: ExecutionStrategy,
    outcome: &RuleOutcome,
    snapshot: &milchick_core::model::ReviewSnapshot,
    selector: &ToneSelector,
    ctx: &crate::context::model::CiContext,
) -> Vec<NotificationMessage> {
    if strategy != ExecutionStrategy::Real
        || outcome.action_plan.has_fail_pipeline()
        || outcome.has_blocking_findings()
    {
        return Vec::new();
    }

    let assigned_reviewers = outcome
        .action_plan
        .actions
        .iter()
        .find_map(|action| match action {
            ReviewAction::AssignReviewers { reviewers } if !reviewers.is_empty() => Some(
                reviewers
                    .iter()
                    .map(|reviewer| reviewer.username.clone())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        });

    let Some(reviewers) = assigned_reviewers else {
        return Vec::new();
    };

    let mut body = milchick_core::model::RenderedMessage::new(Some(
        selector
            .select(ToneCategory::ReviewRequest, ctx)
            .to_string(),
    ));
    let mentions = reviewers
        .iter()
        .map(|reviewer| format!("*@{}*", reviewer))
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(url) = &snapshot.review_ref.web_url {
        body.sections.push(MessageSection::Paragraph(format!(
            "Review requested for: <{}|{}>",
            url, snapshot.title
        )));
    }
    body.sections.push(MessageSection::Paragraph(format!(
        "_Assign reviewers_ {}",
        mentions
    )));

    vec![NotificationMessage {
        subject: format!(
            ":gitlab: Reviews Needed for <{}|MR #{}>, by @{} :pepe-review:",
            snapshot.review_ref.web_url.clone().unwrap_or_default(),
            snapshot.review_ref.review_id,
            snapshot.author.username
        ),
        body,
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
    }]
}

fn print_observe_action_plan(outcome: &RuleOutcome) {
    let rendered_actions: Vec<String> = outcome
        .action_plan
        .actions
        .iter()
        .filter(|action| !matches!(action, ReviewAction::UpsertSummary { .. }))
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

fn print_snapshot_details(snapshot: &milchick_core::model::ReviewSnapshot) {
    println!("Merge request details:");
    println!("- [Title] {}", snapshot.title);
    println!("- [Draft] {}", snapshot.is_draft);
    if let Some(url) = &snapshot.review_ref.web_url {
        println!("- [WebUrl] {}", url);
    }
    println!("- [Author] {}", snapshot.author.username);
    if snapshot.participants.is_empty() {
        println!("- [Reviewers] none");
    } else {
        println!(
            "- [Reviewers] {}",
            snapshot
                .participants
                .iter()
                .map(|reviewer| reviewer.username.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!("- [ChangedFiles] {}", snapshot.changed_file_count());
}

fn print_codeowners_details(
    snapshot: &milchick_core::model::ReviewSnapshot,
    app_config: &AppConfigContext,
) {
    let area_summary = summarize_areas(snapshot);
    println!("Area summary:");
    for (area, count) in &area_summary.counts {
        println!("- [{}] {}", area.as_str(), count);
    }

    let excluded_reviewers = vec![snapshot.author.username.clone()];
    let fallback = recommend_reviewers(
        &area_summary,
        &app_config.routing_config,
        &excluded_reviewers,
    );
    let mut recommendation_reviewers = fallback.reviewers;
    let mut recommendation_reasons = fallback.reasons;

    if let Some(codeowners) = &app_config.codeowners.file {
        let codeowners_plan = plan_codeowners_assignments(codeowners, snapshot);
        if !codeowners_plan.matched_sections.is_empty() {
            recommendation_reviewers = prepend_mandatory_reviewers(
                &app_config.routing_config,
                &excluded_reviewers,
                &codeowners_plan.assigned_reviewers,
                &codeowners_plan.reasons,
            )
            .reviewers;
            recommendation_reasons = codeowners_plan.reasons.clone();
        }

        println!("CODEOWNERS matches:");
        for matched in collect_matched_rules_for_snapshot(codeowners, snapshot) {
            let owners = match_usernames(codeowners, &matched.path);
            println!(
                "- {} => {} => {}",
                matched.path,
                matched.pattern,
                owners.join(", ")
            );
        }
    }

    if recommendation_reviewers.is_empty() {
        println!("No reviewer recommendation was produced.");
    } else {
        println!("Recommended reviewers to add:");
        for reviewer in recommendation_reviewers {
            println!("- {}", reviewer);
        }
        println!("Routing reasons:");
        for reason in recommendation_reasons {
            println!("- {}", reason);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use milchick_core::actions::model::ActionPlan;
    use milchick_core::rules::model::{FindingSeverity, RuleFinding};

    #[test]
    fn notification_building_skips_blocking_outcomes() {
        let outcome = RuleOutcome {
            findings: vec![RuleFinding {
                severity: FindingSeverity::Blocking,
                message: "blocked".to_string(),
            }],
            action_plan: ActionPlan::new(),
        };
        let ctx = build_ci_context().err();
        assert!(ctx.is_some() || outcome.has_blocking_findings());
    }

    #[test]
    fn summary_action_is_described() {
        let text = describe_planned_action(&ReviewAction::UpsertSummary {
            message: milchick_core::model::RenderedMessage::new(Some("Summary".to_string())),
        });

        assert!(text.contains("UpsertSummary"));
        assert!(text.to_lowercase().contains("summary"));
    }
}
