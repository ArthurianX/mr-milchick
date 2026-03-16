use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

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
    if let Ok(path) = std::env::var("MR_MILCHICK_CODEOWNERS_PATH") {
        if !path.trim().is_empty() {
            return Some(path);
        }
    }

    config
        .codeowners
        .as_ref()
        .filter(|c| c.enabled)
        .map(|c| c.path.clone())
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