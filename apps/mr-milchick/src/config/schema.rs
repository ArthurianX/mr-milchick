use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationPolicy {
    Always,
    OnAppliedAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum PlatformKind {
    #[serde(rename = "gitlab")]
    GitLab,
    #[serde(rename = "github")]
    GitHub,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    #[serde(default)]
    pub platform: PlatformConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub reviewers: ReviewersConfig,
    #[serde(default)]
    pub codeowners: CodeownersConfig,
    #[serde(default)]
    pub inference: InferenceConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub templates: TemplatesConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PlatformConfig {
    #[serde(default)]
    pub kind: Option<PlatformKind>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub gitlab: GitLabPlatformConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GitLabPlatformConfig {
    #[serde(default)]
    pub all_pipelines_pass_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ExecutionConfig {
    #[serde(default)]
    pub dry_run: Option<bool>,
    #[serde(default)]
    pub notification_policy: Option<NotificationPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ReviewersConfig {
    #[serde(default)]
    pub max_reviewers: Option<usize>,
    #[serde(default)]
    pub definitions: Vec<ReviewerDefinitionConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewerDefinitionConfig {
    pub username: String,
    #[serde(default)]
    pub areas: Vec<String>,
    #[serde(default)]
    pub fallback: bool,
    #[serde(default)]
    pub mandatory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CodeownersConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct InferenceConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub model_path: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub max_patch_bytes: Option<usize>,
    #[serde(default)]
    pub context_tokens: Option<usize>,
    #[serde(default)]
    pub trace: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct NotificationsConfig {
    #[serde(default)]
    pub slack_app: SlackAppConfig,
    #[serde(default)]
    pub slack_workflow: SlackWorkflowConfig,
    #[serde(default)]
    pub pipeline_status: PipelineStatusConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SlackAppConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub user_map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SlackWorkflowConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub channel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PipelineStatusConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub fail_pipeline_on_failed: Option<bool>,
    #[serde(default)]
    pub search_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TemplatesConfig {
    #[serde(default)]
    pub gitlab: GitLabTemplates,
    #[serde(default)]
    pub github: GitHubTemplates,
    #[serde(default)]
    pub slack_app: SlackAppTemplates,
    #[serde(default)]
    pub slack_workflow: SlackWorkflowTemplates,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GitLabTemplates {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub explain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GitHubTemplates {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub explain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SlackAppTemplates {
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
#[serde(deny_unknown_fields)]
pub struct SlackWorkflowTemplates {
    #[serde(default)]
    pub first_title: Option<String>,
    #[serde(default)]
    pub first_thread: Option<String>,
    #[serde(default)]
    pub update_title: Option<String>,
    #[serde(default)]
    pub update_thread: Option<String>,
}
