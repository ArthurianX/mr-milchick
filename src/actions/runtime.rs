#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStrategy {
    DryRun,
    Real,
}

impl ExecutionStrategy {
    pub fn from_env() -> Self {
        let dry_run = std::env::var("MR_MILCHICK_DRY_RUN")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);

        if dry_run { Self::DryRun } else { Self::Real }
    }
}
