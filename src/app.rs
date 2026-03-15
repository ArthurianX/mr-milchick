use anyhow::Result;

use crate::actions::model::Action;
use crate::cli::Cli;
use crate::context::builder::build_ci_context;
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
            println!("No actions have been implemented yet.");

            if outcome.action_plan.has_fail_pipeline() || outcome.has_blocking_findings() {
                anyhow::bail!("merge request policy requirements were not satisfied");
            }
        }
        ExecutionMode::Explain => {
            println!("Decision explanation:");
            print_outcome(&outcome);
            print_action_plan(&outcome);
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