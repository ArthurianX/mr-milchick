use std::ffi::OsStr;

use anyhow::{Result, bail};

pub const CONFIG_PATH_ENV: &str = "MR_MILCHICK_CONFIG_PATH";
pub const GITLAB_TOKEN_ENV: &str = "GITLAB_TOKEN";
pub const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";
pub const SLACK_BOT_TOKEN_ENV: &str = "MR_MILCHICK_SLACK_BOT_TOKEN";
pub const SLACK_WEBHOOK_URL_ENV: &str = "MR_MILCHICK_SLACK_WEBHOOK_URL";

pub const REMOVED_APP_CONFIG_ENV_VARS: &[&str] = &[
    "MR_MILCHICK_REVIEWERS",
    "MR_MILCHICK_MAX_REVIEWERS",
    "MR_MILCHICK_CODEOWNERS_ENABLED",
    "MR_MILCHICK_CODEOWNERS_PATH",
    "MR_MILCHICK_DRY_RUN",
    "MR_MILCHICK_NOTIFICATION_POLICY",
    "MR_MILCHICK_LLM_ENABLED",
    "MR_MILCHICK_LLM_MODEL_PATH",
    "MR_MILCHICK_LLM_TIMEOUT_MS",
    "MR_MILCHICK_LLM_MAX_PATCH_BYTES",
    "MR_MILCHICK_LLM_CONTEXT_TOKENS",
    "MR_MILCHICK_LLM_TRACE",
    "MR_MILCHICK_SLACK_ENABLED",
    "MR_MILCHICK_SLACK_CHANNEL",
    "MR_MILCHICK_SLACK_BASE_URL",
    "MR_MILCHICK_SLACK_USER_MAP",
    "GITLAB_BASE_URL",
    "GITHUB_API_BASE_URL",
    "MR_MILCHICK_FLAVOR_PATH",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecretEnv {
    pub gitlab_token: Option<String>,
    pub github_token: Option<String>,
    pub slack_bot_token: Option<String>,
    pub slack_webhook_url: Option<String>,
}

pub fn load_config_path() -> Option<String> {
    std::env::var(CONFIG_PATH_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn load_secret_env() -> SecretEnv {
    SecretEnv {
        gitlab_token: read_optional_env(GITLAB_TOKEN_ENV),
        github_token: read_optional_env(GITHUB_TOKEN_ENV),
        slack_bot_token: read_optional_env(SLACK_BOT_TOKEN_ENV),
        slack_webhook_url: read_optional_env(SLACK_WEBHOOK_URL_ENV),
    }
}

pub fn reject_removed_app_config_env() -> Result<()> {
    let present = find_removed_app_config_envs(std::env::vars_os().map(|(name, _)| name));
    if present.is_empty() {
        return Ok(());
    }

    bail!(
        "unsupported legacy app configuration environment variable(s): {}. move non-secret runtime configuration into mr-milchick.toml and use '{}' only for an alternate config path",
        present.join(", "),
        CONFIG_PATH_ENV
    );
}

fn read_optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn find_removed_app_config_envs<I, S>(names: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut present = names
        .into_iter()
        .filter_map(|name| {
            let name = name.as_ref().to_string_lossy();
            REMOVED_APP_CONFIG_ENV_VARS
                .contains(&name.as_ref())
                .then(|| name.to_string())
        })
        .collect::<Vec<_>>();
    present.sort();
    present.dedup();
    present
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_removed_app_config_envs() {
        let present = find_removed_app_config_envs([
            "MR_MILCHICK_REVIEWERS",
            "RUST_LOG",
            "MR_MILCHICK_FLAVOR_PATH",
        ]);

        assert_eq!(
            present,
            vec![
                "MR_MILCHICK_FLAVOR_PATH".to_string(),
                "MR_MILCHICK_REVIEWERS".to_string(),
            ]
        );
    }
}
