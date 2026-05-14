use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::config::ObserveConfig;
use crate::context::model::CiContext;
use crate::core::domain::path_classifier::classify_path;
use crate::core::model::{AreaRisk, AreasConfig, ReviewAction, ReviewSnapshot};
use crate::core::rules::model::{RuleFinding, RuleOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserveStatus {
    Passed,
    Blocked,
    Draft,
}

impl ObserveStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Blocked => "blocked",
            Self::Draft => "draft",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptionStatus {
    Meaningful,
    Missing,
    TemplateOnly,
    NotRequired,
}

impl DescriptionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Meaningful => "meaningful",
            Self::Missing => "missing",
            Self::TemplateOnly => "template-only",
            Self::NotRequired => "not-required",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub label: String,
    pub reasons: Vec<String>,
    pub matched_areas: Vec<String>,
    pub unmatched_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptionAssessment {
    pub status: DescriptionStatus,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservePlan {
    pub status: ObserveStatus,
    pub risk: RiskAssessment,
    pub description: DescriptionAssessment,
    pub blocking_reasons: Vec<String>,
    pub actions: Vec<ReviewAction>,
}

impl ObservePlan {
    pub fn should_fail(&self) -> bool {
        matches!(self.status, ObserveStatus::Blocked)
    }

    pub fn to_outcome(&self) -> RuleOutcome {
        let mut outcome = RuleOutcome::new();
        for reason in &self.blocking_reasons {
            outcome.push(RuleFinding::blocking(reason.clone()));
        }
        for reason in &self.risk.reasons {
            outcome.push(RuleFinding::info(format!("Risk assessment: {reason}")));
        }
        for reason in &self.description.reasons {
            outcome.push(RuleFinding::info(format!(
                "Description assessment: {reason}"
            )));
        }
        for action in &self.actions {
            outcome.action_plan.push(action.clone());
        }
        outcome
    }
}

pub fn plan_observe(
    ctx: &CiContext,
    snapshot: &ReviewSnapshot,
    areas: &AreasConfig,
    config: &ObserveConfig,
) -> ObservePlan {
    let risk = assess_risk(snapshot, areas, config);
    let description = assess_description(ctx, snapshot, config);
    let mut blocking_reasons = Vec::new();

    if !snapshot.is_draft {
        for reason in &description.reasons {
            if !matches!(
                description.status,
                DescriptionStatus::Meaningful | DescriptionStatus::NotRequired
            ) {
                blocking_reasons.push(reason.clone());
            }
        }
        if !risk.unmatched_paths.is_empty() {
            blocking_reasons.push(format!(
                "Unmatched changed paths require area configuration: {}.",
                risk.unmatched_paths.join(", ")
            ));
        }
    }

    let status = if snapshot.is_draft {
        ObserveStatus::Draft
    } else if blocking_reasons.is_empty() {
        ObserveStatus::Passed
    } else {
        ObserveStatus::Blocked
    };

    let mut actions = Vec::new();
    let risk_labels = [
        config.risk.low_label.clone(),
        config.risk.medium_label.clone(),
        config.risk.high_label.clone(),
    ];
    let remove_labels = risk_labels
        .iter()
        .filter(|label| *label != &risk.label)
        .cloned()
        .collect::<Vec<_>>();
    if !remove_labels.is_empty() {
        actions.push(ReviewAction::RemoveLabels {
            labels: remove_labels,
        });
    }
    actions.push(ReviewAction::AddLabels {
        labels: vec![risk.label.clone()],
    });
    if snapshot.is_draft {
        actions.push(ReviewAction::AddLabels {
            labels: vec![config.draft_label.clone()],
        });
    }
    for reason in &blocking_reasons {
        actions.push(ReviewAction::FailPipeline {
            reason: reason.clone(),
        });
    }

    ObservePlan {
        status,
        risk,
        description,
        blocking_reasons,
        actions,
    }
}

pub fn assess_risk(
    snapshot: &ReviewSnapshot,
    areas: &AreasConfig,
    config: &ObserveConfig,
) -> RiskAssessment {
    let mut matched_areas = BTreeSet::new();
    let mut unmatched_paths = Vec::new();
    let mut reasons = Vec::new();
    let mut high_area_touched = false;
    let mut low_only = true;

    for file in &snapshot.changed_files {
        match classify_path(&file.path, areas) {
            Some(area_key) => {
                matched_areas.insert(area_key.clone());
                if let Some(area) = areas
                    .definitions
                    .iter()
                    .find(|definition| definition.key == area_key)
                {
                    if area.critical || area.risk == AreaRisk::High {
                        high_area_touched = true;
                        reasons.push(format!("{} is marked high risk.", area.key));
                    }
                    if area.risk != AreaRisk::Low {
                        low_only = false;
                    }
                }
            }
            None => unmatched_paths.push(file.path.clone()),
        }
    }

    let changed_lines = snapshot
        .changed_files
        .iter()
        .map(|file| file.additions.unwrap_or(0) + file.deletions.unwrap_or(0))
        .sum::<u32>();
    let area_count = matched_areas.len();
    let file_count = snapshot.changed_file_count();

    let level = if !unmatched_paths.is_empty() {
        reasons.push("Some changed paths do not match any configured area.".to_string());
        RiskLevel::High
    } else if high_area_touched {
        RiskLevel::High
    } else if area_count >= config.risk.high_area_count {
        reasons.push(format!("{} configured areas were touched.", area_count));
        RiskLevel::High
    } else if file_count >= config.risk.high_file_count {
        reasons.push(format!("{} files were changed.", file_count));
        RiskLevel::High
    } else if changed_lines >= config.risk.high_changed_lines {
        reasons.push(format!("{} changed lines were reported.", changed_lines));
        RiskLevel::High
    } else if area_count >= config.risk.medium_area_count {
        reasons.push(format!("{} configured areas were touched.", area_count));
        RiskLevel::Medium
    } else if changed_lines >= config.risk.medium_changed_lines {
        reasons.push(format!("{} changed lines were reported.", changed_lines));
        RiskLevel::Medium
    } else if low_only {
        reasons.push("Only low-risk configured areas were touched.".to_string());
        RiskLevel::Low
    } else {
        reasons.push("Change stayed below configured medium-risk thresholds.".to_string());
        RiskLevel::Low
    };

    let label = match level {
        RiskLevel::Low => config.risk.low_label.clone(),
        RiskLevel::Medium => config.risk.medium_label.clone(),
        RiskLevel::High => config.risk.high_label.clone(),
    };

    RiskAssessment {
        level,
        label,
        reasons,
        matched_areas: matched_areas.into_iter().collect(),
        unmatched_paths,
    }
}

pub fn assess_description(
    ctx: &CiContext,
    snapshot: &ReviewSnapshot,
    config: &ObserveConfig,
) -> DescriptionAssessment {
    if !config.description.required {
        return DescriptionAssessment {
            status: DescriptionStatus::NotRequired,
            reasons: vec!["Description requirement is disabled.".to_string()],
        };
    }

    let raw = snapshot.description.as_deref().unwrap_or_default();
    let normalized = normalize_description(raw, ctx, config);
    if normalized.is_empty() {
        return DescriptionAssessment {
            status: DescriptionStatus::Missing,
            reasons: vec![
                "The review description does not contain meaningful user text.".to_string(),
            ],
        };
    }

    for template_path in &config.description.template_paths {
        let path = Path::new(template_path);
        if !path.exists() {
            continue;
        }
        if let Ok(template) = fs::read_to_string(path) {
            if normalized == normalize_description(&template, ctx, config) {
                return DescriptionAssessment {
                    status: DescriptionStatus::TemplateOnly,
                    reasons: vec![format!(
                        "The review description still matches template '{}'.",
                        template_path
                    )],
                };
            }
        }
    }

    DescriptionAssessment {
        status: DescriptionStatus::Meaningful,
        reasons: vec!["The review description contains meaningful user text.".to_string()],
    }
}

fn normalize_description(raw: &str, ctx: &CiContext, config: &ObserveConfig) -> String {
    let issue_keys = if config.description.ignore_branch_issue_key {
        issue_keys_from_text(ctx.source_branch())
    } else {
        Vec::new()
    };
    let without_comments = strip_html_comments(raw);
    let mut content = Vec::new();

    for line in without_comments.lines() {
        let mut line = line.trim().to_string();
        for key in &issue_keys {
            line = line.replace(key, "");
        }
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("- [ ]")
            || line.starts_with("* [ ]")
            || line == "-"
            || is_placeholder(line)
        {
            continue;
        }
        content.push(line.to_ascii_lowercase());
    }

    content.join("\n")
}

fn strip_html_comments(raw: &str) -> String {
    let mut output = String::new();
    let mut rest = raw;
    loop {
        let Some(start) = rest.find("<!--") else {
            output.push_str(rest);
            return output;
        };
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 4..];
        let Some(end) = after_start.find("-->") else {
            return output;
        };
        rest = &after_start[end + 3..];
    }
}

fn is_placeholder(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "todo" | "tbd" | "n/a" | "na" | "none" | "..." | "please describe" | "add description"
    )
}

fn issue_keys_from_text(value: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for token in value
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
        .filter(|token| token.contains('-'))
    {
        let mut parts = token.split('-');
        let prefix = parts.next().unwrap_or_default();
        let number = parts.next().unwrap_or_default();
        if prefix.len() >= 2
            && prefix.chars().all(|ch| ch.is_ascii_alphabetic())
            && number.chars().all(|ch| ch.is_ascii_digit())
        {
            keys.push(format!("{}-{}", prefix.to_ascii_uppercase(), number));
            keys.push(format!("{}-{}", prefix.to_ascii_lowercase(), number));
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::model::{
        BranchInfo, BranchName, Label, PipelineInfo, PipelineSource, PipelineState, ProjectKey,
        ReviewContextRef, ReviewId,
    };
    use crate::core::model::{
        Actor, AreaDefinition, ChangeType, ChangedFile, RepositoryRef, ReviewMetadata,
        ReviewPlatformKind, ReviewRef,
    };

    fn ctx() -> CiContext {
        CiContext {
            project_key: ProjectKey("123".into()),
            review: Some(ReviewContextRef {
                id: ReviewId("456".into()),
            }),
            pipeline: PipelineInfo {
                source: PipelineSource::ReviewEvent,
                state: PipelineState::Unknown,
            },
            branches: BranchInfo {
                source: BranchName("feat/ABC-123-work".into()),
                target: BranchName("develop".into()),
            },
            labels: vec![Label("risk::low".into())],
        }
    }

    fn observe_config() -> ObserveConfig {
        ObserveConfig {
            risk: crate::config::ObserveRiskConfig {
                low_label: "risk::low".into(),
                medium_label: "risk::medium".into(),
                high_label: "risk::high".into(),
                medium_area_count: 2,
                high_area_count: 4,
                medium_changed_lines: 250,
                high_changed_lines: 800,
                high_file_count: 25,
            },
            description: crate::config::ObserveDescriptionConfig {
                required: true,
                template_paths: vec![],
                ignore_branch_issue_key: true,
            },
            draft_label: "status::draft".into(),
        }
    }

    fn snapshot(description: Option<&str>, paths: Vec<&str>) -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitLab,
                project_key: "123".into(),
                review_id: "1".into(),
                web_url: None,
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitLab,
                namespace: "group".into(),
                name: "project".into(),
                web_url: None,
            },
            title: "Test".into(),
            description: description.map(str::to_string),
            author: Actor {
                username: "arthur".into(),
                display_name: None,
            },
            participants: vec![],
            changed_files: paths
                .into_iter()
                .map(|path| ChangedFile {
                    path: path.into(),
                    previous_path: None,
                    change_type: ChangeType::Modified,
                    additions: Some(10),
                    deletions: Some(2),
                    patch: None,
                })
                .collect(),
            labels: vec![],
            is_draft: false,
            default_branch: Some("develop".into()),
            metadata: ReviewMetadata::default(),
        }
    }

    fn areas() -> AreasConfig {
        AreasConfig {
            definitions: vec![
                AreaDefinition {
                    key: "roulette".into(),
                    paths: vec!["apps/roulette/**".into()],
                    risk: AreaRisk::Low,
                    critical: false,
                },
                AreaDefinition {
                    key: "bootstrap".into(),
                    paths: vec!["packages/bootstrap/**".into()],
                    risk: AreaRisk::High,
                    critical: false,
                },
            ],
        }
    }

    #[test]
    fn jira_only_description_is_missing() {
        let result = assess_description(
            &ctx(),
            &snapshot(Some("ABC-123"), vec![]),
            &observe_config(),
        );

        assert_eq!(result.status, DescriptionStatus::Missing);
    }

    #[test]
    fn high_risk_area_is_high() {
        let result = assess_risk(
            &snapshot(
                Some("real description"),
                vec!["packages/bootstrap/src/lib.rs"],
            ),
            &areas(),
            &observe_config(),
        );

        assert_eq!(result.level, RiskLevel::High);
    }

    #[test]
    fn unmatched_path_is_high() {
        let result = assess_risk(
            &snapshot(Some("real description"), vec!["unknown/file.rs"]),
            &areas(),
            &observe_config(),
        );

        assert_eq!(result.level, RiskLevel::High);
        assert_eq!(result.unmatched_paths, vec!["unknown/file.rs".to_string()]);
    }

    #[test]
    fn unchanged_template_description_is_template_only() {
        let path =
            std::env::temp_dir().join(format!("mr-milchick-template-{}.md", std::process::id()));
        std::fs::write(&path, "## Summary\nDescribe the change\n")
            .expect("template should be written");
        let mut config = observe_config();
        config.description.template_paths = vec![path.display().to_string()];

        let result = assess_description(
            &ctx(),
            &snapshot(Some("## Summary\nDescribe the change\n"), vec![]),
            &config,
        );

        let _ = std::fs::remove_file(path);
        assert_eq!(result.status, DescriptionStatus::TemplateOnly);
    }

    #[test]
    fn draft_plan_applies_draft_label_without_blocking() {
        let mut snapshot = snapshot(None, vec!["apps/roulette/src/lib.rs"]);
        snapshot.is_draft = true;

        let plan = plan_observe(&ctx(), &snapshot, &areas(), &observe_config());

        assert_eq!(plan.status, ObserveStatus::Draft);
        assert!(!plan.should_fail());
        assert!(plan.actions.iter().any(|action| matches!(
            action,
            ReviewAction::AddLabels { labels } if labels == &vec!["status::draft".to_string()]
        )));
    }

    #[test]
    fn risk_labels_are_mutually_exclusive() {
        let plan = plan_observe(
            &ctx(),
            &snapshot(
                Some("real description"),
                vec!["packages/bootstrap/src/lib.rs"],
            ),
            &areas(),
            &observe_config(),
        );

        assert!(plan.actions.iter().any(|action| matches!(
            action,
            ReviewAction::RemoveLabels { labels }
                if labels.contains(&"risk::low".to_string())
                    && labels.contains(&"risk::medium".to_string())
        )));
        assert!(plan.actions.iter().any(|action| matches!(
            action,
            ReviewAction::AddLabels { labels } if labels == &vec!["risk::high".to_string()]
        )));
    }
}
