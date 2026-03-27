use clap::{Args, Parser, Subcommand};

use crate::runtime::ExecutionMode;

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
    Observe(ObserveArgs),

    /// Refine the merge request by applying approved actions
    Refine(RefineArgs),

    /// Explain decision-making in greater detail
    Explain(ExplainArgs),

    /// Print version, git SHA and build date
    Version,
}

#[derive(Debug, Args, Clone, Default)]
pub struct FixtureArgs {
    /// Load synthetic review data from a TOML fixture instead of GitLab CI
    #[arg(long)]
    pub fixture: Option<String>,
}

#[derive(Debug, Args, Clone, Default)]
pub struct ObserveArgs {
    #[command(flatten)]
    pub fixture: FixtureArgs,
}

#[derive(Debug, Args, Clone, Default)]
pub struct ExplainArgs {
    #[command(flatten)]
    pub fixture: FixtureArgs,
}

#[derive(Debug, Args, Clone, Default)]
pub struct RefineArgs {
    #[command(flatten)]
    pub fixture: FixtureArgs,

    /// Actually deliver notifications when running from a fixture
    #[arg(long)]
    pub send_notifications: bool,
}

impl Command {
    pub fn execution_mode(&self) -> Option<ExecutionMode> {
        match self {
            Command::Observe(_) => Some(ExecutionMode::Observe),
            Command::Refine(_) => Some(ExecutionMode::Refine),
            Command::Explain(_) => Some(ExecutionMode::Explain),
            Command::Version => None,
        }
    }

    pub fn fixture_path(&self) -> Option<&str> {
        match self {
            Command::Observe(args) => args.fixture.fixture.as_deref(),
            Command::Refine(args) => args.fixture.fixture.as_deref(),
            Command::Explain(args) => args.fixture.fixture.as_deref(),
            Command::Version => None,
        }
    }

    pub fn send_notifications(&self) -> bool {
        match self {
            Command::Refine(args) => args.send_notifications,
            _ => false,
        }
    }
}
