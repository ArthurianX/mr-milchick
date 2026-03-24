use clap::{Parser, Subcommand};

use milchick_runtime::ExecutionMode;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_SHA: &str = env!("BUILD_GIT_SHA");
const BUILD_DATE: &str = env!("BUILD_DATE");

pub fn print_version() {
    println!("mr-milchick {} ({} {})", VERSION, GIT_SHA, BUILD_DATE);
}

#[derive(Debug, Parser)]
#[command(name = "mr-milchick")]
#[command(about = "A pleasantly unsettling steward for GitLab merge requests")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Observe the merge request and report intended actions
    Observe,

    /// Refine the merge request by applying approved actions
    Refine,

    /// Explain decision-making in greater detail
    Explain,

    /// Print version, git SHA and build date
    Version,
}

impl From<Command> for ExecutionMode {
    fn from(value: Command) -> Self {
        match value {
            Command::Observe => ExecutionMode::Observe,
            Command::Refine => ExecutionMode::Refine,
            Command::Explain => ExecutionMode::Explain,
            Command::Version => unreachable!("Version is handled before ExecutionMode conversion"),
        }
    }
}
