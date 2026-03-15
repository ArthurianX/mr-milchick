use anyhow::Result;

use crate::actions::executor::{ActionExecutor, DryRunExecutor, ExecutionReport, ExecutedAction};
use crate::actions::model::Action;
use crate::cli::Cli;
use crate::context::builder::build_ci_context;
use crate::gitlab::api::GitLabConfig;
use crate::gitlab::client::GitLabClient;
use crate::rules::engine::evaluate_rules;
use crate::rules::model::RuleOutcome;
use crate::tone::{ToneCategory, ToneSelector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Observe,
    Refine,
    Explain,
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

    let outcome = evaluate_rules(&ctx);

    match mode {
        ExecutionMode::Observe => {
            print_outcome(&outcome);
            print_action_plan(&outcome);

            if outcome.is_empty() && outcome.action_plan.is_empty() {
                println!("{}", selector.select(ToneCategory::Resolution, &ctx));
            }
        }
        ExecutionMode::Refine => {
            if outcome.action_plan.has_fail_pipeline() || outcome.has_blocking_findings() {
                println!("{}", selector.select(ToneCategory::Blocking, &ctx));
            } else if outcome.is_empty() && outcome.action_plan.is_empty() {
                println!("{}", selector.select(ToneCategory::Resolution, &ctx));
            } else {
                println!("{}", selector.select(ToneCategory::Refinement, &ctx));
            }

            print_outcome(&outcome);
            print_action_plan(&outcome);

            let executor = DryRunExecutor;
            let report = executor.execute(&outcome.action_plan)?;
            print_execution_report(&report);

            if outcome.action_plan.has_fail_pipeline() || outcome.has_blocking_findings() {
                anyhow::bail!("merge request policy requirements were not satisfied");
            }
        }
        ExecutionMode::Explain => {
            println!("Decision explanation:");
            print_outcome(&outcome);
            print_action_plan(&outcome);

            if let Some(mr_iid) = ctx.merge_request_iid() {
                let config = GitLabConfig::from_env()?;
                let client = GitLabClient::new(config);
                let snapshot = client
                    .get_merge_request_snapshot(ctx.project_id(), mr_iid)
                    .await?;

                println!("Merge request details:");
                println!("- [Title] {}", snapshot.details.title);
                println!("- [State] {}", snapshot.details.state.as_str());
                println!("- [Draft] {}", snapshot.details.is_draft);
                println!("- [WebUrl] {}", snapshot.details.web_url);
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
        }
    }

    Ok(())
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
            ExecutedAction::CommentPlanned { body } => {
                println!("- [CommentPlanned] {}", body);
            }
            ExecutedAction::ReviewersPlanned { reviewers } => {
                println!("- [ReviewersPlanned] {}", reviewers.join(", "));
            }
            ExecutedAction::PipelineFailurePlanned { reason } => {
                println!("- [PipelineFailurePlanned] {}", reason);
            }
        }
    }
}