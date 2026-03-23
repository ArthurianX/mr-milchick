use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::config::model::{
    CodeownersConfig, ReviewerConfig, ReviewerDefinition, RuntimeConfig, SlackConfig,
};
use crate::domain::code_area::CodeArea;

const DEFAULT_MAX_REVIEWERS: usize = 2;
const REVIEWERS_ENV: &str = "MR_MILCHICK_REVIEWERS";
const MAX_REVIEWERS_ENV: &str = "MR_MILCHICK_MAX_REVIEWERS";
const CODEOWNERS_ENABLED_ENV: &str = "MR_MILCHICK_CODEOWNERS_ENABLED";
const CODEOWNERS_PATH_ENV: &str = "MR_MILCHICK_CODEOWNERS_PATH";
const SLACK_ENABLED_ENV: &str = "MR_MILCHICK_SLACK_ENABLED";
const SLACK_BASE_URL_ENV: &str = "MR_MILCHICK_SLACK_BASE_URL";
const SLACK_BOT_TOKEN_ENV: &str = "MR_MILCHICK_SLACK_BOT_TOKEN";
const SLACK_CHANNEL_ENV: &str = "MR_MILCHICK_SLACK_CHANNEL";
const DEFAULT_SLACK_BASE_URL: &str = "https://slack.com/api";
const DEFAULT_CODEOWNERS_CANDIDATES: [&str; 4] = [
    "CODEOWNERS",
    ".github/CODEOWNERS",
    ".gitlab/CODEOWNERS",
    ".CODEOWNERS",
];

#[derive(Debug, Clone, Deserialize)]
struct ReviewerDefinitionDto {
    username: String,
    #[serde(default)]
    areas: Vec<String>,
    #[serde(default, alias = "fallback")]
    is_fallback: bool,
    #[serde(default, alias = "mandatory")]
    is_mandatory: bool,
}

pub fn load_config() -> Result<RuntimeConfig> {
    Ok(RuntimeConfig {
        reviewers: load_reviewers_config()?,
        codeowners: load_codeowners_config()?,
        slack: load_slack_config()?,
    })
}

pub fn resolve_codeowners_path(config: &CodeownersConfig) -> Option<String> {
    if !config.enabled {
        return None;
    }

    if let Some(path) = &config.path {
        return Some(path.clone());
    }

    resolve_first_existing_path(DEFAULT_CODEOWNERS_CANDIDATES)
}

fn resolve_first_existing_path<I, P>(candidates: I) -> Option<String>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    candidates.into_iter().find_map(|candidate| {
        let candidate = candidate.as_ref();
        candidate.exists().then(|| candidate.display().to_string())
    })
}

fn load_reviewers_config() -> Result<ReviewerConfig> {
    let definitions = match std::env::var(REVIEWERS_ENV) {
        Ok(raw) if !raw.trim().is_empty() => parse_reviewer_definitions(&raw)?,
        _ => Vec::new(),
    };

    Ok(ReviewerConfig {
        definitions,
        max_reviewers: parse_max_reviewers()?,
    })
}

fn load_codeowners_config() -> Result<CodeownersConfig> {
    let enabled = match std::env::var(CODEOWNERS_ENABLED_ENV) {
        Ok(raw) if !raw.trim().is_empty() => parse_bool_flag(CODEOWNERS_ENABLED_ENV, &raw)?,
        _ => true,
    };

    let path = std::env::var(CODEOWNERS_PATH_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Ok(CodeownersConfig { enabled, path })
}

fn load_slack_config() -> Result<SlackConfig> {
    let enabled = match std::env::var(SLACK_ENABLED_ENV) {
        Ok(raw) if !raw.trim().is_empty() => parse_bool_flag(SLACK_ENABLED_ENV, &raw)?,
        _ => true,
    };

    let base_url = std::env::var(SLACK_BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_SLACK_BASE_URL.to_string());

    let bot_token = std::env::var(SLACK_BOT_TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let channel = std::env::var(SLACK_CHANNEL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Ok(SlackConfig {
        enabled,
        base_url,
        bot_token,
        channel,
    })
}

fn parse_reviewer_definitions(raw: &str) -> Result<Vec<ReviewerDefinition>> {
    let parsed: Vec<ReviewerDefinitionDto> = serde_json::from_str(raw).with_context(|| {
        format!(
            "failed to parse '{}' as JSON reviewer definitions",
            REVIEWERS_ENV
        )
    })?;

    parsed
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let username = item.username.trim();
            if username.is_empty() {
                bail!(
                    "reviewer entry {} in '{}' is missing a username",
                    index,
                    REVIEWERS_ENV
                );
            }

            let areas = item
                .areas
                .into_iter()
                .map(|area| {
                    CodeArea::from_config_key(&area).ok_or_else(|| {
                        anyhow!(
                            "reviewer '{}' uses unknown area '{}' in '{}'",
                            username,
                            area,
                            REVIEWERS_ENV
                        )
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            Ok(ReviewerDefinition {
                username: username.to_string(),
                areas,
                is_fallback: item.is_fallback,
                is_mandatory: item.is_mandatory,
            })
        })
        .collect()
}

fn parse_max_reviewers() -> Result<usize> {
    match std::env::var(MAX_REVIEWERS_ENV) {
        Ok(raw) if !raw.trim().is_empty() => {
            let value = raw
                .trim()
                .parse::<usize>()
                .with_context(|| format!("'{}' must be a positive integer", MAX_REVIEWERS_ENV))?;

            if value == 0 {
                bail!("'{}' must be greater than zero", MAX_REVIEWERS_ENV);
            }

            Ok(value)
        }
        _ => Ok(DEFAULT_MAX_REVIEWERS),
    }
}

fn parse_bool_flag(name: &str, raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => bail!("'{}' must be one of true/false/1/0/yes/no/on/off", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reviewer_definitions_from_json() {
        let raw = r#"
[
  {"username":"alice","areas":["frontend","packages"]},
  {"username":"milchick-duty","fallback":true},
  {"username":"principal-reviewer","mandatory":true}
]
"#;

        let reviewers = parse_reviewer_definitions(raw).expect("reviewers should parse");

        assert_eq!(reviewers.len(), 3);
        assert_eq!(reviewers[0].username, "alice");
        assert_eq!(
            reviewers[0].areas,
            vec![CodeArea::Frontend, CodeArea::Shared]
        );
        assert!(reviewers[1].is_fallback);
        assert!(reviewers[2].is_mandatory);
    }

    #[test]
    fn resolves_first_existing_codeowners_candidate() {
        let temp_path = std::env::temp_dir().join(format!(
            "mr-milchick-codeowners-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::write(&temp_path, "* @alice").expect("temp codeowners file should be created");

        assert_eq!(
            resolve_first_existing_path([temp_path.as_path()]),
            Some(temp_path.display().to_string())
        );

        std::fs::remove_file(&temp_path).expect("temp codeowners file should be removed");
    }

    #[test]
    fn supports_explicit_codeowners_disable_flag() {
        assert!(!parse_bool_flag(CODEOWNERS_ENABLED_ENV, "false").unwrap());
    }

    #[test]
    fn rejects_unknown_area_name() {
        let raw = r#"[{"username":"alice","areas":["mystery-zone"]}]"#;

        let error = parse_reviewer_definitions(raw).expect_err("unknown areas should fail");
        assert!(error.to_string().contains("unknown area"));
    }

    #[test]
    fn slack_config_defaults_to_enabled_without_values() {
        let config = load_slack_config().expect("slack config should load");

        assert!(config.enabled);
        assert_eq!(config.base_url, DEFAULT_SLACK_BASE_URL);
        assert_eq!(config.bot_token, None);
        assert_eq!(config.channel, None);
    }

    #[test]
    fn supports_explicit_slack_disable_flag() {
        assert!(!parse_bool_flag(SLACK_ENABLED_ENV, "false").unwrap());
    }
}
