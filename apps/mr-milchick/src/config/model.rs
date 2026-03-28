pub use crate::core::model::{ReviewerConfig, ReviewerDefinition};
use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub reviewers: ReviewerConfig,
    pub codeowners: CodeownersConfig,
    pub slack: SlackConfig,
    pub notification_policy: Option<NotificationPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeownersConfig {
    pub enabled: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackConfig {
    pub enabled: bool,
    pub base_url: String,
    pub bot_token: Option<String>,
    pub webhook_url: Option<String>,
    pub channel: Option<String>,
    pub user_map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationPolicy {
    Always,
    OnAppliedAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FlavorConfig {
    pub review_platform: FlavorReviewPlatform,
    #[serde(default)]
    pub notification_policy: Option<NotificationPolicy>,
    #[serde(default)]
    pub notifications: Vec<FlavorNotification>,
    #[serde(default)]
    pub slack_app: Option<FlavorSlackAppConfig>,
    #[serde(default)]
    pub templates: FlavorTemplatesConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FlavorReviewPlatform {
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FlavorNotification {
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct FlavorSlackAppConfig {
    #[serde(default)]
    pub user_map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct FlavorTemplatesConfig {
    #[serde(default)]
    pub gitlab: FlavorGitLabTemplates,
    #[serde(default)]
    pub slack_app: FlavorSlackAppTemplates,
    #[serde(default)]
    pub slack_workflow: FlavorSlackWorkflowTemplates,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct FlavorGitLabTemplates {
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct FlavorSlackAppTemplates {
    #[serde(default)]
    pub first_root: Option<String>,
    #[serde(default)]
    pub first_thread: Option<String>,
    #[serde(default)]
    pub update_root: Option<String>,
    #[serde(default)]
    pub update_thread: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct FlavorSlackWorkflowTemplates {
    #[serde(default)]
    pub first_title: Option<String>,
    #[serde(default)]
    pub first_thread: Option<String>,
    #[serde(default)]
    pub update_title: Option<String>,
    #[serde(default)]
    pub update_thread: Option<String>,
}

fn default_true() -> bool {
    true
}
