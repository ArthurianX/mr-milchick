use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::model::MrMilchickConfig;

const DEFAULT_CONFIG_PATH: &str = "mr-milchick.toml";

/// The `mr-milchick.toml` bundled at compile time so the binary is
/// self-contained when deployed as a CI artifact.
const EMBEDDED_DEFAULT_CONFIG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/mr-milchick.toml"
));

pub fn load_config() -> Result<MrMilchickConfig> {
    match load_config_from(DEFAULT_CONFIG_PATH) {
        Ok(cfg) => Ok(cfg),
        Err(_) => {
            // Fall back to the config baked into the binary at compile time.
            eprintln!(
                "mr-milchick: '{}' not found on disk — using embedded default config",
                DEFAULT_CONFIG_PATH
            );
            load_config_from_str(EMBEDDED_DEFAULT_CONFIG, "<embedded>")
        }
    }
}

pub fn load_config_from(path: impl AsRef<Path>) -> Result<MrMilchickConfig> {
    let path = path.as_ref();

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file '{}'", path.display()))?;

    load_config_from_str(&raw, &path.display().to_string())
}

fn load_config_from_str(raw: &str, label: &str) -> Result<MrMilchickConfig> {
    toml::from_str::<MrMilchickConfig>(raw)
        .with_context(|| format!("failed to parse config file '{}'", label))
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

    #[test]
    fn embedded_default_config_is_valid() {
        let config = load_config_from_str(EMBEDDED_DEFAULT_CONFIG, "<embedded>")
            .expect("embedded default config must be parseable");
        assert!(
            !config.reviewers.fallback_reviewers.is_empty(),
            "embedded config should have at least one fallback reviewer"
        );
    }
}