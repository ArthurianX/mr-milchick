use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use tracing::debug;

use crate::config::model::MrMilchickConfig;

const DEFAULT_CONFIG_PATH: &str = "mr-milchick.toml";

pub fn load_config() -> Result<MrMilchickConfig> {
    load_config_from(DEFAULT_CONFIG_PATH)
}

pub fn load_config_from(path: impl AsRef<Path>) -> Result<MrMilchickConfig> {
    let path = path.as_ref();

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file '{}'", path.display()))?;

    let config = toml::from_str::<MrMilchickConfig>(&raw)
        .with_context(|| format!("failed to parse config file '{}'", path.display()))?;

    Ok(config)
}

pub fn resolve_codeowners_path(config: &crate::config::model::MrMilchickConfig) -> Option<String> {
    let raw_env = std::env::var("MR_MILCHICK_CODEOWNERS_PATH").ok();
    debug!(raw_env = ?raw_env, "MR_MILCHICK_CODEOWNERS_PATH env value");

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "<unavailable>".to_string());
    debug!(cwd = %cwd, "current working directory");

    let resolved = match raw_env {
        Some(ref path) if !path.trim().is_empty() => Some(path.clone()),
        _ => config
            .codeowners
            .as_ref()
            .filter(|c| c.enabled)
            .map(|c| c.path.clone()),
    };

    match &resolved {
        Some(path) => {
            let absolute = Path::new(path)
                .canonicalize()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| format!("{} (could not canonicalize)", path));
            let exists = Path::new(path).exists();
            debug!(resolved_path = %path, absolute_path = %absolute, exists = %exists, "resolved CODEOWNERS path");
        }
        None => {
            debug!("no CODEOWNERS path resolved — codeowners disabled or not configured");
        }
    }

    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_config_from_toml_string() {
        let raw = r#"
[reviewers]
max_reviewers = 2
fallback_reviewers = ["milchick-duty"]
frontend = ["alice", "bob"]
backend = ["carol", "dave"]
shared = ["erin", "frank"]
devops = ["grace"]
documentation = ["heidi"]
tests = ["ivan"]
"#;

        let config: MrMilchickConfig = toml::from_str(raw).expect("config should parse");

        assert_eq!(config.reviewers.max_reviewers, 2);
        assert_eq!(config.reviewers.frontend, vec!["alice", "bob"]);
        assert_eq!(config.reviewers.fallback_reviewers, vec!["milchick-duty"]);
    }
}