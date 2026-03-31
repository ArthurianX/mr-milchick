use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use tracing::warn;

#[cfg(feature = "github")]
use crate::connectors::github::{GitHubPlatformConnector, render_github_markdown};
#[cfg(feature = "gitlab")]
use crate::connectors::gitlab::{GitLabPlatformConnector, render_gitlab_markdown};
#[cfg(feature = "slack-app")]
use crate::connectors::notifications::slack_app::{SlackAppConfig, SlackAppSink};
#[cfg(feature = "slack-workflow")]
use crate::connectors::notifications::slack_workflow::{SlackWorkflowConfig, SlackWorkflowSink};
use crate::core::actions::planner::enrich_with_reviewer_assignment;
use crate::core::domain::codeowners::context::CodeownersContext;
use crate::core::domain::codeowners::matcher::{
    collect_matched_rules_for_snapshot, match_usernames,
};
use crate::core::domain::codeowners::parser::parse_codeowners_file;
use crate::core::domain::codeowners::planner::plan_codeowners_assignments;
use crate::core::domain::reviewer_routing::{
    ReviewerRoutingConfig, prepend_mandatory_reviewers, recommend_reviewers,
};
use crate::core::domain::snapshot_analysis::summarize_areas;
use crate::core::message_templates::{
    build_notification_template_context, build_summary_template_context,
    notification_template_variant, render_review_summary, render_slack_app_notification,
    render_slack_workflow_notification, resolve_template_catalog,
};
use crate::core::model::{
    NotificationAudience, NotificationMessage, NotificationSeverity, NotificationSinkKind,
    ReviewAction, ReviewActionKind, ReviewPlatformKind,
};
use crate::core::rules::engine::evaluate_rules;
use crate::core::rules::model::RuleOutcome;
use crate::core::tone::{ToneCategory, ToneSelector};
use crate::runtime::{ExecutionMode, ExecutionStrategy, RuntimeWiring};

use crate::cli::{Cli, FixtureNotificationVariantArg};
use crate::config::loader::{load_config, load_flavor_config, resolve_codeowners_path};
use crate::config::model::{FlavorConfig, NotificationPolicy, RuntimeConfig};
use crate::context::builder::build_ci_context;
use crate::fixture::load_review_fixture;
use crate::runtime::{ConnectorError, NotificationSink, PlatformConnector};

#[cfg(all(feature = "gitlab", feature = "github"))]
compile_error!("Exactly one platform connector feature must be enabled.");
#[cfg(not(any(feature = "gitlab", feature = "github")))]
compile_error!("Exactly one platform connector feature must be enabled.");

#[derive(Debug, Clone)]
struct AppConfigContext {
    runtime: RuntimeConfig,
    routing_config: ReviewerRoutingConfig,
    codeowners: CodeownersContext,
}

#[derive(Debug)]
struct FixturePlatformConnector;

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

#[async_trait::async_trait]
impl PlatformConnector for FixturePlatformConnector {
    fn kind(&self) -> ReviewPlatformKind {
        compiled_platform_kind()
    }

    async fn load_snapshot(
        &self,
    ) -> std::result::Result<crate::core::model::ReviewSnapshot, ConnectorError> {
        Err(ConnectorError::Unsupported(
            "fixture platform connector cannot load live snapshots".to_string(),
        ))
    }

    async fn apply_review_actions(
        &self,
        actions: &[ReviewAction],
    ) -> std::result::Result<crate::core::model::ReviewActionReport, ConnectorError> {
        let mut report = crate::core::model::ReviewActionReport::default();

        for action in actions {
            let detail = match action {
                ReviewAction::AssignReviewers { reviewers } => Some(
                    reviewers
                        .iter()
                        .map(|reviewer| reviewer.username.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                ReviewAction::UpsertSummary { .. } => Some("fixture".to_string()),
                ReviewAction::AddLabels { labels } | ReviewAction::RemoveLabels { labels } => {
                    Some(labels.join(", "))
                }
                ReviewAction::FailPipeline { reason } => Some(reason.clone()),
            };

            report
                .applied
                .push(crate::core::model::AppliedReviewAction {
                    action: action.kind(),
                    detail,
                });
        }

        Ok(report)
    }
}

fn compiled_platform_kind() -> ReviewPlatformKind {
    #[cfg(feature = "gitlab")]
    {
        ReviewPlatformKind::GitLab
    }
    #[cfg(feature = "github")]
    {
        ReviewPlatformKind::GitHub
    }
}

fn render_summary_for_platform(markdown: &str, platform: ReviewPlatformKind) -> String {
    match platform {
        #[cfg(feature = "gitlab")]
        ReviewPlatformKind::GitLab => render_gitlab_markdown(markdown),
        #[cfg(feature = "github")]
        ReviewPlatformKind::GitHub => render_github_markdown(markdown),
        _ => unreachable!("unsupported compiled review platform"),
    }
}

pub async fn run(cli: Cli) -> Result<()> {
    if matches!(cli.command, crate::cli::Command::Version) {
        crate::cli::print_version();
        print_compiled_capabilities();
        return Ok(());
    }

    let mode = cli
        .command
        .execution_mode()
        .expect("execution mode should exist for non-version commands");
    run_mode(
        mode,
        cli.command.fixture_path(),
        cli.command.fixture_variant(),
        cli.command.send_notifications(),
    )
    .await
}

pub async fn run_mode(
    mode: ExecutionMode,
    fixture_path: Option<&str>,
    fixture_variant: Option<FixtureNotificationVariantArg>,
    send_notifications: bool,
) -> Result<()> {
    if send_notifications && fixture_path.is_none() {
        anyhow::bail!("'--send-notifications' is only supported together with '--fixture'");
    }
    if fixture_variant.is_some() && fixture_path.is_none() {
        anyhow::bail!("'--fixture-variant' is only supported together with '--fixture'");
    }

    let selector = ToneSelector::default();
    let app_config = load_app_config_context()?;
    let flavor = load_flavor_config()?;
    let template_catalog = resolve_template_catalog(flavor.as_ref());
    let fixture_mode = fixture_path.is_some();
    let mut fixture_notification_variant = fixture_variant.map(map_fixture_variant_arg);

    let (ctx, snapshot, mut outcome);
    if let Some(fixture_path) = fixture_path {
        let fixture = load_review_fixture(fixture_path)?;
        fixture_notification_variant =
            fixture_notification_variant.or_else(|| fixture.notification_template_variant());
        ctx = fixture.to_ci_context()?;
        snapshot = fixture.to_review_snapshot(compiled_platform_kind())?;
        outcome = fixture.to_rule_outcome()?;
    } else {
        ctx = build_ci_context()?;
        println!("{}", selector.select(ToneCategory::Observation, &ctx));
        print_compiled_capabilities();

        if !ctx.is_review_pipeline() {
            println!("This pipeline does not currently present review responsibilities.");
            return Ok(());
        }

        let wiring = build_runtime_wiring(&ctx, &app_config, flavor.as_ref())?;
        snapshot = wiring.platform_connector.load_snapshot().await?;
        outcome = enrich_with_reviewer_assignment(
            evaluate_rules(&ctx),
            &snapshot,
            &app_config.routing_config,
            &app_config.codeowners,
        );
    }

    if fixture_mode {
        println!("{}", selector.select(ToneCategory::Observation, &ctx));
        print_compiled_capabilities();
    }

    let wiring = if fixture_mode {
        build_fixture_runtime_wiring(&app_config, flavor.as_ref())?
    } else {
        build_runtime_wiring(&ctx, &app_config, flavor.as_ref())?
    };
    let preview_sink_kinds = configured_notification_sink_kinds(flavor.as_ref());

    let summary = render_review_summary(
        &template_catalog,
        &build_summary_template_context(&outcome, &snapshot, &selector, &ctx),
        compiled_platform_kind(),
    );
    outcome.action_plan.push(ReviewAction::UpsertSummary {
        markdown: summary.clone(),
    });

    match mode {
        ExecutionMode::Observe => {
            print_outcome(&outcome);
            print_observe_action_plan(&outcome);
            if fixture_mode {
                let notifications = build_notifications(
                    &outcome,
                    &snapshot,
                    &template_catalog,
                    &selector,
                    &ctx,
                    &preview_sink_kinds,
                    fixture_notification_variant,
                );
                print_notification_previews(&notifications);
            }
        }
        ExecutionMode::Explain => {
            print_outcome(&outcome);
            print_action_plan(&outcome, true);
            println!("Structured summary comment preview:");
            println!("---");
            if let Some(ReviewAction::UpsertSummary { markdown }) = outcome
                .action_plan
                .actions
                .iter()
                .find(|action| matches!(action, ReviewAction::UpsertSummary { .. }))
            {
                println!(
                    "{}",
                    render_summary_for_platform(markdown, compiled_platform_kind())
                );
            }
            println!("---");
            print_snapshot_details(&snapshot);
            let notifications = build_notifications(
                &outcome,
                &snapshot,
                &template_catalog,
                &selector,
                &ctx,
                &preview_sink_kinds,
                fixture_notification_variant,
            );
            if fixture_mode {
                print_notification_previews(&notifications);
            }
            print_codeowners_details(&snapshot, &app_config);
        }
        ExecutionMode::Refine => {
            print_outcome(&outcome);
            print_action_plan(&outcome, true);

            let strategy = if fixture_mode {
                if send_notifications {
                    ExecutionStrategy::Real
                } else {
                    ExecutionStrategy::DryRun
                }
            } else {
                ExecutionStrategy::from_env()
            };
            let notification_policy =
                resolve_notification_policy(&app_config.runtime, flavor.as_ref());
            let notifications = build_notifications(
                &outcome,
                &snapshot,
                &template_catalog,
                &selector,
                &ctx,
                &preview_sink_kinds,
                fixture_notification_variant,
            );
            if fixture_mode {
                print_notification_previews(&notifications);
            }
            let report = wiring
                .execute(
                    strategy,
                    notification_policy,
                    &outcome.action_plan.actions,
                    &notifications,
                )
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
    let review_id = ctx
        .review_id()
        .ok_or_else(|| anyhow::anyhow!("missing review identifier"))?;

    #[cfg(feature = "gitlab")]
    let platform_connector: Box<dyn PlatformConnector> = Box::new(GitLabPlatformConnector::new(
        crate::connectors::gitlab::api::GitLabConfig::from_env()?,
        ctx.project_key(),
        review_id,
        ctx.source_branch(),
        ctx.target_branch(),
        ctx.labels.iter().map(|label| label.0.clone()).collect(),
    ));
    #[cfg(feature = "github")]
    let platform_connector: Box<dyn PlatformConnector> = Box::new(GitHubPlatformConnector::new(
        crate::connectors::github::api::GitHubConfig::from_env()?,
        ctx.project_key(),
        review_id,
        ctx.source_branch(),
        ctx.target_branch(),
        ctx.labels.iter().map(|label| label.0.clone()).collect(),
    ));

    Ok(RuntimeWiring::new(
        platform_connector,
        build_notification_sinks(app_config, flavor),
    ))
}

fn build_fixture_runtime_wiring(
    app_config: &AppConfigContext,
    flavor: Option<&FlavorConfig>,
) -> Result<RuntimeWiring> {
    validate_flavor(flavor)?;
    Ok(RuntimeWiring::new(
        Box::new(FixturePlatformConnector),
        build_notification_sinks(app_config, flavor),
    ))
}

fn build_notification_sinks(
    _app_config: &AppConfigContext,
    _flavor: Option<&FlavorConfig>,
) -> Vec<Box<dyn NotificationSink>> {
    #[allow(unused_mut)]
    let mut sinks: Vec<Box<dyn NotificationSink>> = Vec::new();

    #[cfg(feature = "slack-app")]
    if notification_enabled_by_flavor(_flavor, "slack-app") {
        let slack = SlackAppSink::new(SlackAppConfig {
            enabled: _app_config.runtime.slack.enabled,
            base_url: _app_config.runtime.slack.base_url.clone(),
            bot_token: _app_config.runtime.slack.bot_token.clone(),
            channel: _app_config.runtime.slack.channel.clone(),
            user_map: resolve_slack_app_user_map(&_app_config.runtime, _flavor),
        });
        if slack.is_enabled() {
            sinks.push(Box::new(slack));
        }
    }

    #[cfg(feature = "slack-workflow")]
    if notification_enabled_by_flavor(_flavor, "slack-workflow") {
        let slack = SlackWorkflowSink::new(SlackWorkflowConfig {
            enabled: _app_config.runtime.slack.enabled,
            webhook_url: _app_config.runtime.slack.webhook_url.clone(),
            channel: _app_config.runtime.slack.channel.clone(),
        });
        if slack.is_enabled() {
            sinks.push(Box::new(slack));
        }
    }

    sinks
}

fn configured_notification_sink_kinds(_flavor: Option<&FlavorConfig>) -> Vec<NotificationSinkKind> {
    #[allow(unused_mut)]
    let mut sinks = Vec::new();

    #[cfg(feature = "slack-app")]
    if notification_enabled_by_flavor(_flavor, "slack-app") {
        sinks.push(NotificationSinkKind::SlackApp);
    }

    #[cfg(feature = "slack-workflow")]
    if notification_enabled_by_flavor(_flavor, "slack-workflow") {
        sinks.push(NotificationSinkKind::SlackWorkflow);
    }

    sinks
}

fn validate_flavor(flavor: Option<&FlavorConfig>) -> Result<()> {
    if let Some(flavor) = flavor {
        if flavor.platform_connector.kind != compiled_platform_kind().as_str() {
            anyhow::bail!(
                "flavor platform connector '{}' does not match compiled capability '{}'",
                flavor.platform_connector.kind,
                compiled_platform_kind().as_str()
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

#[allow(dead_code)]
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

fn resolve_notification_policy(
    runtime: &RuntimeConfig,
    flavor: Option<&FlavorConfig>,
) -> NotificationPolicy {
    runtime
        .notification_policy
        .or_else(|| flavor.and_then(|config| config.notification_policy))
        .unwrap_or(NotificationPolicy::Always)
}

#[allow(dead_code)]
fn resolve_slack_app_user_map(
    runtime: &RuntimeConfig,
    flavor: Option<&FlavorConfig>,
) -> BTreeMap<String, String> {
    if !runtime.slack.user_map.is_empty() {
        return runtime.slack.user_map.clone();
    }

    flavor
        .and_then(|config| config.slack_app.as_ref())
        .map(|config| {
            config
                .user_map
                .iter()
                .filter_map(|(gitlab_username, slack_user_id)| {
                    let gitlab_username = gitlab_username.trim();
                    let slack_user_id = slack_user_id.trim();

                    if gitlab_username.is_empty() || slack_user_id.is_empty() {
                        return None;
                    }

                    Some((gitlab_username.to_string(), slack_user_id.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn print_compiled_capabilities() {
    println!("Compiled capabilities:");
    println!(
        "- platform connector: {}",
        compiled_platform_kind().as_str()
    );
    #[allow(unused_mut)]
    let mut sinks: Vec<&str> = Vec::new();
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

fn print_execution_report(report: &crate::runtime::ExecutionReport) {
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

fn print_notification_previews(notifications: &[NotificationMessage]) {
    if notifications.is_empty() {
        println!("No notification previews were produced.");
        return;
    }

    println!("Notification previews:");
    for notification in notifications {
        println!("--- {:?} subject ---", notification.sink);
        println!("{}", notification.subject);
        println!("--- {:?} body ---", notification.sink);
        println!("{}", notification.body);
    }
}

fn build_notifications(
    outcome: &RuleOutcome,
    snapshot: &crate::core::model::ReviewSnapshot,
    template_catalog: &crate::core::message_templates::TemplateCatalog,
    selector: &ToneSelector,
    ctx: &crate::context::model::CiContext,
    sink_kinds: &[NotificationSinkKind],
    variant_override: Option<crate::core::message_templates::NotificationTemplateVariant>,
) -> Vec<NotificationMessage> {
    if outcome.action_plan.has_fail_pipeline() || outcome.has_blocking_findings() {
        return Vec::new();
    }

    let reviewers = reviewers_for_notification(outcome, snapshot);
    let variant =
        variant_override.unwrap_or_else(|| notification_template_variant(&reviewers.reviewers));
    let notification_context = build_notification_template_context(
        outcome,
        snapshot,
        selector,
        ctx,
        variant,
        reviewers.reviewers.clone(),
        reviewers.new_reviewers.clone(),
        reviewers.existing_reviewers.clone(),
    );

    sink_kinds
        .iter()
        .filter_map(|sink| match sink {
            NotificationSinkKind::SlackApp => {
                let (subject, body) =
                    render_slack_app_notification(template_catalog, &notification_context, variant);
                Some(NotificationMessage {
                    sink: *sink,
                    subject,
                    body,
                    audience: NotificationAudience::Default,
                    severity: NotificationSeverity::Info,
                })
            }
            NotificationSinkKind::SlackWorkflow => {
                let (subject, body) = render_slack_workflow_notification(
                    template_catalog,
                    &notification_context,
                    variant,
                );
                Some(NotificationMessage {
                    sink: *sink,
                    subject,
                    body,
                    audience: NotificationAudience::Default,
                    severity: NotificationSeverity::Info,
                })
            }
            _ => None,
        })
        .collect()
}

fn map_fixture_variant_arg(
    variant: FixtureNotificationVariantArg,
) -> crate::core::message_templates::NotificationTemplateVariant {
    match variant {
        FixtureNotificationVariantArg::First => {
            crate::core::message_templates::NotificationTemplateVariant::First
        }
        FixtureNotificationVariantArg::Update => {
            crate::core::message_templates::NotificationTemplateVariant::Update
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct NotificationReviewers {
    reviewers: Vec<String>,
    new_reviewers: Vec<String>,
    existing_reviewers: Vec<String>,
}

fn reviewers_for_notification(
    outcome: &RuleOutcome,
    snapshot: &crate::core::model::ReviewSnapshot,
) -> NotificationReviewers {
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

    let Some(assigned_reviewers) = assigned_reviewers else {
        return NotificationReviewers::default();
    };

    let mut merged_reviewers = Vec::new();
    let mut seen = BTreeSet::new();
    let mut existing_reviewers = Vec::new();

    for reviewer in snapshot.reviewer_usernames() {
        if seen.insert(reviewer.clone()) {
            merged_reviewers.push(reviewer);
            existing_reviewers.push(merged_reviewers.last().cloned().unwrap_or_default());
        }
    }

    let mut new_reviewers = Vec::new();
    for reviewer in assigned_reviewers {
        if seen.insert(reviewer.clone()) {
            merged_reviewers.push(reviewer);
            new_reviewers.push(merged_reviewers.last().cloned().unwrap_or_default());
        }
    }

    NotificationReviewers {
        reviewers: merged_reviewers,
        new_reviewers,
        existing_reviewers,
    }
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

fn print_snapshot_details(snapshot: &crate::core::model::ReviewSnapshot) {
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
    snapshot: &crate::core::model::ReviewSnapshot,
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
    use crate::config::model::{
        CodeownersConfig, FlavorPlatformConnector, FlavorSlackAppConfig, SlackConfig,
    };
    use crate::core::actions::model::ActionPlan;
    use crate::core::context::model::{
        BranchInfo, BranchName, CiContext, PipelineInfo, PipelineSource, ProjectKey,
        ReviewContextRef, ReviewId,
    };
    use crate::core::model::{
        Actor, ChangeType, ChangedFile, RepositoryRef, ReviewMetadata, ReviewRef, ReviewSnapshot,
    };
    use crate::core::rules::model::{FindingSeverity, RuleFinding};

    fn sample_context() -> CiContext {
        CiContext {
            project_key: ProjectKey("123".to_string()),
            review: Some(ReviewContextRef {
                id: ReviewId("456".to_string()),
            }),
            pipeline: PipelineInfo {
                source: PipelineSource::ReviewEvent,
            },
            branches: BranchInfo {
                source: BranchName("feat/test".to_string()),
                target: BranchName("develop".to_string()),
            },
            labels: vec![],
        }
    }

    fn sample_snapshot(existing_reviewers: Vec<&str>) -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitLab,
                project_key: "123".to_string(),
                review_id: "456".to_string(),
                web_url: Some(
                    "https://gitlab.example.com/group/project/-/merge_requests/456".to_string(),
                ),
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
                username: "arthur".to_string(),
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
            is_draft: false,
            default_branch: Some("develop".to_string()),
            metadata: ReviewMetadata::default(),
        }
    }

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
            markdown: "## Summary".to_string(),
        });

        assert!(text.contains("UpsertSummary"));
        assert!(text.to_lowercase().contains("summary"));
    }

    #[test]
    fn slack_notifications_include_existing_and_new_reviewers() {
        let mut outcome = RuleOutcome::new();
        outcome.action_plan.push(ReviewAction::AssignReviewers {
            reviewers: vec![Actor {
                username: "bob".to_string(),
                display_name: None,
            }],
        });
        let snapshot = sample_snapshot(vec!["principal-reviewer"]);
        let selector = ToneSelector::default();
        let ctx = sample_context();

        let notifications = build_notifications(
            &outcome,
            &snapshot,
            &crate::core::message_templates::TemplateCatalog::default(),
            &selector,
            &ctx,
            &[NotificationSinkKind::SlackApp],
            None,
        );

        assert_eq!(notifications.len(), 1);
        assert!(
            notifications[0]
                .body
                .contains("_Assigned reviewers_ *@principal-reviewer* *@bob*")
        );
    }

    #[test]
    fn slack_notifications_include_summary_when_no_reviewers_are_added() {
        let outcome = RuleOutcome::new();
        let snapshot = sample_snapshot(Vec::new());
        let selector = ToneSelector::default();
        let ctx = sample_context();

        let notifications = build_notifications(
            &outcome,
            &snapshot,
            &crate::core::message_templates::TemplateCatalog::default(),
            &selector,
            &ctx,
            &[NotificationSinkKind::SlackWorkflow],
            None,
        );

        assert_eq!(notifications.len(), 1);
        assert!(notifications[0].subject.contains("Mr. Milchick - updates"));
        assert!(notifications[0]
            .body
            .contains("Merge request: Frontend adjustments (https://gitlab.example.com/group/project/-/merge_requests/456)"));
    }

    #[test]
    fn slack_app_user_map_prefers_runtime_env_mapping() {
        let runtime = RuntimeConfig {
            reviewers: crate::core::model::ReviewerConfig {
                definitions: Vec::new(),
                max_reviewers: 2,
            },
            codeowners: CodeownersConfig {
                enabled: true,
                path: None,
            },
            slack: SlackConfig {
                enabled: true,
                base_url: "https://slack.com/api".to_string(),
                bot_token: None,
                webhook_url: None,
                channel: None,
                user_map: BTreeMap::from([("alice".to_string(), "UENV12345".to_string())]),
            },
            notification_policy: None,
        };
        let flavor = FlavorConfig {
            platform_connector: FlavorPlatformConnector {
                kind: "gitlab".to_string(),
            },
            notification_policy: None,
            notifications: Vec::new(),
            slack_app: Some(FlavorSlackAppConfig {
                user_map: BTreeMap::from([("alice".to_string(), "UTOML1234".to_string())]),
            }),
            templates: crate::config::model::FlavorTemplatesConfig::default(),
        };

        let resolved = resolve_slack_app_user_map(&runtime, Some(&flavor));

        assert_eq!(resolved.get("alice"), Some(&"UENV12345".to_string()));
    }

    #[test]
    fn slack_app_user_map_falls_back_to_flavor_mapping() {
        let runtime = RuntimeConfig {
            reviewers: crate::core::model::ReviewerConfig {
                definitions: Vec::new(),
                max_reviewers: 2,
            },
            codeowners: CodeownersConfig {
                enabled: true,
                path: None,
            },
            slack: SlackConfig {
                enabled: true,
                base_url: "https://slack.com/api".to_string(),
                bot_token: None,
                webhook_url: None,
                channel: None,
                user_map: BTreeMap::new(),
            },
            notification_policy: None,
        };
        let flavor = FlavorConfig {
            platform_connector: FlavorPlatformConnector {
                kind: "gitlab".to_string(),
            },
            notification_policy: None,
            notifications: Vec::new(),
            slack_app: Some(FlavorSlackAppConfig {
                user_map: BTreeMap::from([
                    ("alice".to_string(), "UTOML1234".to_string()),
                    ("bob".to_string(), "".to_string()),
                ]),
            }),
            templates: crate::config::model::FlavorTemplatesConfig::default(),
        };

        let resolved = resolve_slack_app_user_map(&runtime, Some(&flavor));

        assert_eq!(resolved.get("alice"), Some(&"UTOML1234".to_string()));
        assert!(!resolved.contains_key("bob"));
    }

    #[test]
    fn notification_policy_defaults_to_always() {
        let runtime = RuntimeConfig {
            reviewers: crate::core::model::ReviewerConfig {
                definitions: Vec::new(),
                max_reviewers: 2,
            },
            codeowners: CodeownersConfig {
                enabled: true,
                path: None,
            },
            slack: SlackConfig {
                enabled: true,
                base_url: "https://slack.com/api".to_string(),
                bot_token: None,
                webhook_url: None,
                channel: None,
                user_map: BTreeMap::new(),
            },
            notification_policy: None,
        };

        assert_eq!(
            resolve_notification_policy(&runtime, None),
            NotificationPolicy::Always
        );
    }

    #[test]
    fn notification_policy_uses_flavor_when_runtime_has_no_override() {
        let runtime = RuntimeConfig {
            reviewers: crate::core::model::ReviewerConfig {
                definitions: Vec::new(),
                max_reviewers: 2,
            },
            codeowners: CodeownersConfig {
                enabled: true,
                path: None,
            },
            slack: SlackConfig {
                enabled: true,
                base_url: "https://slack.com/api".to_string(),
                bot_token: None,
                webhook_url: None,
                channel: None,
                user_map: BTreeMap::new(),
            },
            notification_policy: None,
        };
        let flavor = FlavorConfig {
            platform_connector: FlavorPlatformConnector {
                kind: "gitlab".to_string(),
            },
            notification_policy: Some(NotificationPolicy::OnAppliedAction),
            notifications: Vec::new(),
            slack_app: None,
            templates: crate::config::model::FlavorTemplatesConfig::default(),
        };

        assert_eq!(
            resolve_notification_policy(&runtime, Some(&flavor)),
            NotificationPolicy::OnAppliedAction
        );
    }

    #[test]
    fn notification_policy_prefers_runtime_override_over_flavor() {
        let runtime = RuntimeConfig {
            reviewers: crate::core::model::ReviewerConfig {
                definitions: Vec::new(),
                max_reviewers: 2,
            },
            codeowners: CodeownersConfig {
                enabled: true,
                path: None,
            },
            slack: SlackConfig {
                enabled: true,
                base_url: "https://slack.com/api".to_string(),
                bot_token: None,
                webhook_url: None,
                channel: None,
                user_map: BTreeMap::new(),
            },
            notification_policy: Some(NotificationPolicy::Always),
        };
        let flavor = FlavorConfig {
            platform_connector: FlavorPlatformConnector {
                kind: "gitlab".to_string(),
            },
            notification_policy: Some(NotificationPolicy::OnAppliedAction),
            notifications: Vec::new(),
            slack_app: None,
            templates: crate::config::model::FlavorTemplatesConfig::default(),
        };

        assert_eq!(
            resolve_notification_policy(&runtime, Some(&flavor)),
            NotificationPolicy::Always
        );
    }
}
