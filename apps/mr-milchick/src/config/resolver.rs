use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};

use crate::core::domain::code_area::CodeArea;
use crate::core::model::{
    NotificationSinkKind, ReviewPlatformKind, ReviewerConfig, ReviewerDefinition,
};

use super::env::{self, SecretEnv};
use super::schema;

const DEFAULT_CONFIG_PATH: &str = "mr-milchick.toml";
const DEFAULT_MAX_REVIEWERS: usize = 2;
const DEFAULT_GITLAB_BASE_URL: &str = "https://gitlab.com/api/v4";
const DEFAULT_GITHUB_BASE_URL: &str = "https://api.github.com";
const DEFAULT_SLACK_BASE_URL: &str = "https://slack.com/api";
const DEFAULT_LLM_TIMEOUT_MS: u64 = 15_000;
const DEFAULT_LLM_MAX_PATCH_BYTES: usize = 32 * 1024;
const DEFAULT_LLM_CONTEXT_TOKENS: usize = 4_096;
const DEFAULT_CODEOWNERS_CANDIDATES: [&str; 4] = [
    "CODEOWNERS",
    ".github/CODEOWNERS",
    ".gitlab/CODEOWNERS",
    ".CODEOWNERS",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub platform: PlatformConfig,
    pub execution: ExecutionConfig,
    pub reviewers: ReviewerConfig,
    pub codeowners: CodeownersConfig,
    pub inference: InferenceConfig,
    pub notifications: NotificationsConfig,
    pub templates: schema::TemplatesConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformConfig {
    pub kind: ReviewPlatformKind,
    pub base_url: String,
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionConfig {
    pub dry_run: bool,
    pub notification_policy: schema::NotificationPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersConfig {
    pub enabled: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceConfig {
    pub enabled: bool,
    pub model_path: Option<String>,
    pub timeout_ms: u64,
    pub max_patch_bytes: usize,
    pub context_tokens: usize,
    pub trace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationsConfig {
    pub slack_app: SlackAppConfig,
    pub slack_workflow: SlackWorkflowConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackAppConfig {
    pub enabled: bool,
    pub base_url: String,
    pub bot_token: Option<String>,
    pub channel: Option<String>,
    pub user_map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackWorkflowConfig {
    pub enabled: bool,
    pub webhook_url: Option<String>,
    pub channel: Option<String>,
}

pub fn load_resolved_config() -> Result<ResolvedConfig> {
    env::reject_removed_app_config_env()?;
    let config_path = env::load_config_path();
    let file = load_config_file(config_path.as_deref())?;
    resolve_config(file, env::load_secret_env())
}

pub fn resolve_config(file: schema::ConfigFile, secrets: SecretEnv) -> Result<ResolvedConfig> {
    let platform = resolve_platform_config(&file.platform, &secrets)?;
    validate_compiled_platform(platform.kind)?;

    let notifications = resolve_notifications_config(&file.notifications, &secrets);
    validate_compiled_notifications(&notifications)?;

    Ok(ResolvedConfig {
        platform,
        execution: resolve_execution_config(&file.execution),
        reviewers: resolve_reviewer_config(&file.reviewers)?,
        codeowners: resolve_codeowners_config(&file.codeowners),
        inference: resolve_inference_config(&file.inference)?,
        notifications,
        templates: file.templates,
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

pub fn compiled_platform_kind() -> ReviewPlatformKind {
    #[cfg(feature = "gitlab")]
    {
        ReviewPlatformKind::GitLab
    }
    #[cfg(feature = "github")]
    {
        ReviewPlatformKind::GitHub
    }
}

pub fn compiled_notification_sinks() -> Vec<NotificationSinkKind> {
    let sinks = Vec::new();
    #[cfg(feature = "slack-app")]
    let sinks = {
        let mut sinks = sinks;
        sinks.push(NotificationSinkKind::SlackApp);
        sinks
    };
    #[cfg(feature = "slack-workflow")]
    let sinks = {
        let mut sinks = sinks;
        sinks.push(NotificationSinkKind::SlackWorkflow);
        sinks
    };
    sinks
}

pub fn llm_backend_compiled() -> bool {
    cfg!(feature = "llm-local")
}

fn load_config_file(path_override: Option<&str>) -> Result<schema::ConfigFile> {
    let path = path_override.unwrap_or(DEFAULT_CONFIG_PATH);
    if !Path::new(path).exists() {
        return Ok(schema::ConfigFile::default());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file '{}'", path))?;
    toml::from_str::<schema::ConfigFile>(&raw)
        .with_context(|| format!("failed to parse config file '{}'", path))
}

fn resolve_platform_config(
    file: &schema::PlatformConfig,
    secrets: &SecretEnv,
) -> Result<PlatformConfig> {
    let kind = file
        .kind
        .map(resolve_platform_kind)
        .unwrap_or_else(compiled_platform_kind);
    let base_url = sanitize_optional(file.base_url.clone()).unwrap_or_else(|| match kind {
        ReviewPlatformKind::GitLab => DEFAULT_GITLAB_BASE_URL.to_string(),
        ReviewPlatformKind::GitHub => DEFAULT_GITHUB_BASE_URL.to_string(),
    });

    let token = match kind {
        ReviewPlatformKind::GitLab => secrets.gitlab_token.clone(),
        ReviewPlatformKind::GitHub => secrets.github_token.clone(),
    };

    Ok(PlatformConfig {
        kind,
        base_url,
        token,
    })
}

fn resolve_execution_config(file: &schema::ExecutionConfig) -> ExecutionConfig {
    ExecutionConfig {
        dry_run: file.dry_run.unwrap_or(false),
        notification_policy: file
            .notification_policy
            .unwrap_or(schema::NotificationPolicy::Always),
    }
}

fn resolve_reviewer_config(file: &schema::ReviewersConfig) -> Result<ReviewerConfig> {
    let max_reviewers = match file.max_reviewers {
        Some(0) => bail!("'reviewers.max_reviewers' must be greater than zero"),
        Some(value) => value,
        None => DEFAULT_MAX_REVIEWERS,
    };

    let definitions = file
        .definitions
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let username = item.username.trim();
            if username.is_empty() {
                bail!(
                    "reviewer entry {} in 'reviewers.definitions' is missing a username",
                    index
                );
            }

            let areas = item
                .areas
                .iter()
                .map(|area| {
                    CodeArea::from_config_key(area).ok_or_else(|| {
                        anyhow!(
                            "reviewer '{}' uses unknown area '{}' in 'reviewers.definitions'",
                            username,
                            area
                        )
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            Ok(ReviewerDefinition {
                username: username.to_string(),
                areas,
                is_fallback: item.fallback,
                is_mandatory: item.mandatory,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ReviewerConfig {
        definitions,
        max_reviewers,
    })
}

fn resolve_codeowners_config(file: &schema::CodeownersConfig) -> CodeownersConfig {
    CodeownersConfig {
        enabled: file.enabled.unwrap_or(true),
        path: sanitize_optional(file.path.clone()),
    }
}

fn resolve_inference_config(file: &schema::InferenceConfig) -> Result<InferenceConfig> {
    Ok(InferenceConfig {
        enabled: file.enabled.unwrap_or(false),
        model_path: sanitize_optional(file.model_path.clone()),
        timeout_ms: resolve_positive_u64(
            file.timeout_ms,
            DEFAULT_LLM_TIMEOUT_MS,
            "inference.timeout_ms",
        )?,
        max_patch_bytes: resolve_positive_usize(
            file.max_patch_bytes,
            DEFAULT_LLM_MAX_PATCH_BYTES,
            "inference.max_patch_bytes",
        )?,
        context_tokens: resolve_positive_usize(
            file.context_tokens,
            DEFAULT_LLM_CONTEXT_TOKENS,
            "inference.context_tokens",
        )?,
        trace: file.trace.unwrap_or(false),
    })
}

fn resolve_notifications_config(
    file: &schema::NotificationsConfig,
    secrets: &SecretEnv,
) -> NotificationsConfig {
    NotificationsConfig {
        slack_app: SlackAppConfig {
            enabled: file.slack_app.enabled.unwrap_or(false),
            base_url: sanitize_optional(file.slack_app.base_url.clone())
                .unwrap_or_else(|| DEFAULT_SLACK_BASE_URL.to_string()),
            bot_token: secrets.slack_bot_token.clone(),
            channel: sanitize_optional(file.slack_app.channel.clone()),
            user_map: sanitize_user_map(&file.slack_app.user_map),
        },
        slack_workflow: SlackWorkflowConfig {
            enabled: file.slack_workflow.enabled.unwrap_or(false),
            webhook_url: secrets.slack_webhook_url.clone(),
            channel: sanitize_optional(file.slack_workflow.channel.clone()),
        },
    }
}

fn validate_compiled_platform(platform: ReviewPlatformKind) -> Result<()> {
    let compiled = compiled_platform_kind();
    if platform != compiled {
        bail!(
            "config platform '{}' does not match compiled capability '{}'",
            platform.as_str(),
            compiled.as_str()
        );
    }
    Ok(())
}

fn validate_compiled_notifications(config: &NotificationsConfig) -> Result<()> {
    if config.slack_app.enabled && !cfg!(feature = "slack-app") {
        bail!("config enables 'slack-app' notifications but that sink is not compiled in");
    }

    if config.slack_workflow.enabled && !cfg!(feature = "slack-workflow") {
        bail!("config enables 'slack-workflow' notifications but that sink is not compiled in");
    }

    Ok(())
}

fn resolve_platform_kind(kind: schema::PlatformKind) -> ReviewPlatformKind {
    match kind {
        schema::PlatformKind::GitLab => ReviewPlatformKind::GitLab,
        schema::PlatformKind::GitHub => ReviewPlatformKind::GitHub,
    }
}

fn sanitize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn sanitize_user_map(raw: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut sanitized = BTreeMap::new();
    for (username, user_id) in raw {
        let username = username.trim();
        let user_id = user_id.trim();
        if username.is_empty() || user_id.is_empty() {
            continue;
        }
        sanitized.insert(username.to_string(), user_id.to_string());
    }
    sanitized
}

fn resolve_positive_u64(value: Option<u64>, default: u64, field: &str) -> Result<u64> {
    match value {
        Some(0) => bail!("'{}' must be greater than zero", field),
        Some(value) => Ok(value),
        None => Ok(default),
    }
}

fn resolve_positive_usize(value: Option<usize>, default: usize, field: &str) -> Result<usize> {
    match value {
        Some(0) => bail!("'{}' must be greater than zero", field),
        Some(value) => Ok(value),
        None => Ok(default),
    }
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn resolved_config_defaults_without_file_or_secrets() {
        let config = resolve_config(schema::ConfigFile::default(), SecretEnv::default())
            .expect("resolved config should load");

        assert_eq!(config.platform.kind, compiled_platform_kind());
        assert_eq!(config.reviewers.max_reviewers, DEFAULT_MAX_REVIEWERS);
        assert!(config.reviewers.definitions.is_empty());
        assert!(config.codeowners.enabled);
        assert!(!config.execution.dry_run);
        assert_eq!(
            config.execution.notification_policy,
            schema::NotificationPolicy::Always
        );
        assert!(!config.inference.enabled);
        assert!(!config.inference.trace);
        assert!(!config.notifications.slack_app.enabled);
        assert!(!config.notifications.slack_workflow.enabled);
    }

    #[test]
    fn parses_full_config_file_shape() {
        let slack_workflow_enabled = cfg!(feature = "slack-workflow");
        let raw = format!(
            r###"
[platform]
kind = "gitlab"
base_url = "https://gitlab.example.com/api/v4"

[execution]
dry_run = true
notification_policy = "on-applied-action"

[reviewers]
max_reviewers = 3

[[reviewers.definitions]]
username = "milchick-duty"
fallback = true

[[reviewers.definitions]]
username = "principal-reviewer"
mandatory = true

[[reviewers.definitions]]
username = "alice"
areas = ["frontend", "packages"]

[codeowners]
enabled = false
path = ".gitlab/CODEOWNERS"

[inference]
enabled = true
model_path = "/models/review.gguf"
timeout_ms = 20000
max_patch_bytes = 48000
context_tokens = 8192
trace = true

[notifications.slack_app]
enabled = true
channel = "C123"
base_url = "https://slack.example.test/api"

[notifications.slack_app.user_map]
"alice" = "U123"
"bob" = ""

[notifications.slack_workflow]
enabled = {slack_workflow_enabled}
channel = "C456"

[templates.gitlab]
summary = "## {{{{summary_title}}}}"

[templates.slack_app]
first_root = "hello"
"###
        );

        let file = toml::from_str::<schema::ConfigFile>(&raw).expect("config file should parse");
        let config = resolve_config(
            file,
            SecretEnv {
                gitlab_token: Some("gitlab-token".to_string()),
                github_token: None,
                slack_bot_token: Some("xoxb-token".to_string()),
                slack_webhook_url: Some("https://hooks.slack.com/triggers/test".to_string()),
            },
        )
        .expect("resolved config should load");

        assert_eq!(config.platform.kind, ReviewPlatformKind::GitLab);
        assert_eq!(
            config.platform.base_url,
            "https://gitlab.example.com/api/v4"
        );
        assert_eq!(config.platform.token.as_deref(), Some("gitlab-token"));
        assert!(config.execution.dry_run);
        assert_eq!(
            config.execution.notification_policy,
            schema::NotificationPolicy::OnAppliedAction
        );
        assert_eq!(config.reviewers.max_reviewers, 3);
        assert_eq!(config.reviewers.definitions.len(), 3);
        assert!(!config.codeowners.enabled);
        assert_eq!(
            config.codeowners.path.as_deref(),
            Some(".gitlab/CODEOWNERS")
        );
        assert!(config.inference.enabled);
        assert!(config.inference.trace);
        assert_eq!(
            config.notifications.slack_app.channel.as_deref(),
            Some("C123")
        );
        assert_eq!(
            config.notifications.slack_app.bot_token.as_deref(),
            Some("xoxb-token")
        );
        assert_eq!(
            config.notifications.slack_workflow.webhook_url.as_deref(),
            Some("https://hooks.slack.com/triggers/test")
        );
        assert_eq!(
            config.notifications.slack_app.user_map.get("alice"),
            Some(&"U123".to_string())
        );
        assert!(!config.notifications.slack_app.user_map.contains_key("bob"));
        assert_eq!(
            config.templates.gitlab.summary.as_deref(),
            Some("## {{summary_title}}")
        );
    }

    #[test]
    fn injects_platform_and_slack_secrets_from_env_layer() {
        let slack_workflow_enabled = cfg!(feature = "slack-workflow");
        let config = resolve_config(
            toml::from_str::<schema::ConfigFile>(&format!(
                r#"
[platform]
kind = "gitlab"

[notifications.slack_app]
enabled = true
channel = "C123"

[notifications.slack_workflow]
enabled = {slack_workflow_enabled}
channel = "C456"
 "#,
            ))
            .expect("config file should parse"),
            SecretEnv {
                gitlab_token: Some("gitlab-token".to_string()),
                github_token: None,
                slack_bot_token: Some("xoxb-token".to_string()),
                slack_webhook_url: Some("https://hooks.slack.com/triggers/test".to_string()),
            },
        )
        .expect("resolved config should load");

        assert_eq!(config.platform.token.as_deref(), Some("gitlab-token"));
        assert_eq!(
            config.notifications.slack_app.bot_token.as_deref(),
            Some("xoxb-token")
        );
        assert_eq!(
            config.notifications.slack_workflow.webhook_url.as_deref(),
            Some("https://hooks.slack.com/triggers/test")
        );
    }

    #[test]
    fn rejects_positive_values_of_zero() {
        let error = resolve_config(
            toml::from_str::<schema::ConfigFile>(
                r#"
[reviewers]
max_reviewers = 0
"#,
            )
            .expect("config file should parse"),
            SecretEnv::default(),
        )
        .expect_err("zero max reviewers should fail");

        assert!(error.to_string().contains("reviewers.max_reviewers"));
    }

    #[test]
    fn rejects_unknown_reviewer_area() {
        let error = resolve_config(
            toml::from_str::<schema::ConfigFile>(
                r#"
[[reviewers.definitions]]
username = "alice"
areas = ["mystery-zone"]
"#,
            )
            .expect("config file should parse"),
            SecretEnv::default(),
        )
        .expect_err("unknown area should fail");

        assert!(error.to_string().contains("unknown area"));
    }

    #[test]
    fn rejects_configured_platform_that_does_not_match_binary() {
        let wrong_platform = match compiled_platform_kind() {
            ReviewPlatformKind::GitLab => "github",
            ReviewPlatformKind::GitHub => "gitlab",
        };
        let error = resolve_config(
            toml::from_str::<schema::ConfigFile>(&format!(
                r#"
[platform]
kind = "{wrong_platform}"
"#
            ))
            .expect("config file should parse"),
            SecretEnv::default(),
        )
        .expect_err("mismatched platform should fail");

        assert!(
            error
                .to_string()
                .contains("does not match compiled capability")
        );
    }

    #[cfg(not(feature = "slack-app"))]
    #[test]
    fn rejects_enabled_slack_app_when_not_compiled() {
        let error = resolve_config(
            toml::from_str::<schema::ConfigFile>(
                r#"
[notifications.slack_app]
enabled = true
"#,
            )
            .expect("config file should parse"),
            SecretEnv::default(),
        )
        .expect_err("enabled slack app should fail");

        assert!(error.to_string().contains("slack-app"));
    }

    #[cfg(not(feature = "slack-workflow"))]
    #[test]
    fn rejects_enabled_slack_workflow_when_not_compiled() {
        let error = resolve_config(
            toml::from_str::<schema::ConfigFile>(
                r#"
[notifications.slack_workflow]
enabled = true
"#,
            )
            .expect("config file should parse"),
            SecretEnv::default(),
        )
        .expect_err("enabled slack workflow should fail");

        assert!(error.to_string().contains("slack-workflow"));
    }

    #[test]
    fn checked_in_config_examples_parse_with_current_schema() {
        for path in ["mr-milchick.toml", "mr-milchick.github.toml"] {
            let raw = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("failed to read {path}: {error}"));
            toml::from_str::<schema::ConfigFile>(&raw)
                .unwrap_or_else(|error| panic!("{path} should parse with current schema: {error}"));
        }
    }
}
