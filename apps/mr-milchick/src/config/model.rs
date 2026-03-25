pub use crate::core::model::{ReviewerConfig, ReviewerDefinition};
use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub reviewers: ReviewerConfig,
    pub codeowners: CodeownersConfig,
    pub slack: SlackConfig,
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FlavorConfig {
    pub review_platform: FlavorReviewPlatform,
    #[serde(default)]
    pub notifications: Vec<FlavorNotification>,
    #[serde(default)]
    pub slack_app: Option<FlavorSlackAppConfig>,
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

fn default_true() -> bool {
    true
}
