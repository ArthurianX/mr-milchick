use std::collections::BTreeMap;

use tracing::warn;

use crate::config::model::FlavorConfig;
use crate::context::model::CiContext;
use crate::core::model::{NotificationSinkKind, ReviewAction, ReviewSnapshot};
use crate::core::rules::model::{FindingSeverity, RuleOutcome};
use crate::core::tone::{ToneCategory, ToneSelector};

const GITLAB_SUMMARY_TEMPLATE: &str = r#"## {{summary_title}}

{{summary_intro}}

{{findings_block}}

{{actions_block}}

_{{closing_tone_message}}_"#;

const SLACK_APP_ROOT_TEMPLATE: &str = ":gitlab: {{notification_subject_action}} {{mr_ref_link}}, by @{{mr_author_username}}{{notification_subject_suffix}}";
const SLACK_APP_THREAD_TEMPLATE: &str = r#"*{{notification_title}}*
{{mr_line}}
{{reviewers_line}}
{{summary_intro}}
{{findings_block}}
{{actions_block}}
_{{summary_footer}}_"#;

const SLACK_WORKFLOW_TITLE_TEMPLATE: &str = ":gitlab: {{notification_subject_action}} {{mr_ref_link}}, by @{{mr_author_username}}{{notification_subject_suffix}}";
const SLACK_WORKFLOW_THREAD_TEMPLATE: &str = r#"{{notification_title}}
{{mr_line}}
{{reviewers_line}}
{{summary_intro}}
{{findings_block}}
{{actions_block}}
{{summary_footer}}"#;

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
    "reviewers_count",
    "new_reviewers_count",
    "existing_reviewers_count",
    "mr_link",
    "reviewers_list",
    "new_reviewers_list",
    "existing_reviewers_list",
    "findings_block",
    "actions_block",
    "tone_message",
    "tone_category",
    "summary_title",
    "summary_intro",
    "summary_footer",
    "notification_title",
    "notification_subject_action",
    "notification_subject_suffix",
    "mr_line",
    "reviewers_line",
    "mr_ref_link",
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
    "reviewers_count",
    "new_reviewers_count",
    "existing_reviewers_count",
    "mr_link",
    "reviewers_list",
    "new_reviewers_list",
    "existing_reviewers_list",
    "findings_block",
    "actions_block",
    "tone_message",
    "tone_category",
    "summary_title",
    "summary_intro",
    "summary_footer",
    "mr_line",
    "reviewers_line",
    "mr_ref_link",
    "closing_tone_message",
    "closing_tone_category",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateCatalog {
    pub gitlab: GitLabTemplateCatalog,
    pub slack_app: SlackAppTemplateCatalog,
    pub slack_workflow: SlackWorkflowTemplateCatalog,
}

impl Default for TemplateCatalog {
    fn default() -> Self {
        Self {
            gitlab: GitLabTemplateCatalog {
                summary: GITLAB_SUMMARY_TEMPLATE.to_string(),
            },
            slack_app: SlackAppTemplateCatalog {
                root: SLACK_APP_ROOT_TEMPLATE.to_string(),
                thread: SLACK_APP_THREAD_TEMPLATE.to_string(),
            },
            slack_workflow: SlackWorkflowTemplateCatalog {
                title: SLACK_WORKFLOW_TITLE_TEMPLATE.to_string(),
                thread: SLACK_WORKFLOW_THREAD_TEMPLATE.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabTemplateCatalog {
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackAppTemplateCatalog {
    pub root: String,
    pub thread: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackWorkflowTemplateCatalog {
    pub title: String,
    pub thread: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryTemplateContext {
    snapshot: SnapshotFacts,
    findings: Vec<FindingView>,
    actions: Vec<String>,
    tone: SelectedTone,
    closing_tone: SelectedTone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationTemplateContext {
    snapshot: SnapshotFacts,
    findings: Vec<FindingView>,
    actions: Vec<String>,
    tone: SelectedTone,
    summary_intro: String,
    summary_footer: String,
    notification_title: String,
    notification_subject_action: String,
    notification_subject_suffix: String,
    reviewers: Vec<String>,
    new_reviewers: Vec<String>,
    existing_reviewers: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedTone {
    category: ToneCategory,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemplateField {
    GitLabSummary,
    SlackAppRoot,
    SlackAppThread,
    SlackWorkflowTitle,
    SlackWorkflowThread,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderStyle {
    GitLab,
    SlackApp,
    SlackWorkflow,
}

impl TemplateField {
    fn config_path(self) -> &'static str {
        match self {
            Self::GitLabSummary => "templates.gitlab.summary",
            Self::SlackAppRoot => "templates.slack_app.root",
            Self::SlackAppThread => "templates.slack_app.thread",
            Self::SlackWorkflowTitle => "templates.slack_workflow.title",
            Self::SlackWorkflowThread => "templates.slack_workflow.thread",
        }
    }

    fn allowed_placeholders(self) -> &'static [&'static str] {
        match self {
            Self::GitLabSummary => GITLAB_SUMMARY_PLACEHOLDERS,
            Self::SlackAppRoot
            | Self::SlackAppThread
            | Self::SlackWorkflowTitle
            | Self::SlackWorkflowThread => COMMON_PLACEHOLDERS,
        }
    }
}

pub fn resolve_template_catalog(flavor: Option<&FlavorConfig>) -> TemplateCatalog {
    let mut catalog = TemplateCatalog::default();

    let Some(flavor) = flavor else {
        return catalog;
    };

    apply_template_override(
        &mut catalog.gitlab.summary,
        flavor.templates.gitlab.summary.as_deref(),
        TemplateField::GitLabSummary,
    );
    apply_template_override(
        &mut catalog.slack_app.root,
        flavor.templates.slack_app.root.as_deref(),
        TemplateField::SlackAppRoot,
    );
    apply_template_override(
        &mut catalog.slack_app.thread,
        flavor.templates.slack_app.thread.as_deref(),
        TemplateField::SlackAppThread,
    );
    apply_template_override(
        &mut catalog.slack_workflow.title,
        flavor.templates.slack_workflow.title.as_deref(),
        TemplateField::SlackWorkflowTitle,
    );
    apply_template_override(
        &mut catalog.slack_workflow.thread,
        flavor.templates.slack_workflow.thread.as_deref(),
        TemplateField::SlackWorkflowThread,
    );

    catalog
}

pub fn build_summary_template_context(
    outcome: &RuleOutcome,
    snapshot: &ReviewSnapshot,
    selector: &ToneSelector,
    ctx: &CiContext,
) -> SummaryTemplateContext {
    SummaryTemplateContext {
        snapshot: SnapshotFacts::from_snapshot(snapshot),
        findings: findings_from_outcome(outcome),
        actions: actions_from_outcome(outcome),
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

pub fn build_notification_template_context(
    outcome: &RuleOutcome,
    snapshot: &ReviewSnapshot,
    selector: &ToneSelector,
    ctx: &CiContext,
    reviewers: Vec<String>,
    new_reviewers: Vec<String>,
    existing_reviewers: Vec<String>,
) -> NotificationTemplateContext {
    let has_reviewers = !reviewers.is_empty();
    let notification_tone_category = if has_reviewers {
        ToneCategory::ReviewRequest
    } else {
        ToneCategory::Observation
    };
    let notification_title = if has_reviewers {
        "Mr. Milchick Review Summary".to_string()
    } else {
        selector.select(ToneCategory::Observation, ctx).to_string()
    };
    let summary_footer_category = summary_closing_category(outcome);

    NotificationTemplateContext {
        snapshot: SnapshotFacts::from_snapshot(snapshot),
        findings: findings_from_outcome(outcome),
        actions: actions_from_outcome(outcome),
        tone: SelectedTone {
            category: notification_tone_category,
            message: selector.select(notification_tone_category, ctx).to_string(),
        },
        summary_intro: selector.select(ToneCategory::Observation, ctx).to_string(),
        summary_footer: selector.select(summary_footer_category, ctx).to_string(),
        notification_title,
        notification_subject_action: if has_reviewers {
            "Reviews Needed for".to_string()
        } else {
            "Mr. Milchick updated".to_string()
        },
        notification_subject_suffix: if has_reviewers {
            " :pepe-review:".to_string()
        } else {
            String::new()
        },
        reviewers,
        new_reviewers,
        existing_reviewers,
    }
}

pub fn render_gitlab_summary(
    catalog: &TemplateCatalog,
    context: &SummaryTemplateContext,
) -> String {
    render_template(
        &catalog.gitlab.summary,
        &context.variables(RenderStyle::GitLab, true),
    )
}

pub fn render_slack_app_notification(
    catalog: &TemplateCatalog,
    context: &NotificationTemplateContext,
) -> (String, String) {
    (
        render_template(
            &catalog.slack_app.root,
            &context.variables(RenderStyle::SlackApp),
        ),
        render_template(
            &catalog.slack_app.thread,
            &context.variables(RenderStyle::SlackApp),
        ),
    )
}

pub fn render_slack_workflow_notification(
    catalog: &TemplateCatalog,
    context: &NotificationTemplateContext,
) -> (String, String) {
    (
        render_template(
            &catalog.slack_workflow.title,
            &context.variables(RenderStyle::SlackWorkflow),
        ),
        render_template(
            &catalog.slack_workflow.thread,
            &context.variables(RenderStyle::SlackWorkflow),
        ),
    )
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
            &[],
            &[],
            &[],
            style,
            &self.tone,
        );

        values.insert("summary_title", "Mr. Milchick Review Summary".to_string());
        values.insert("summary_intro", self.tone.message.clone());
        values.insert("summary_footer", self.closing_tone.message.clone());
        values.insert("notification_title", String::new());
        values.insert("notification_subject_action", String::new());
        values.insert("notification_subject_suffix", String::new());
        values.insert("mr_line", format!("Merge request: {}", values["mr_link"]));
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
            &self.reviewers,
            &self.new_reviewers,
            &self.existing_reviewers,
            style,
            &self.tone,
        );

        values.insert("summary_title", "Mr. Milchick Review Summary".to_string());
        values.insert("summary_intro", self.summary_intro.clone());
        values.insert("summary_footer", self.summary_footer.clone());
        values.insert("notification_title", self.notification_title.clone());
        values.insert(
            "notification_subject_action",
            self.notification_subject_action.clone(),
        );
        values.insert(
            "notification_subject_suffix",
            self.notification_subject_suffix.clone(),
        );
        values.insert("mr_line", format!("Merge request: {}", values["mr_link"]));
        values.insert(
            "reviewers_line",
            if self.reviewers.is_empty() {
                String::new()
            } else {
                match style {
                    RenderStyle::GitLab => {
                        format!("Assign reviewers: {}", values["reviewers_list"])
                    }
                    RenderStyle::SlackApp => {
                        format!("_Assign reviewers_ {}", values["reviewers_list"])
                    }
                    RenderStyle::SlackWorkflow => {
                        format!("Assign reviewers {}", values["reviewers_list"])
                    }
                }
            },
        );
        values.insert("mr_ref_link", ref_link(style, &self.snapshot));

        values
    }
}

fn common_values(
    snapshot: &SnapshotFacts,
    findings: &[FindingView],
    actions: &[String],
    reviewers: &[String],
    new_reviewers: &[String],
    existing_reviewers: &[String],
    style: RenderStyle,
    tone: &SelectedTone,
) -> BTreeMap<&'static str, String> {
    let mut values = BTreeMap::new();
    values.insert("mr_number", snapshot.mr_number.clone());
    values.insert("mr_ref", snapshot.mr_ref.clone());
    values.insert("mr_title", snapshot.mr_title.clone());
    values.insert("mr_url", snapshot.mr_url.clone());
    values.insert("mr_author_username", snapshot.mr_author_username.clone());
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
    values.insert("reviewers_count", reviewers.len().to_string());
    values.insert("new_reviewers_count", new_reviewers.len().to_string());
    values.insert(
        "existing_reviewers_count",
        existing_reviewers.len().to_string(),
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
            "Assign reviewers: {}",
            reviewers
                .iter()
                .map(|reviewer| format!("@{}", reviewer.username))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        ReviewAction::UpsertSummary { .. } => None,
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
            RenderStyle::GitLab => format!("- **{}**: {}", finding.label, finding.message),
            RenderStyle::SlackApp => format!("*{}*: {}", finding.label, finding.message),
            RenderStyle::SlackWorkflow => format!("{}: {}", finding.label, finding.message),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_actions_block(style: RenderStyle, actions: &[String]) -> String {
    actions
        .iter()
        .map(|action| match style {
            RenderStyle::GitLab | RenderStyle::SlackWorkflow => format!("- {}", action),
            RenderStyle::SlackApp => format!("• {}", action),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_reviewers_list(style: RenderStyle, reviewers: &[String]) -> String {
    match style {
        RenderStyle::GitLab => reviewers
            .iter()
            .map(|reviewer| format!("@{}", reviewer))
            .collect::<Vec<_>>()
            .join(", "),
        RenderStyle::SlackApp => reviewers
            .iter()
            .map(|reviewer| format!("*@{}*", reviewer))
            .collect::<Vec<_>>()
            .join(" "),
        RenderStyle::SlackWorkflow => reviewers
            .iter()
            .map(|reviewer| format!("@{}", reviewer))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn message_link(style: RenderStyle, url: &str, label: &str) -> String {
    if url.is_empty() {
        return label.to_string();
    }

    match style {
        RenderStyle::GitLab => format!("[{}]({})", label, url),
        RenderStyle::SlackApp => format!("<{}|{}>", url, label),
        RenderStyle::SlackWorkflow => format!("{} ({})", label, url),
    }
}

fn ref_link(style: RenderStyle, snapshot: &SnapshotFacts) -> String {
    message_link(style, &snapshot.mr_url, &snapshot.mr_ref)
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
    }
}

pub fn enabled_notification_targets(sinks: &[NotificationSinkKind]) -> Vec<NotificationSinkKind> {
    sinks.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{
        FlavorConfig, FlavorGitLabTemplates, FlavorNotification, FlavorReviewPlatform,
        FlavorSlackAppConfig, FlavorSlackAppTemplates, FlavorSlackWorkflowTemplates,
        FlavorTemplatesConfig,
    };
    use crate::core::actions::model::ActionPlan;
    use crate::core::model::{Actor, ReviewMetadata};
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
                change_type: crate::core::model::ChangeType::Modified,
                additions: None,
                deletions: None,
            }],
            labels: Vec::new(),
            is_draft: false,
            default_branch: Some("develop".to_string()),
            metadata: ReviewMetadata {
                source_branch: Some("feat/buttons".to_string()),
                target_branch: Some("develop".to_string()),
                commit_count: None,
                approvals_required: None,
                approvals_given: None,
            },
        }
    }

    fn sample_context() -> CiContext {
        CiContext {
            project_id: crate::context::model::ProjectId("123".to_string()),
            merge_request: Some(crate::context::model::MergeRequestRef {
                iid: crate::context::model::MergeRequestIid("456".to_string()),
            }),
            pipeline: crate::context::model::PipelineInfo {
                source: crate::context::model::PipelineSource::MergeRequestEvent,
            },
            branches: crate::context::model::BranchInfo {
                source: crate::context::model::BranchName("feat/buttons".to_string()),
                target: crate::context::model::BranchName("develop".to_string()),
            },
            labels: Vec::new(),
        }
    }

    #[test]
    fn validates_unknown_placeholder() {
        let error = validate_template("{{unknown_placeholder}}", TemplateField::SlackAppRoot)
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

        let rendered = render_gitlab_summary(
            &TemplateCatalog::default(),
            &build_summary_template_context(
                &outcome,
                &sample_snapshot(),
                &ToneSelector::default(),
                &sample_context(),
            ),
        );

        assert!(rendered.contains("Mr. Milchick Review Summary"));
        assert!(rendered.contains("Warning"));
        assert!(rendered.contains("Assign reviewers: @bob"));
    }

    #[test]
    fn renders_notification_context_placeholders() {
        let outcome = RuleOutcome::new();
        let (subject, body) = render_slack_workflow_notification(
            &TemplateCatalog::default(),
            &build_notification_template_context(
                &outcome,
                &sample_snapshot(),
                &ToneSelector::default(),
                &sample_context(),
                vec!["principal-reviewer".to_string(), "bob".to_string()],
                vec!["bob".to_string()],
                vec!["principal-reviewer".to_string()],
            ),
        );

        assert!(subject.contains("Reviews Needed for"));
        assert!(body.contains("Assign reviewers @principal-reviewer @bob"));
        assert!(body.contains("No findings were produced."));
    }

    #[test]
    fn uses_partial_template_override_without_affecting_other_fields() {
        let flavor = FlavorConfig {
            review_platform: FlavorReviewPlatform {
                kind: "gitlab".to_string(),
            },
            notification_policy: None,
            notifications: vec![FlavorNotification {
                kind: "slack-app".to_string(),
                enabled: true,
            }],
            slack_app: Some(FlavorSlackAppConfig::default()),
            templates: FlavorTemplatesConfig {
                gitlab: FlavorGitLabTemplates::default(),
                slack_app: FlavorSlackAppTemplates {
                    root: Some("custom root for {{mr_ref}}".to_string()),
                    thread: None,
                },
                slack_workflow: FlavorSlackWorkflowTemplates::default(),
            },
        };

        let catalog = resolve_template_catalog(Some(&flavor));

        assert_eq!(catalog.slack_app.root, "custom root for {{mr_ref}}");
        assert_eq!(catalog.slack_app.thread, SLACK_APP_THREAD_TEMPLATE);
    }

    #[test]
    fn falls_back_to_default_when_override_is_invalid() {
        let flavor = FlavorConfig {
            review_platform: FlavorReviewPlatform {
                kind: "gitlab".to_string(),
            },
            notification_policy: None,
            notifications: Vec::new(),
            slack_app: None,
            templates: FlavorTemplatesConfig {
                gitlab: FlavorGitLabTemplates::default(),
                slack_app: FlavorSlackAppTemplates {
                    root: Some("custom {{unknown_placeholder}}".to_string()),
                    thread: None,
                },
                slack_workflow: FlavorSlackWorkflowTemplates::default(),
            },
        };

        let catalog = resolve_template_catalog(Some(&flavor));

        assert_eq!(catalog.slack_app.root, SLACK_APP_ROOT_TEMPLATE);
    }
}
