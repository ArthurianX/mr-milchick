use anyhow::Result;

use crate::cli::{Cli, Command};
use crate::context::env::load_ci_context;
use crate::tone::{ToneCategory, ToneSelector};

pub async fn run(cli: Cli) -> Result<()> {
    let ctx = load_ci_context()?;
    let selector = ToneSelector::default();

    println!("{}", selector.select(ToneCategory::Observation, &ctx));

    match cli.command {
        Command::Observe => {
            println!("No actions were performed.");
            println!("{ctx:#?}");
        }
        Command::Refine => {
            println!("A refinement opportunity has been identified.");
            println!("No actions have been implemented yet.");
            println!("{ctx:#?}");
        }
        Command::Explain => {
            println!("Decision explanation is not yet available.");
            println!("{ctx:#?}");
        }
    }

    Ok(())
}