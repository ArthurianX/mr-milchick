use anyhow::Result;

use crate::config::loader::{load_config, resolve_codeowners_path};
use crate::actions::executor::{ActionExecutor, DryRunExecutor, ExecutionReport, ExecutedAction};
use crate::actions::model::Action;
use crate::actions::planner::enrich_with_reviewer_assignment;
use crate::actions::runtime::ExecutionStrategy;
use crate::cli::Cli;
use crate::context::builder::build_ci_context;
use crate::domain::reviewer_routing::{recommend_reviewers_with_codeowners, ReviewerRoutingConfig};
use crate::domain::snapshot_analysis::summarize_areas;
use crate::gitlab::api::{GitLabConfig, MergeRequestSnapshot};
use crate::gitlab::client::GitLabClient;
use crate::rules::engine::evaluate_rules;
use crate::rules::model::RuleOutcome;
use crate::tone::{ToneCategory, ToneSelector};
use crate::comment::render::render_summary_comment;
use crate::domain::codeowners::matcher::match_usernames;
use crate::domain::codeowners::parser::parse_codeowners_file;
use crate::domain::codeowners::matcher::collect_usernames_for_snapshot;
use crate::domain::codeowners::context::CodeownersContext;

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
}

fn load_app_config_context() -> AppConfigContext {
    match load_config() {
        Ok(config) => {
            let routing_config = ReviewerRoutingConfig::from_config(&config);
            let codeowners = CodeownersContext {
                file: resolve_codeowners_path(&config)
                    .and_then(|path| parse_codeowners_file(&path).ok()),
            };

            AppConfigContext {
                routing_config,
                codeowners,
            }
        }
        Err(_) => AppConfigContext {
            routing_config: ReviewerRoutingConfig::example(),
            codeowners: CodeownersContext::empty(),
        },
    }
}

fn codeowners_usernames_for_snapshot(
    app_config: &AppConfigContext,
    snapshot: &MergeRequestSnapshot,
) -> Vec<String> {
    if let Some(codeowners) = &app_config.codeowners.file {
        collect_usernames_for_snapshot(codeowners, snapshot)
    } else {
        Vec::new()
    }
}

fn has_non_comment_actions(plan: &crate::actions::model::ActionPlan) -> bool {
    plan.actions
        .iter()
        .any(|action| !matches!(action, Action::PostComment { .. }))
}

pub async fn run(cli: Cli) -> Result<()> {
    let mode: ExecutionMode = cli.command.into();
    run_mode(mode).await
}

pub async fn run_mode(mode: ExecutionMode) -> Result<()> {
    let ctx = build_ci_context()?;
    let selector = ToneSelector::default();

    println!("{}", selector.select(ToneCategory::Observation, &ctx));

    if !ctx.is_merge_request_pipeline() {
        println!("This pipeline does not currently present merge request responsibilities.");
        return Ok(());
    }

    let mut outcome = evaluate_rules(&ctx);
    let app_config = load_app_config_context();

    let snapshot = maybe_fetch_snapshot(&ctx).await?;

    if let Some(snapshot) = &snapshot {
        outcome = enrich_with_reviewer_assignment(
            outcome,
            snapshot,
            &app_config.routing_config,
            &app_config.codeowners,
        );
    }

    let summary_comment = render_summary_comment(&outcome);
    outcome
        .action_plan
        .push(Action::PostComment { body: summary_comment });
    let has_meaningful_actions = has_non_comment_actions(&outcome.action_plan);

    match mode {
        ExecutionMode::Observe => {
            print_outcome(&outcome);
            print_action_plan(&outcome);

            if outcome.is_empty() && !has_meaningful_actions {
                println!("{}", selector.select(ToneCategory::Resolution, &ctx));
            }
        }
        ExecutionMode::Refine => {
            if outcome.action_plan.has_fail_pipeline() || outcome.has_blocking_findings() {
                println!("{}", selector.select(ToneCategory::Blocking, &ctx));
            } else if outcome.is_empty() && !has_meaningful_actions {
                println!("{}", selector.select(ToneCategory::Resolution, &ctx));
            } else {
                println!("{}", selector.select(ToneCategory::Refinement, &ctx));
            }

            print_outcome(&outcome);
            print_action_plan(&outcome);

            let strategy = ExecutionStrategy::from_env();
            let report = execute_action_plan(strategy, &ctx, &outcome.action_plan).await?;
            print_execution_report(&report);

            if outcome.action_plan.has_fail_pipeline() || outcome.has_blocking_findings() {
                anyhow::bail!("merge request policy requirements were not satisfied");
            }
        }
        ExecutionMode::Explain => {
            println!("Decision explanation:");
            print_outcome(&outcome);
            print_action_plan(&outcome);
            let summary_comment = render_summary_comment(&outcome);
            println!("Structured summary comment preview:");
            println!("---");
            println!("{}", summary_comment);
            println!("---");

            if let Some(snapshot) = &snapshot {
                print_snapshot_details(snapshot);

                if snapshot.details.is_draft {
                    println!("Reviewer assignment is currently deferred because this merge request is draft.");
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
                let codeowners_usernames = codeowners_usernames_for_snapshot(&app_config, snapshot);
                let recommendation = recommend_reviewers_with_codeowners(
                    &area_summary,
                    &app_config.routing_config,
                    &excluded_reviewers,
                    &codeowners_usernames,
                );

                if recommendation.is_empty() {
                    println!("No reviewer recommendation was produced.");
                } else {
                    println!("Recommended reviewers:");

                    for reviewer in &recommendation.reviewers {
                        println!("- {}", reviewer);
                    }

                    println!("Routing reasons:");

                    for reason in &recommendation.reasons {
                        println!("- {}", reason);
                    }
                }

                if let Some(codeowners) = &app_config.codeowners.file {
                    println!("CODEOWNERS matches:");

                    for file in &snapshot.changed_files {
                        let owners = match_usernames(codeowners, &file.new_path);

                        if owners.is_empty() {
                            println!("- {} => no individual owners", file.new_path);
                        } else {
                            println!("- {} => {}", file.new_path, owners.join(", "));
                        }
                    }
                }

                if app_config.codeowners.is_enabled() {
                    let usernames = codeowners_usernames_for_snapshot(&app_config, snapshot);

                    println!("CODEOWNERS aggregated usernames:");

                    if usernames.is_empty() {
                        println!("- none");
                    } else {
                        for username in usernames {
                            println!("- {}", username);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn execute_action_plan(
    strategy: ExecutionStrategy,
    ctx: &crate::context::model::CiContext,
    plan: &crate::actions::model::ActionPlan,
) -> Result<ExecutionReport> {
    match strategy {
        ExecutionStrategy::DryRun => {
            let executor = DryRunExecutor;
            executor.execute(plan).await
        }
        ExecutionStrategy::Real => {
            let mr_iid = ctx
                .merge_request_iid()
                .ok_or_else(|| anyhow::anyhow!("missing merge request IID for execution"))?;

            let config = GitLabConfig::from_env()?;
            let client = GitLabClient::new(config);

            let executor = crate::actions::executor::gitlab::GitLabExecutor {
                client: &client,
                project_id: ctx.project_id(),
                merge_request_iid: mr_iid,
            };

            executor.execute(plan).await
        }
    }
}
async fn maybe_fetch_snapshot(
    ctx: &crate::context::model::CiContext,
) -> Result<Option<MergeRequestSnapshot>> {
    let Some(mr_iid) = ctx.merge_request_iid() else {
        return Ok(None);
    };

    let config = GitLabConfig::from_env()?;
    let client = GitLabClient::new(config);
    let snapshot = client
        .get_merge_request_snapshot(ctx.project_id(), mr_iid)
        .await?;

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
    if outcome.action_plan.is_empty() {
        println!("No actions are currently planned.");
        return;
    }

    println!("Planned actions:");

    for action in &outcome.action_plan.actions {
        match action {
            Action::PostComment { body } => {
                println!("- [PostComment] {}", body);
            }
            Action::AssignReviewers { reviewers } => {
                println!("- [AssignReviewers] {}", reviewers.join(", "));
            }
            Action::FailPipeline { reason } => {
                println!("- [FailPipeline] {}", reason);
            }
        }
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
                println!("- [CommentPosted] {}", body);
            }
            ExecutedAction::ReviewersAssigned { reviewers } => {
                println!("- [ReviewersAssigned] {}", reviewers.join(", "));
            }
            ExecutedAction::PipelineFailurePlanned { reason } => {
                println!("- [PipelineFailurePlanned] {}", reason);
            }
            ExecutedAction::CommentSkippedAlreadyPresent { body } => {
                println!("- [CommentSkippedAlreadyPresent] {}", body);
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