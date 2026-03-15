use anyhow::Result;

use crate::cli::Cli;
use crate::context::builder::build_ci_context;
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

    match mode {
        ExecutionMode::Observe => {
            println!("No actions were performed.");
            println!("{ctx:#?}");
        }
        ExecutionMode::Refine => {
            println!("{}", selector.select(ToneCategory::Refinement, &ctx));
            println!("No actions have been implemented yet.");
            println!("{ctx:#?}");
        }
        ExecutionMode::Explain => {
            println!("Decision explanation is not yet available.");
            println!("{ctx:#?}");
        }
    }

    Ok(())
}