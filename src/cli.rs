use clap::{Parser, Subcommand};

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
}
