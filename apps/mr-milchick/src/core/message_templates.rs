use std::collections::BTreeMap;

use tracing::warn;

use crate::config::TemplatesConfig;
use crate::context::model::CiContext;
use crate::core::inference::{RecommendationCategory, ReviewInferenceOutcome};
use crate::core::model::{NotificationSinkKind, ReviewAction, ReviewPlatformKind, ReviewSnapshot};
use crate::core::observe::{ObservePlan, ObserveStatus, RiskLevel};
use crate::core::rules::model::{FindingSeverity, RuleOutcome};
use crate::core::tone::{ToneCategory, ToneSelector};

const GITLAB_SUMMARY_TEMPLATE: &str = r#"## {{summary_title}}

{{summary_intro}}

{{findings_block}}

{{actions_block}}

_{{closing_tone_message}}_"#;
const GITLAB_EXPLAIN_TEMPLATE: &str = r#"## {{summary_title}}

{{summary_intro}}

{{findings_block}}

{{actions_block}}

{{recommendations_block}}

_{{closing_tone_message}}_"#;
const GITHUB_SUMMARY_TEMPLATE: &str = GITLAB_SUMMARY_TEMPLATE;
const GITHUB_EXPLAIN_TEMPLATE: &str = GITLAB_EXPLAIN_TEMPLATE;

const SLACK_APP_FIRST_ROOT_TEMPLATE: &str = "{{notification_subject}}";
const SLACK_APP_THREAD_ROOT_TEMPLATE: &str =
    "Mr. Milchick is observing {{thread_key}}: {{mr_title}}";
const SLACK_APP_EXPLAIN_THREAD_TEMPLATE: &str = r#"*{{summary_title}}*
Merge request: {{mr_link}}

{{summary_intro}}

{{findings_block}}

{{actions_block}}

{{recommendations_block}}

_{{closing_tone_message}}_"#;
const SLACK_APP_OBSERVE_THREAD_TEMPLATE: &str = r#"*{{notification_title}}*
Review: {{mr_link}}
Status: {{observe_status}}
Risk: {{risk_level}}{{risk_emoji_suffix}}
{{risk_reasons_block}}
{{blocking_reasons_block}}
_{{next_step}}_"#;
const SLACK_APP_OBSERVE_BLOCKED_THREAD_TEMPLATE: &str = r#"*{{notification_title}}*
Review: {{mr_link}}
Status: {{observe_status}}
Risk: {{risk_level}}{{risk_emoji_suffix}}
Description: {{description_status}}
{{blocking_reasons_block}}
_{{next_step}}_"#;
const SLACK_APP_OBSERVE_DRAFT_THREAD_TEMPLATE: &str = r#"*{{notification_title}}*
Review: {{mr_link}}
Status: {{observe_status}}
Risk: {{risk_level}}{{risk_emoji_suffix}}
_{{next_step}}_"#;
const SLACK_APP_REFINE_THREAD_TEMPLATE: &str = r#"*{{notification_title}}*
Merge request: {{mr_link}}
{{reviewers_line}}
{{findings_block}}
{{actions_block}}
{{pipeline_status_block}}
_{{summary_footer}}_"#;
const SLACK_APP_FIRST_THREAD_TEMPLATE: &str = r#"*{{notification_title}}*
Merge request: {{mr_link}}
{{reviewers_line}}
{{pipeline_status_block}}"#;
const SLACK_APP_UPDATE_ROOT_TEMPLATE: &str = "{{notification_subject}}";
const SLACK_APP_UPDATE_THREAD_TEMPLATE: &str = r#"Merge request: {{mr_link}}
{{findings_block}}
{{actions_block}}
{{pipeline_status_block}}
_{{summary_footer}}_"#;

const COMMON_PLACEHOLDERS: &[&str] = &[
    "mr_number",
    "mr_ref",
    "mr_title",
    "mr_url",
    "mr_author_username",
    "source_branch",
    "target_branch",
    "is_draft",
    "changed_file_count",
    "findings_count",
    "blocking_findings_count",
    "warning_findings_count",
    "info_findings_count",
    "actions_count",
    "recommendations_count",
    "reviewers_count",
    "new_reviewers_count",
    "existing_reviewers_count",
    "pipeline_status_count",
    "pipeline_status_passed_count",
    "pipeline_status_failed_count",
    "pipeline_status_unknown_count",
    "mr_link",
    "reviewers_list",
    "new_reviewers_list",
    "existing_reviewers_list",
    "findings_block",
    "actions_block",
    "pipeline_status_block",
    "llm_summary",
    "recommendations_block",
    "tone_message",
    "tone_category",
    "summary_title",
    "summary_intro",
    "summary_footer",
    "notification_title",
    "notification_subject",
    "reviewers_line",
    "mr_ref_link",
    "thread_key",
    "observe_status",
    "risk_level",
    "risk_label",
    "risk_emoji",
    "risk_emoji_suffix",
    "risk_reasons_block",
    "matched_areas_list",
    "unmatched_paths_block",
    "description_status",
    "blocking_reasons_block",
    "next_step",
];

const GITLAB_SUMMARY_PLACEHOLDERS: &[&str] = &[
    "mr_number",
    "mr_ref",
    "mr_title",
    "mr_url",
    "mr_author_username",
    "source_branch",
    "target_branch",
    "is_draft",
    "changed_file_count",
    "findings_count",
    "blocking_findings_count",
    "warning_findings_count",
    "info_findings_count",
    "actions_count",
    "recommendations_count",
    "reviewers_count",
    "new_reviewers_count",
    "existing_reviewers_count",
    "mr_link",
    "reviewers_list",
    "new_reviewers_list",
    "existing_reviewers_list",
    "findings_block",
    "actions_block",
    "llm_summary",
    "recommendations_block",
    "tone_message",
    "tone_category",
    "summary_title",
    "summary_intro",
    "summary_footer",
    "reviewers_line",
    "mr_ref_link",
    "closing_tone_message",
    "closing_tone_category",
];
const GITHUB_SUMMARY_PLACEHOLDERS: &[&str] = GITLAB_SUMMARY_PLACEHOLDERS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateCatalog {
    pub gitlab: GitLabTemplateCatalog,
    pub github: GitHubTemplateCatalog,
    pub slack_app: SlackAppTemplateCatalog,
    pub risk_emojis: RiskEmojiTemplateCatalog,
}

impl Default for TemplateCatalog {
    fn default() -> Self {
        Self {
            gitlab: GitLabTemplateCatalog {
                summary: GITLAB_SUMMARY_TEMPLATE.to_string(),
                explain: GITLAB_EXPLAIN_TEMPLATE.to_string(),
            },
            github: GitHubTemplateCatalog {
                summary: GITHUB_SUMMARY_TEMPLATE.to_string(),
                explain: GITHUB_EXPLAIN_TEMPLATE.to_string(),
            },
            slack_app: SlackAppTemplateCatalog {
                thread_root: SLACK_APP_THREAD_ROOT_TEMPLATE.to_string(),
                explain_thread: SLACK_APP_EXPLAIN_THREAD_TEMPLATE.to_string(),
                observe_thread: SLACK_APP_OBSERVE_THREAD_TEMPLATE.to_string(),
                observe_blocked_thread: SLACK_APP_OBSERVE_BLOCKED_THREAD_TEMPLATE.to_string(),
                observe_draft_thread: SLACK_APP_OBSERVE_DRAFT_THREAD_TEMPLATE.to_string(),
                refine_thread: SLACK_APP_REFINE_THREAD_TEMPLATE.to_string(),
                first_root: SLACK_APP_FIRST_ROOT_TEMPLATE.to_string(),
                first_thread: SLACK_APP_FIRST_THREAD_TEMPLATE.to_string(),
                update_root: SLACK_APP_UPDATE_ROOT_TEMPLATE.to_string(),
                update_thread: SLACK_APP_UPDATE_THREAD_TEMPLATE.to_string(),
            },
            risk_emojis: RiskEmojiTemplateCatalog::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RiskEmojiTemplateCatalog {
    pub low: String,
    pub medium: String,
    pub high: String,
}

impl RiskEmojiTemplateCatalog {
    fn emoji_for(&self, level: RiskLevel) -> &str {
        match level {
            RiskLevel::Low => &self.low,
            RiskLevel::Medium => &self.medium,
            RiskLevel::High => &self.high,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabTemplateCatalog {
    pub summary: String,
    pub explain: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubTemplateCatalog {
    pub summary: String,
    pub explain: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackAppTemplateCatalog {
    pub thread_root: String,
    pub explain_thread: String,
    pub observe_thread: String,
    pub observe_blocked_thread: String,
    pub observe_draft_thread: String,
    pub refine_thread: String,
    pub first_root: String,
    pub first_thread: String,
    pub update_root: String,
    pub update_thread: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryTemplateContext {
    summary_title: String,
    snapshot: SnapshotFacts,
    findings: Vec<FindingView>,
    actions: Vec<String>,
    inference: InferenceTemplateContext,
    tone: SelectedTone,
    closing_tone: SelectedTone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationTemplateContext {
    snapshot: SnapshotFacts,
    findings: Vec<FindingView>,
    actions: Vec<String>,
    inference: InferenceTemplateContext,
    tone: SelectedTone,
    summary_intro: String,
    summary_footer: String,
    notification_title: String,
    notification_subject: String,
    reviewers: Vec<String>,
    new_reviewers: Vec<String>,
    existing_reviewers: Vec<String>,
    pipeline_statuses: Vec<PipelineStatusTemplateEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveTemplateContext {
    snapshot: SnapshotFacts,
    tone: SelectedTone,
    notification_title: String,
    notification_subject: String,
    thread_key: String,
    observe_status: String,
    risk_level: String,
    risk: RiskLevel,
    risk_label: String,
    risk_reasons: Vec<String>,
    matched_areas: Vec<String>,
    unmatched_paths: Vec<String>,
    description_status: String,
    blocking_reasons: Vec<String>,
    next_step: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotFacts {
    mr_number: String,
    mr_ref: String,
    mr_title: String,
    mr_url: String,
    mr_author_username: String,
    source_branch: String,
    target_branch: String,
    is_draft: bool,
    changed_file_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FindingView {
    label: String,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct InferenceTemplateContext {
    summary: Option<String>,
    recommendations: Vec<RecommendationView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecommendationView {
    label: String,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStatusState {
    Passed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineStatusTemplateEntry {
    pub label: String,
    pub state: PipelineStatusState,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedTone {
    category: ToneCategory,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationTemplateVariant {
    First,
    Update,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemplateField {
    GitLabSummary,
    GitLabExplain,
    GitHubSummary,
    GitHubExplain,
    SlackAppThreadRoot,
    SlackAppExplainThread,
    SlackAppObserveThread,
    SlackAppObserveBlockedThread,
    SlackAppObserveDraftThread,
    SlackAppRefineThread,
    SlackAppFirstRoot,
    SlackAppFirstThread,
    SlackAppUpdateRoot,
    SlackAppUpdateThread,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderStyle {
    GitLab,
    GitHub,
    SlackApp,
}

impl TemplateField {
    fn config_path(self) -> &'static str {
        match self {
            Self::GitLabSummary => "templates.gitlab.summary",
            Self::GitLabExplain => "templates.gitlab.explain",
            Self::GitHubSummary => "templates.github.summary",
            Self::GitHubExplain => "templates.github.explain",
            Self::SlackAppThreadRoot => "templates.slack_app.thread_root",
            Self::SlackAppExplainThread => "templates.slack_app.explain_thread",
            Self::SlackAppObserveThread => "templates.slack_app.observe_thread",
            Self::SlackAppObserveBlockedThread => "templates.slack_app.observe_blocked_thread",
            Self::SlackAppObserveDraftThread => "templates.slack_app.observe_draft_thread",
            Self::SlackAppRefineThread => "templates.slack_app.refine_thread",
            Self::SlackAppFirstRoot => "templates.slack_app.first_root",
            Self::SlackAppFirstThread => "templates.slack_app.first_thread",
            Self::SlackAppUpdateRoot => "templates.slack_app.update_root",
            Self::SlackAppUpdateThread => "templates.slack_app.update_thread",
        }
    }

    fn allowed_placeholders(self) -> &'static [&'static str] {
        match self {
            Self::GitLabSummary => GITLAB_SUMMARY_PLACEHOLDERS,
            Self::GitLabExplain => GITLAB_SUMMARY_PLACEHOLDERS,
            Self::GitHubSummary => GITHUB_SUMMARY_PLACEHOLDERS,
            Self::GitHubExplain => GITHUB_SUMMARY_PLACEHOLDERS,
            Self::SlackAppThreadRoot
            | Self::SlackAppExplainThread
            | Self::SlackAppObserveThread
            | Self::SlackAppObserveBlockedThread
            | Self::SlackAppObserveDraftThread
            | Self::SlackAppRefineThread
            | Self::SlackAppFirstRoot
            | Self::SlackAppFirstThread
            | Self::SlackAppUpdateRoot
            | Self::SlackAppUpdateThread => COMMON_PLACEHOLDERS,
        }
    }
}

pub fn resolve_template_catalog(templates: &TemplatesConfig) -> TemplateCatalog {
    let mut catalog = TemplateCatalog::default();
    catalog.risk_emojis = RiskEmojiTemplateCatalog {
        low: templates.risk_emojis.low.clone().unwrap_or_default(),
        medium: templates.risk_emojis.medium.clone().unwrap_or_default(),
        high: templates.risk_emojis.high.clone().unwrap_or_default(),
    };

    apply_template_override(
        &mut catalog.gitlab.summary,
        templates.gitlab.summary.as_deref(),
        TemplateField::GitLabSummary,
    );
    apply_template_override(
        &mut catalog.gitlab.explain,
        templates.gitlab.explain.as_deref(),
        TemplateField::GitLabExplain,
    );
    apply_template_override(
        &mut catalog.github.summary,
        templates.github.summary.as_deref(),
        TemplateField::GitHubSummary,
    );
    apply_template_override(
        &mut catalog.github.explain,
        templates.github.explain.as_deref(),
        TemplateField::GitHubExplain,
    );
    apply_template_override(
        &mut catalog.slack_app.thread_root,
        templates.slack_app.thread_root.as_deref(),
        TemplateField::SlackAppThreadRoot,
    );
    apply_template_override(
        &mut catalog.slack_app.explain_thread,
        templates.slack_app.explain_thread.as_deref(),
        TemplateField::SlackAppExplainThread,
    );
    apply_template_override(
        &mut catalog.slack_app.observe_thread,
        templates.slack_app.observe_thread.as_deref(),
        TemplateField::SlackAppObserveThread,
    );
    apply_template_override(
        &mut catalog.slack_app.observe_blocked_thread,
        templates.slack_app.observe_blocked_thread.as_deref(),
        TemplateField::SlackAppObserveBlockedThread,
    );
    apply_template_override(
        &mut catalog.slack_app.observe_draft_thread,
        templates.slack_app.observe_draft_thread.as_deref(),
        TemplateField::SlackAppObserveDraftThread,
    );
    apply_template_override(
        &mut catalog.slack_app.refine_thread,
        templates.slack_app.refine_thread.as_deref(),
        TemplateField::SlackAppRefineThread,
    );
    apply_template_override(
        &mut catalog.slack_app.first_root,
        templates.slack_app.first_root.as_deref(),
        TemplateField::SlackAppFirstRoot,
    );
    apply_template_override(
        &mut catalog.slack_app.first_thread,
        templates.slack_app.first_thread.as_deref(),
        TemplateField::SlackAppFirstThread,
    );
    apply_template_override(
        &mut catalog.slack_app.update_root,
        templates.slack_app.update_root.as_deref(),
        TemplateField::SlackAppUpdateRoot,
    );
    apply_template_override(
        &mut catalog.slack_app.update_thread,
        templates.slack_app.update_thread.as_deref(),
        TemplateField::SlackAppUpdateThread,
    );
    catalog
}

pub fn build_summary_template_context(
    outcome: &RuleOutcome,
    snapshot: &ReviewSnapshot,
    selector: &ToneSelector,
    ctx: &CiContext,
) -> SummaryTemplateContext {
    build_review_comment_template_context(
        "Mr. Milchick Review Summary",
        outcome,
        snapshot,
        &ReviewInferenceOutcome::ready(Default::default()),
        selector,
        ctx,
    )
}

pub fn build_explain_template_context(
    outcome: &RuleOutcome,
    snapshot: &ReviewSnapshot,
    inference_outcome: &ReviewInferenceOutcome,
    selector: &ToneSelector,
    ctx: &CiContext,
) -> SummaryTemplateContext {
    build_review_comment_template_context(
        "Mr. Milchick Advisory Explain",
        outcome,
        snapshot,
        inference_outcome,
        selector,
        ctx,
    )
}

pub fn build_notification_template_context(
    outcome: &RuleOutcome,
    snapshot: &ReviewSnapshot,
    selector: &ToneSelector,
    ctx: &CiContext,
    variant: NotificationTemplateVariant,
    reviewers: Vec<String>,
    new_reviewers: Vec<String>,
    existing_reviewers: Vec<String>,
    pipeline_statuses: Vec<PipelineStatusTemplateEntry>,
) -> NotificationTemplateContext {
    let snapshot_facts = SnapshotFacts::from_snapshot(snapshot);
    let notification_tone_category = if matches!(variant, NotificationTemplateVariant::First) {
        ToneCategory::ReviewRequest
    } else {
        ToneCategory::Observation
    };
    let summary_footer_category = summary_closing_category(outcome);

    NotificationTemplateContext {
        snapshot: snapshot_facts.clone(),
        findings: findings_from_outcome(outcome),
        actions: actions_from_outcome(outcome),
        inference: InferenceTemplateContext::default(),
        tone: SelectedTone {
            category: notification_tone_category,
            message: selector.select(notification_tone_category, ctx).to_string(),
        },
        summary_intro: selector.select(ToneCategory::Observation, ctx).to_string(),
        summary_footer: selector.select(summary_footer_category, ctx).to_string(),
        notification_title: selector.select(notification_tone_category, ctx).to_string(),
        notification_subject: build_notification_subject(
            variant,
            RenderStyle::SlackApp,
            &snapshot_facts,
            &snapshot.author.username,
        ),
        reviewers,
        new_reviewers,
        existing_reviewers,
        pipeline_statuses,
    }
}

pub fn build_observe_template_context(
    plan: &ObservePlan,
    snapshot: &ReviewSnapshot,
    selector: &ToneSelector,
    ctx: &CiContext,
) -> ObserveTemplateContext {
    let snapshot_facts = SnapshotFacts::from_snapshot(snapshot);
    let tone_category = match plan.status {
        ObserveStatus::Passed => ToneCategory::ObservePassed,
        ObserveStatus::Blocked => ToneCategory::ObserveBlocked,
        ObserveStatus::Draft => ToneCategory::ObserveDraft,
    };
    let thread_key = format!("MR #{}", snapshot.review_ref.review_id);

    ObserveTemplateContext {
        notification_subject: format!(
            "Mr. Milchick is observing {}",
            ref_link(RenderStyle::SlackApp, &snapshot_facts)
        ),
        notification_title: selector.select(tone_category, ctx).to_string(),
        snapshot: snapshot_facts,
        tone: SelectedTone {
            category: tone_category,
            message: selector.select(tone_category, ctx).to_string(),
        },
        thread_key,
        observe_status: plan.status.as_str().to_string(),
        risk_level: plan.risk.level.as_str().to_string(),
        risk: plan.risk.level,
        risk_label: plan.risk.label.clone(),
        risk_reasons: plan.risk.reasons.clone(),
        matched_areas: plan.risk.matched_areas.clone(),
        unmatched_paths: plan.risk.unmatched_paths.clone(),
        description_status: plan.description.status.as_str().to_string(),
        blocking_reasons: plan.blocking_reasons.clone(),
        next_step: match plan.status {
            ObserveStatus::Passed => selector.select(ToneCategory::RefineReady, ctx).to_string(),
            ObserveStatus::Blocked => selector
                .select(ToneCategory::PipelinePaused, ctx)
                .to_string(),
            ObserveStatus::Draft => {
                "Draft review observed; full governance is underway.".to_string()
            }
        },
    }
}

fn build_review_comment_template_context(
    summary_title: impl Into<String>,
    outcome: &RuleOutcome,
    snapshot: &ReviewSnapshot,
    inference_outcome: &ReviewInferenceOutcome,
    selector: &ToneSelector,
    ctx: &CiContext,
) -> SummaryTemplateContext {
    SummaryTemplateContext {
        summary_title: summary_title.into(),
        snapshot: SnapshotFacts::from_snapshot(snapshot),
        findings: findings_from_outcome(outcome),
        actions: actions_from_outcome(outcome),
        inference: inference_from_outcome(inference_outcome),
        tone: SelectedTone {
            category: ToneCategory::Observation,
            message: selector.select(ToneCategory::Observation, ctx).to_string(),
        },
        closing_tone: SelectedTone {
            category: summary_closing_category(outcome),
            message: selector
                .select(summary_closing_category(outcome), ctx)
                .to_string(),
        },
    }
}

pub fn render_review_summary(
    catalog: &TemplateCatalog,
    context: &SummaryTemplateContext,
    platform: ReviewPlatformKind,
) -> String {
    let (template, style) = match platform {
        ReviewPlatformKind::GitLab => (&catalog.gitlab.summary, RenderStyle::GitLab),
        ReviewPlatformKind::GitHub => (&catalog.github.summary, RenderStyle::GitHub),
    };

    render_template(template, &context.variables(style, true))
}

pub fn render_review_explain(
    catalog: &TemplateCatalog,
    context: &SummaryTemplateContext,
    platform: ReviewPlatformKind,
) -> String {
    let (template, style) = match platform {
        ReviewPlatformKind::GitLab => (&catalog.gitlab.explain, RenderStyle::GitLab),
        ReviewPlatformKind::GitHub => (&catalog.github.explain, RenderStyle::GitHub),
    };

    render_template(template, &context.variables(style, true))
}

pub fn render_slack_app_explain(
    catalog: &TemplateCatalog,
    context: &SummaryTemplateContext,
) -> String {
    render_template(
        &catalog.slack_app.explain_thread,
        &context.variables(RenderStyle::SlackApp, true),
    )
}

pub fn render_slack_app_notification(
    catalog: &TemplateCatalog,
    context: &NotificationTemplateContext,
    variant: NotificationTemplateVariant,
) -> (String, String) {
    render_notification(catalog, context, NotificationSinkKind::SlackApp, variant)
        .expect("Slack app notifications should be renderable")
}

pub fn render_slack_app_thread_root(
    catalog: &TemplateCatalog,
    context: &ObserveTemplateContext,
) -> String {
    render_template(
        &catalog.slack_app.thread_root,
        &context.variables(RenderStyle::SlackApp, &catalog.risk_emojis),
    )
}

pub fn render_slack_app_observe_notification(
    catalog: &TemplateCatalog,
    context: &ObserveTemplateContext,
) -> String {
    render_observe_notification(catalog, context, NotificationSinkKind::SlackApp)
        .expect("Slack app observe notifications should be renderable")
        .1
}

pub fn render_notification(
    catalog: &TemplateCatalog,
    context: &NotificationTemplateContext,
    sink: NotificationSinkKind,
    variant: NotificationTemplateVariant,
) -> Option<(String, String)> {
    let _ = variant;
    match sink {
        NotificationSinkKind::SlackApp => Some((
            render_template(
                &catalog.slack_app.thread_root,
                &context.variables(RenderStyle::SlackApp),
            ),
            render_template(
                &catalog.slack_app.refine_thread,
                &context.variables(RenderStyle::SlackApp),
            ),
        )),
        NotificationSinkKind::Teams | NotificationSinkKind::Discord => None,
    }
}

pub fn render_observe_notification(
    catalog: &TemplateCatalog,
    context: &ObserveTemplateContext,
    sink: NotificationSinkKind,
) -> Option<(String, String)> {
    match sink {
        NotificationSinkKind::SlackApp => Some((
            render_template(
                &catalog.slack_app.thread_root,
                &context.variables(RenderStyle::SlackApp, &catalog.risk_emojis),
            ),
            render_template(
                select_observe_template(
                    context,
                    &catalog.slack_app.observe_thread,
                    &catalog.slack_app.observe_blocked_thread,
                    &catalog.slack_app.observe_draft_thread,
                ),
                &context.variables(RenderStyle::SlackApp, &catalog.risk_emojis),
            ),
        )),
        NotificationSinkKind::Teams | NotificationSinkKind::Discord => None,
    }
}

fn select_observe_template<'a>(
    context: &ObserveTemplateContext,
    default: &'a str,
    blocked: &'a str,
    draft: &'a str,
) -> &'a str {
    if context.observe_status == "blocked" {
        blocked
    } else if context.snapshot.is_draft {
        draft
    } else {
        default
    }
}

fn apply_template_override(
    target: &mut String,
    override_value: Option<&str>,
    field: TemplateField,
) {
    let Some(override_value) = override_value else {
        return;
    };

    match validate_template(override_value, field) {
        Ok(()) => *target = override_value.to_string(),
        Err(err) => warn!(
            "ignoring invalid template override '{}': {}",
            field.config_path(),
            err
        ),
    }
}

fn validate_template(template: &str, field: TemplateField) -> Result<(), String> {
    for placeholder in extract_placeholders(template)? {
        if !field.allowed_placeholders().contains(&placeholder.as_str()) {
            return Err(format!("unknown placeholder '{{{{{}}}}}'", placeholder));
        }
    }

    Ok(())
}

fn extract_placeholders(template: &str) -> Result<Vec<String>, String> {
    let chars = template.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut placeholders = Vec::new();

    while index < chars.len() {
        if chars[index] == '{' && chars.get(index + 1) == Some(&'{') {
            let start = index + 2;
            let mut end = start;
            while end + 1 < chars.len() && !(chars[end] == '}' && chars[end + 1] == '}') {
                end += 1;
            }

            if end + 1 >= chars.len() {
                return Err("unclosed placeholder".to_string());
            }

            let placeholder = chars[start..end]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            if placeholder.is_empty() {
                return Err("empty placeholder".to_string());
            }

            if !placeholder
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
            {
                return Err(format!("invalid placeholder '{{{{{}}}}}'", placeholder));
            }

            placeholders.push(placeholder);
            index = end + 2;
            continue;
        }

        index += 1;
    }

    Ok(placeholders)
}

fn render_template(template: &str, values: &BTreeMap<&'static str, String>) -> String {
    let chars = template.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut output = String::with_capacity(template.len());

    while index < chars.len() {
        if chars[index] == '{' && chars.get(index + 1) == Some(&'{') {
            let start = index + 2;
            let mut end = start;
            while end + 1 < chars.len() && !(chars[end] == '}' && chars[end + 1] == '}') {
                end += 1;
            }

            if end + 1 >= chars.len() {
                output.push(chars[index]);
                index += 1;
                continue;
            }

            let placeholder = chars[start..end].iter().collect::<String>();
            let placeholder = placeholder.trim();
            output.push_str(
                values
                    .get(placeholder)
                    .map(String::as_str)
                    .unwrap_or_default(),
            );
            index = end + 2;
            continue;
        }

        output.push(chars[index]);
        index += 1;
    }

    output
}

impl SnapshotFacts {
    fn from_snapshot(snapshot: &ReviewSnapshot) -> Self {
        Self {
            mr_number: snapshot.review_ref.review_id.clone(),
            mr_ref: format!("MR #{}", snapshot.review_ref.review_id),
            mr_title: snapshot.title.clone(),
            mr_url: snapshot.review_ref.web_url.clone().unwrap_or_default(),
            mr_author_username: snapshot.author.username.clone(),
            source_branch: snapshot.metadata.source_branch.clone().unwrap_or_default(),
            target_branch: snapshot.metadata.target_branch.clone().unwrap_or_default(),
            is_draft: snapshot.is_draft,
            changed_file_count: snapshot.changed_file_count(),
        }
    }
}

impl SummaryTemplateContext {
    fn variables(
        &self,
        style: RenderStyle,
        include_closing_tone: bool,
    ) -> BTreeMap<&'static str, String> {
        let mut values = common_values(
            &self.snapshot,
            &self.findings,
            &self.actions,
            &self.inference,
            &[],
            &[],
            &[],
            &[],
            style,
            &self.tone,
        );

        values.insert("summary_title", self.summary_title.clone());
        values.insert("summary_intro", self.tone.message.clone());
        values.insert("summary_footer", self.closing_tone.message.clone());
        values.insert("notification_title", String::new());
        values.insert("notification_subject", String::new());
        values.insert("reviewers_line", String::new());
        values.insert("mr_ref_link", ref_link(style, &self.snapshot));

        if include_closing_tone {
            values.insert("closing_tone_message", self.closing_tone.message.clone());
            values.insert(
                "closing_tone_category",
                tone_category_name(self.closing_tone.category).to_string(),
            );
        }

        values
    }
}

impl NotificationTemplateContext {
    fn variables(&self, style: RenderStyle) -> BTreeMap<&'static str, String> {
        let mut values = common_values(
            &self.snapshot,
            &self.findings,
            &self.actions,
            &self.inference,
            &self.reviewers,
            &self.new_reviewers,
            &self.existing_reviewers,
            &self.pipeline_statuses,
            style,
            &self.tone,
        );

        values.insert("summary_title", "Mr. Milchick Review Summary".to_string());
        values.insert("summary_intro", self.summary_intro.clone());
        values.insert("summary_footer", self.summary_footer.clone());
        values.insert("notification_title", self.notification_title.clone());
        values.insert(
            "notification_subject",
            match style {
                RenderStyle::SlackApp => self.notification_subject.clone(),
                RenderStyle::GitLab | RenderStyle::GitHub => String::new(),
            },
        );
        values.insert(
            "reviewers_line",
            if self.reviewers.is_empty() {
                String::new()
            } else {
                match style {
                    RenderStyle::GitLab | RenderStyle::GitHub => {
                        format!("Assigned reviewers: {}", values["reviewers_list"])
                    }
                    RenderStyle::SlackApp => {
                        format!("_Assigned reviewers_ {}", values["reviewers_list"])
                    }
                }
            },
        );
        values.insert("mr_ref_link", ref_link(style, &self.snapshot));
        values.insert("thread_key", format!("MR #{}", self.snapshot.mr_number));

        values
    }
}

impl ObserveTemplateContext {
    fn variables(
        &self,
        style: RenderStyle,
        risk_emojis: &RiskEmojiTemplateCatalog,
    ) -> BTreeMap<&'static str, String> {
        let mut values = common_values(
            &self.snapshot,
            &[],
            &[],
            &InferenceTemplateContext::default(),
            &[],
            &[],
            &[],
            &[],
            style,
            &self.tone,
        );

        values.insert("summary_title", String::new());
        values.insert("summary_intro", self.tone.message.clone());
        values.insert("summary_footer", self.next_step.clone());
        values.insert("notification_title", self.notification_title.clone());
        values.insert(
            "notification_subject",
            match style {
                RenderStyle::SlackApp => self.notification_subject.clone(),
                RenderStyle::GitLab | RenderStyle::GitHub => String::new(),
            },
        );
        values.insert("reviewers_line", String::new());
        values.insert("mr_ref_link", ref_link(style, &self.snapshot));
        values.insert("thread_key", self.thread_key.clone());
        values.insert("observe_status", self.observe_status.clone());
        values.insert("risk_level", self.risk_level.clone());
        values.insert("risk_label", self.risk_label.clone());
        let risk_emoji = risk_emojis.emoji_for(self.risk).to_string();
        values.insert("risk_emoji", risk_emoji.clone());
        values.insert("risk_emoji_suffix", emoji_suffix(&risk_emoji));
        values.insert(
            "risk_reasons_block",
            format_plain_block(style, "Risk reasons", &self.risk_reasons),
        );
        values.insert("matched_areas_list", self.matched_areas.join(", "));
        values.insert(
            "unmatched_paths_block",
            format_plain_block(style, "Unmatched paths", &self.unmatched_paths),
        );
        values.insert("description_status", self.description_status.clone());
        values.insert(
            "blocking_reasons_block",
            format_plain_block(style, "Blocking reasons", &self.blocking_reasons),
        );
        values.insert("next_step", self.next_step.clone());

        values
    }
}

fn emoji_suffix(emoji: &str) -> String {
    let trimmed = emoji.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!(" {trimmed}")
    }
}

fn common_values(
    snapshot: &SnapshotFacts,
    findings: &[FindingView],
    actions: &[String],
    inference: &InferenceTemplateContext,
    reviewers: &[String],
    new_reviewers: &[String],
    existing_reviewers: &[String],
    pipeline_statuses: &[PipelineStatusTemplateEntry],
    style: RenderStyle,
    tone: &SelectedTone,
) -> BTreeMap<&'static str, String> {
    let mut values = BTreeMap::new();
    values.insert("mr_number", snapshot.mr_number.clone());
    values.insert("mr_ref", snapshot.mr_ref.clone());
    values.insert("mr_title", snapshot.mr_title.clone());
    values.insert("mr_url", snapshot.mr_url.clone());
    values.insert(
        "mr_author_username",
        match style {
            RenderStyle::SlackApp => format!("@{}", snapshot.mr_author_username),
            RenderStyle::GitLab | RenderStyle::GitHub => snapshot.mr_author_username.clone(),
        },
    );
    values.insert("source_branch", snapshot.source_branch.clone());
    values.insert("target_branch", snapshot.target_branch.clone());
    values.insert("is_draft", snapshot.is_draft.to_string());
    values.insert(
        "changed_file_count",
        snapshot.changed_file_count.to_string(),
    );
    values.insert("findings_count", findings.len().to_string());
    values.insert(
        "blocking_findings_count",
        findings
            .iter()
            .filter(|finding| finding.label == "Blocking")
            .count()
            .to_string(),
    );
    values.insert(
        "warning_findings_count",
        findings
            .iter()
            .filter(|finding| finding.label == "Warning")
            .count()
            .to_string(),
    );
    values.insert(
        "info_findings_count",
        findings
            .iter()
            .filter(|finding| finding.label == "Info")
            .count()
            .to_string(),
    );
    values.insert("actions_count", actions.len().to_string());
    values.insert(
        "recommendations_count",
        inference.recommendations.len().to_string(),
    );
    values.insert("reviewers_count", reviewers.len().to_string());
    values.insert("new_reviewers_count", new_reviewers.len().to_string());
    values.insert(
        "existing_reviewers_count",
        existing_reviewers.len().to_string(),
    );
    values.insert("pipeline_status_count", pipeline_statuses.len().to_string());
    values.insert(
        "pipeline_status_passed_count",
        pipeline_statuses
            .iter()
            .filter(|entry| matches!(entry.state, PipelineStatusState::Passed))
            .count()
            .to_string(),
    );
    values.insert(
        "pipeline_status_failed_count",
        pipeline_statuses
            .iter()
            .filter(|entry| matches!(entry.state, PipelineStatusState::Failed))
            .count()
            .to_string(),
    );
    values.insert(
        "pipeline_status_unknown_count",
        pipeline_statuses
            .iter()
            .filter(|entry| matches!(entry.state, PipelineStatusState::Unknown))
            .count()
            .to_string(),
    );
    values.insert(
        "mr_link",
        message_link(style, &snapshot.mr_url, &snapshot.mr_title),
    );
    values.insert("reviewers_list", format_reviewers_list(style, reviewers));
    values.insert(
        "new_reviewers_list",
        format_reviewers_list(style, new_reviewers),
    );
    values.insert(
        "existing_reviewers_list",
        format_reviewers_list(style, existing_reviewers),
    );
    values.insert("findings_block", format_findings_block(style, findings));
    values.insert("actions_block", format_actions_block(style, actions));
    values.insert(
        "pipeline_status_block",
        format_pipeline_status_block(style, pipeline_statuses),
    );
    values.insert("llm_summary", inference.summary.clone().unwrap_or_default());
    values.insert(
        "recommendations_block",
        format_recommendations_block(style, inference),
    );
    values.insert("tone_message", tone.message.clone());
    values.insert(
        "tone_category",
        tone_category_name(tone.category).to_string(),
    );

    values
}

fn findings_from_outcome(outcome: &RuleOutcome) -> Vec<FindingView> {
    outcome
        .findings
        .iter()
        .map(|finding| FindingView {
            label: finding_label(&finding.severity),
            message: finding.message.clone(),
        })
        .collect()
}

fn actions_from_outcome(outcome: &RuleOutcome) -> Vec<String> {
    let actions = outcome
        .action_plan
        .actions
        .iter()
        .filter_map(describe_action)
        .collect::<Vec<_>>();

    if actions.is_empty() {
        vec!["None".to_string()]
    } else {
        actions
    }
}

fn inference_from_outcome(outcome: &ReviewInferenceOutcome) -> InferenceTemplateContext {
    InferenceTemplateContext {
        summary: outcome.insights.summary.clone(),
        recommendations: outcome
            .insights
            .recommendations
            .iter()
            .map(|recommendation| RecommendationView {
                label: recommendation_label(recommendation.category).to_string(),
                message: recommendation.message.clone(),
            })
            .collect(),
    }
}

fn finding_label(severity: &FindingSeverity) -> String {
    match severity {
        FindingSeverity::Info => "Info".to_string(),
        FindingSeverity::Warning => "Warning".to_string(),
        FindingSeverity::Blocking => "Blocking".to_string(),
    }
}

fn describe_action(action: &ReviewAction) -> Option<String> {
    match action {
        ReviewAction::AssignReviewers { reviewers } => Some(format!(
            "Assigned reviewers: {}",
            reviewers
                .iter()
                .map(|reviewer| format!("@{}", reviewer.username))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        ReviewAction::UpsertSummary { .. } | ReviewAction::UpsertExplain { .. } => None,
        ReviewAction::AddLabels { labels } => Some(format!("Add labels: {}", labels.join(", "))),
        ReviewAction::RemoveLabels { labels } => {
            Some(format!("Remove labels: {}", labels.join(", ")))
        }
        ReviewAction::FailPipeline { reason } => Some(format!("Fail pipeline: {}", reason)),
    }
}

fn summary_closing_category(outcome: &RuleOutcome) -> ToneCategory {
    if outcome.has_blocking_findings() || outcome.action_plan.has_fail_pipeline() {
        ToneCategory::Blocking
    } else if outcome.is_empty() && outcome.action_plan.is_empty() {
        ToneCategory::NoAction
    } else if outcome.findings.is_empty() {
        ToneCategory::Praise
    } else {
        ToneCategory::Refinement
    }
}

fn format_findings_block(style: RenderStyle, findings: &[FindingView]) -> String {
    if findings.is_empty() {
        return "No findings were produced.".to_string();
    }

    findings
        .iter()
        .map(|finding| match style {
            RenderStyle::GitLab | RenderStyle::GitHub => {
                format!("- **{}**: {}", finding.label, finding.message)
            }
            RenderStyle::SlackApp => format!("*{}*: {}", finding.label, finding.message),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_plain_block(style: RenderStyle, title: &str, values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }

    match style {
        RenderStyle::SlackApp => format!(
            "*{}:*\n{}",
            title,
            values
                .iter()
                .map(|value| format!("• {}", value))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        RenderStyle::GitLab | RenderStyle::GitHub => format!(
            "{}:\n{}",
            title,
            values
                .iter()
                .map(|value| format!("- {}", value))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

fn format_actions_block(style: RenderStyle, actions: &[String]) -> String {
    actions
        .iter()
        .map(|action| match style {
            RenderStyle::GitLab | RenderStyle::GitHub => {
                format!("- {}", action)
            }
            RenderStyle::SlackApp => format!("• {}", action),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_recommendations_block(
    style: RenderStyle,
    inference: &InferenceTemplateContext,
) -> String {
    if inference.summary.is_none() && inference.recommendations.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    match style {
        RenderStyle::GitLab | RenderStyle::GitHub => {
            lines.push("### AI review recommendations".to_string())
        }
        RenderStyle::SlackApp => lines.push("*AI review recommendations*".to_string()),
    }

    if let Some(summary) = &inference.summary {
        lines.push(summary.clone());
    }

    lines.extend(
        inference
            .recommendations
            .iter()
            .map(|recommendation| match style {
                RenderStyle::GitLab | RenderStyle::GitHub => {
                    format!("- **{}**: {}", recommendation.label, recommendation.message)
                }
                RenderStyle::SlackApp => {
                    format!("• *{}*: {}", recommendation.label, recommendation.message)
                }
            }),
    );

    lines.join("\n")
}

fn format_pipeline_status_block(
    style: RenderStyle,
    pipeline_statuses: &[PipelineStatusTemplateEntry],
) -> String {
    if pipeline_statuses.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    match style {
        RenderStyle::GitLab | RenderStyle::GitHub => lines.push("### CI task status".to_string()),
        RenderStyle::SlackApp => lines.push("*CI task status*".to_string()),
    }

    lines.extend(pipeline_statuses.iter().map(|entry| {
        let indicator = match entry.state {
            PipelineStatusState::Passed => ":large_green_circle:",
            PipelineStatusState::Failed => ":red_circle:",
            PipelineStatusState::Unknown => ":large_yellow_circle:",
        };
        let mut line = match style {
            RenderStyle::GitLab | RenderStyle::GitHub => {
                format!("- {} {}", indicator, entry.label)
            }
            RenderStyle::SlackApp => format!("{} {}", indicator, entry.label),
        };

        if let Some(detail) = entry.detail.as_deref().filter(|detail| !detail.is_empty()) {
            line.push_str(": ");
            line.push_str(detail);
        }

        line
    }));

    lines.join("\n")
}

fn format_reviewers_list(style: RenderStyle, reviewers: &[String]) -> String {
    match style {
        RenderStyle::GitLab | RenderStyle::GitHub => reviewers
            .iter()
            .map(|reviewer| format!("@{}", reviewer))
            .collect::<Vec<_>>()
            .join(", "),
        RenderStyle::SlackApp => reviewers
            .iter()
            .map(|reviewer| format!("*@{}*", reviewer))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn recommendation_label(category: RecommendationCategory) -> &'static str {
    match category {
        RecommendationCategory::ReviewFocus => "Review focus",
        RecommendationCategory::Risk => "Risk",
        RecommendationCategory::TestGap => "Test gap",
    }
}

fn message_link(style: RenderStyle, url: &str, label: &str) -> String {
    if url.is_empty() {
        return label.to_string();
    }

    match style {
        RenderStyle::GitLab | RenderStyle::GitHub => format!("[{}]({})", label, url),
        RenderStyle::SlackApp => format!("<{}|{}>", url, label),
    }
}

fn ref_link(style: RenderStyle, snapshot: &SnapshotFacts) -> String {
    message_link(style, &snapshot.mr_url, &snapshot.mr_ref)
}

pub fn notification_template_variant(reviewers: &[String]) -> NotificationTemplateVariant {
    if reviewers.is_empty() {
        NotificationTemplateVariant::Update
    } else {
        NotificationTemplateVariant::First
    }
}

fn build_notification_subject(
    variant: NotificationTemplateVariant,
    style: RenderStyle,
    snapshot: &SnapshotFacts,
    author_username: &str,
) -> String {
    match variant {
        NotificationTemplateVariant::First => format!(
            "Mr. Milchick took a first look at {}, by @{}",
            ref_link(style, snapshot),
            author_username
        ),
        NotificationTemplateVariant::Update => {
            format!("Mr. Milchick - updates on {}", ref_link(style, snapshot))
        }
    }
}

fn tone_category_name(category: ToneCategory) -> &'static str {
    match category {
        ToneCategory::Observation => "Observation",
        ToneCategory::Refinement => "Refinement",
        ToneCategory::Resolution => "Resolution",
        ToneCategory::Blocking => "Blocking",
        ToneCategory::Praise => "Praise",
        ToneCategory::ReviewRequest => "ReviewRequest",
        ToneCategory::NoAction => "NoAction",
        ToneCategory::ReviewerAssigned => "ReviewerAssigned",
        ToneCategory::ObserveOpened => "ObserveOpened",
        ToneCategory::ObservePassed => "ObservePassed",
        ToneCategory::ObserveBlocked => "ObserveBlocked",
        ToneCategory::ObserveDraft => "ObserveDraft",
        ToneCategory::RiskLow => "RiskLow",
        ToneCategory::RiskMedium => "RiskMedium",
        ToneCategory::RiskHigh => "RiskHigh",
        ToneCategory::DescriptionMissing => "DescriptionMissing",
        ToneCategory::DescriptionTemplateOnly => "DescriptionTemplateOnly",
        ToneCategory::PipelinePaused => "PipelinePaused",
        ToneCategory::RefineReady => "RefineReady",
    }
}

pub fn enabled_notification_targets(sinks: &[NotificationSinkKind]) -> Vec<NotificationSinkKind> {
    sinks.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        GitHubTemplates, GitLabTemplates, RiskEmojiTemplates, SlackAppTemplates, TemplatesConfig,
    };
    use crate::core::actions::model::ActionPlan;
    use crate::core::inference::{
        RecommendationCategory, ReviewInferenceOutcome, ReviewInsights, ReviewRecommendation,
    };
    use crate::core::model::{Actor, ReviewMetadata};
    use crate::core::observe::{DescriptionAssessment, DescriptionStatus, RiskAssessment};
    use crate::core::rules::model::{RuleFinding, RuleOutcome};

    fn sample_snapshot() -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: crate::core::model::ReviewRef {
                platform: crate::core::model::ReviewPlatformKind::GitLab,
                project_key: "123".to_string(),
                review_id: "456".to_string(),
                web_url: Some(
                    "https://gitlab.example.com/group/project/-/merge_requests/456".to_string(),
                ),
            },
            repository: crate::core::model::RepositoryRef {
                platform: crate::core::model::ReviewPlatformKind::GitLab,
                namespace: "group".to_string(),
                name: "project".to_string(),
                web_url: Some("https://gitlab.example.com/group/project".to_string()),
            },
            title: "Frontend adjustments".to_string(),
            description: None,
            author: Actor {
                username: "arthur".to_string(),
                display_name: None,
            },
            participants: vec![Actor {
                username: "principal-reviewer".to_string(),
                display_name: None,
            }],
            changed_files: vec![crate::core::model::ChangedFile {
                path: "apps/frontend/button.tsx".to_string(),
                previous_path: None,
                change_type: crate::core::model::ChangeType::Modified,
                additions: None,
                deletions: None,
                patch: None,
            }],
            labels: Vec::new(),
            is_draft: false,
            default_branch: Some("develop".to_string()),
            metadata: ReviewMetadata {
                source_branch: Some("feat/buttons".to_string()),
                target_branch: Some("develop".to_string()),
                merge_request_state: Some("opened".to_string()),
                commit_count: None,
                commits: Vec::new(),
                approvals_required: None,
                approvals_given: None,
            },
        }
    }

    fn sample_context() -> CiContext {
        CiContext {
            project_key: crate::context::model::ProjectKey("123".to_string()),
            review: Some(crate::context::model::ReviewContextRef {
                id: crate::context::model::ReviewId("456".to_string()),
            }),
            pipeline: crate::context::model::PipelineInfo {
                source: crate::context::model::PipelineSource::ReviewEvent,
                state: crate::context::model::PipelineState::Unknown,
            },
            branches: crate::context::model::BranchInfo {
                source: crate::context::model::BranchName("feat/buttons".to_string()),
                target: crate::context::model::BranchName("develop".to_string()),
            },
            labels: Vec::new(),
        }
    }

    fn sample_inference_outcome() -> ReviewInferenceOutcome {
        ReviewInferenceOutcome::ready(ReviewInsights {
            summary: Some("The change mostly affects reviewer routing and notification copy.".to_string()),
            recommendations: vec![
                ReviewRecommendation {
                    category: RecommendationCategory::ReviewFocus,
                    message: "Check that first-time reviewer notifications still fire only when new reviewers are added.".to_string(),
                },
                ReviewRecommendation {
                    category: RecommendationCategory::TestGap,
                    message: "Add coverage for advisory inference text appearing in summary and Slack output.".to_string(),
                },
            ],
        })
    }

    fn sample_observe_plan(level: RiskLevel) -> ObservePlan {
        let label = match level {
            RiskLevel::Low => "risk::low",
            RiskLevel::Medium => "risk::medium",
            RiskLevel::High => "risk::high",
        };

        ObservePlan {
            status: ObserveStatus::Passed,
            risk: RiskAssessment {
                level,
                label: label.to_string(),
                reasons: vec!["Risk reason.".to_string()],
                matched_areas: vec!["frontend".to_string()],
                unmatched_paths: Vec::new(),
            },
            description: DescriptionAssessment {
                status: DescriptionStatus::Meaningful,
                reasons: vec!["Description is meaningful.".to_string()],
            },
            blocking_reasons: Vec::new(),
            actions: Vec::new(),
        }
    }

    #[test]
    fn validates_unknown_placeholder() {
        let error = validate_template("{{unknown_placeholder}}", TemplateField::SlackAppFirstRoot)
            .expect_err("template should fail");

        assert!(error.contains("unknown placeholder"));
    }

    #[test]
    fn renders_default_gitlab_summary_template() {
        let mut outcome = RuleOutcome {
            findings: vec![RuleFinding::warning("Tidy this up.")],
            action_plan: ActionPlan::new(),
        };
        outcome.action_plan.push(ReviewAction::AssignReviewers {
            reviewers: vec![Actor {
                username: "bob".to_string(),
                display_name: None,
            }],
        });

        let rendered = render_review_summary(
            &TemplateCatalog::default(),
            &build_summary_template_context(
                &outcome,
                &sample_snapshot(),
                &ToneSelector::default(),
                &sample_context(),
            ),
            ReviewPlatformKind::GitLab,
        );

        assert!(rendered.contains("Mr. Milchick Review Summary"));
        assert!(rendered.contains("Warning"));
        assert!(rendered.contains("Assigned reviewers: @bob"));
        assert!(!rendered.contains("Local review recommendations"));
    }

    #[test]
    fn renders_default_gitlab_explain_template() {
        let outcome = RuleOutcome::new();

        let rendered = render_review_explain(
            &TemplateCatalog::default(),
            &build_explain_template_context(
                &outcome,
                &sample_snapshot(),
                &sample_inference_outcome(),
                &ToneSelector::default(),
                &sample_context(),
            ),
            ReviewPlatformKind::GitLab,
        );

        assert!(rendered.contains("Mr. Milchick Advisory Explain"));
        assert!(rendered.contains("AI review recommendations"));
        assert!(!rendered.contains("Local review recommendations"));
        assert!(rendered.contains("Check that first-time reviewer notifications"));
    }

    #[test]
    fn renders_default_slack_explain_template() {
        let rendered = render_slack_app_explain(
            &TemplateCatalog::default(),
            &build_explain_template_context(
                &RuleOutcome::new(),
                &sample_snapshot(),
                &sample_inference_outcome(),
                &ToneSelector::default(),
                &sample_context(),
            ),
        );

        assert!(rendered.starts_with("*Mr. Milchick Advisory Explain*"));
        assert!(rendered.contains("AI review recommendations"));
        assert!(rendered.contains("Check that first-time reviewer notifications"));
        assert!(!rendered.contains("## Mr. Milchick Advisory Explain"));
    }

    #[test]
    fn applies_slack_explain_thread_template_override() {
        let catalog = resolve_template_catalog(&TemplatesConfig {
            slack_app: SlackAppTemplates {
                explain_thread: Some("*{{summary_title}}* {{recommendations_count}}".to_string()),
                ..SlackAppTemplates::default()
            },
            ..TemplatesConfig::default()
        });

        let rendered = render_slack_app_explain(
            &catalog,
            &build_explain_template_context(
                &RuleOutcome::new(),
                &sample_snapshot(),
                &sample_inference_outcome(),
                &ToneSelector::default(),
                &sample_context(),
            ),
        );

        assert_eq!(rendered, "*Mr. Milchick Advisory Explain* 2");
    }

    #[test]
    fn renders_configured_risk_emoji_in_slack_observe_template() {
        let catalog = resolve_template_catalog(&TemplatesConfig {
            risk_emojis: RiskEmojiTemplates {
                high: Some(":alert:".to_string()),
                medium: Some(":large_yellow_circle:".to_string()),
                low: Some(":white_circle:".to_string()),
            },
            ..TemplatesConfig::default()
        });

        for (level, expected) in [
            (RiskLevel::High, "Risk: high :alert:"),
            (RiskLevel::Medium, "Risk: medium :large_yellow_circle:"),
            (RiskLevel::Low, "Risk: low :white_circle:"),
        ] {
            let body = render_slack_app_observe_notification(
                &catalog,
                &build_observe_template_context(
                    &sample_observe_plan(level),
                    &sample_snapshot(),
                    &ToneSelector::default(),
                    &sample_context(),
                ),
            );

            assert!(body.contains(expected));
        }
    }

    #[test]
    fn default_risk_emoji_is_empty() {
        let body = render_slack_app_observe_notification(
            &TemplateCatalog::default(),
            &build_observe_template_context(
                &sample_observe_plan(RiskLevel::High),
                &sample_snapshot(),
                &ToneSelector::default(),
                &sample_context(),
            ),
        );

        assert!(body.contains("Risk: high\n"));
        assert!(!body.contains(":alert:"));
    }

    #[test]
    fn renders_draft_template_for_a_passing_draft_observe_plan() {
        let mut snapshot = sample_snapshot();
        snapshot.is_draft = true;
        let mut catalog = TemplateCatalog::default();
        catalog.slack_app.observe_thread = "PASSED TEMPLATE".to_string();
        catalog.slack_app.observe_draft_thread = "DRAFT TEMPLATE {{is_draft}}".to_string();

        let body = render_slack_app_observe_notification(
            &catalog,
            &build_observe_template_context(
                &sample_observe_plan(RiskLevel::Low),
                &snapshot,
                &ToneSelector::default(),
                &sample_context(),
            ),
        );

        assert_eq!(body, "DRAFT TEMPLATE true");
    }

    #[test]
    fn renders_notification_context_placeholders() {
        let outcome = RuleOutcome::new();
        let (subject, body) = render_slack_app_notification(
            &TemplateCatalog::default(),
            &build_notification_template_context(
                &outcome,
                &sample_snapshot(),
                &ToneSelector::default(),
                &sample_context(),
                NotificationTemplateVariant::First,
                vec!["principal-reviewer".to_string(), "bob".to_string()],
                vec!["bob".to_string()],
                vec!["principal-reviewer".to_string()],
                vec![PipelineStatusTemplateEntry {
                    label: "unit_tests".to_string(),
                    state: PipelineStatusState::Passed,
                    detail: Some("12 tests passed".to_string()),
                }],
            ),
            NotificationTemplateVariant::First,
        );

        assert!(subject.contains("Mr. Milchick is observing"));
        assert!(body.contains("_Assigned reviewers_ *@principal-reviewer* *@bob*"));
        assert!(body.contains("CI task status"));
        assert!(body.contains(":large_green_circle: unit_tests: 12 tests passed"));
        assert!(!body.contains("Local review recommendations"));
    }

    #[test]
    fn slack_app_author_username_placeholder_renders_as_mention() {
        let outcome = RuleOutcome::new();
        let context = build_notification_template_context(
            &outcome,
            &sample_snapshot(),
            &ToneSelector::default(),
            &sample_context(),
            NotificationTemplateVariant::Update,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let mut catalog = TemplateCatalog::default();
        catalog.slack_app.refine_thread = "Author {{mr_author_username}}".to_string();

        let (_, app_body) = render_notification(
            &catalog,
            &context,
            NotificationSinkKind::SlackApp,
            NotificationTemplateVariant::Update,
        )
        .expect("Slack app notification should render");
        assert_eq!(app_body, "Author @arthur");
    }

    #[test]
    fn uses_partial_template_override_without_affecting_other_fields() {
        let templates = TemplatesConfig {
            risk_emojis: RiskEmojiTemplates::default(),
            gitlab: GitLabTemplates::default(),
            github: GitHubTemplates::default(),
            slack_app: SlackAppTemplates {
                first_root: Some("custom root for {{mr_ref}}".to_string()),
                ..SlackAppTemplates::default()
            },
        };

        let catalog = resolve_template_catalog(&templates);

        assert_eq!(catalog.slack_app.first_root, "custom root for {{mr_ref}}");
        assert_eq!(
            catalog.slack_app.update_thread,
            SLACK_APP_UPDATE_THREAD_TEMPLATE
        );
    }

    #[test]
    fn falls_back_to_default_when_override_is_invalid() {
        let templates = TemplatesConfig {
            risk_emojis: RiskEmojiTemplates::default(),
            gitlab: GitLabTemplates::default(),
            github: GitHubTemplates::default(),
            slack_app: SlackAppTemplates {
                first_root: Some("custom {{unknown_placeholder}}".to_string()),
                ..SlackAppTemplates::default()
            },
        };

        let catalog = resolve_template_catalog(&templates);

        assert_eq!(catalog.slack_app.first_root, SLACK_APP_FIRST_ROOT_TEMPLATE);
    }
}
