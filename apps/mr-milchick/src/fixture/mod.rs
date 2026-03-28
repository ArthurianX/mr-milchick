use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::context::model::{
    BranchInfo, BranchName, CiContext, Label, MergeRequestIid, MergeRequestRef, PipelineInfo,
    PipelineSource, ProjectId,
};
use crate::core::actions::model::ActionPlan;
use crate::core::message_templates::NotificationTemplateVariant;
use crate::core::model::{
    Actor, ChangeType, ChangedFile, RepositoryRef, ReviewAction, ReviewMetadata,
    ReviewPlatformKind, ReviewRef, ReviewSnapshot,
};
use crate::core::rules::model::{FindingSeverity, RuleFinding, RuleOutcome};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReviewFixture {
    pub project_id: String,
    pub merge_request_iid: String,
    #[serde(default = "default_pipeline_source")]
    pub pipeline_source: String,
    #[serde(default)]
    pub notification_variant: Option<FixtureNotificationVariant>,
    #[serde(rename = "merge_request")]
    pub merge_request: FixtureMergeRequest,
    #[serde(default)]
    pub findings: Vec<FixtureFinding>,
    #[serde(default)]
    pub actions: Vec<FixtureAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FixtureNotificationVariant {
    First,
    Update,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FixtureMergeRequest {
    pub title: String,
    pub url: String,
    pub author: String,
    #[serde(default)]
    pub description: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub existing_reviewers: Vec<String>,
    #[serde(default)]
    pub changed_files: Vec<FixtureChangedFile>,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FixtureChangedFile {
    pub path: String,
    #[serde(default = "default_change_type")]
    pub change_type: String,
    #[serde(default)]
    pub additions: Option<u32>,
    #[serde(default)]
    pub deletions: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FixtureFinding {
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FixtureAction {
    pub kind: String,
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

pub fn load_review_fixture(path: impl AsRef<std::path::Path>) -> Result<ReviewFixture> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture '{}'", path.display()))?;
    toml::from_str::<ReviewFixture>(&raw)
        .with_context(|| format!("failed to parse fixture '{}'", path.display()))
}

impl ReviewFixture {
    pub fn notification_template_variant(&self) -> Option<NotificationTemplateVariant> {
        self.notification_variant.map(|variant| match variant {
            FixtureNotificationVariant::First => NotificationTemplateVariant::First,
            FixtureNotificationVariant::Update => NotificationTemplateVariant::Update,
        })
    }

    pub fn to_ci_context(&self) -> Result<CiContext> {
        Ok(CiContext {
            project_id: ProjectId(self.project_id.trim().to_string()),
            merge_request: Some(MergeRequestRef {
                iid: MergeRequestIid(self.merge_request_iid.trim().to_string()),
            }),
            pipeline: PipelineInfo {
                source: parse_pipeline_source(&self.pipeline_source)?,
            },
            branches: BranchInfo {
                source: BranchName(self.merge_request.source_branch.clone()),
                target: BranchName(self.merge_request.target_branch.clone()),
            },
            labels: self
                .merge_request
                .labels
                .iter()
                .cloned()
                .map(Label)
                .collect(),
        })
    }

    pub fn to_review_snapshot(&self) -> Result<ReviewSnapshot> {
        let repository_url = repository_url_from_review_url(&self.merge_request.url);
        let (namespace, name) = repository_identity_from_url(repository_url.as_deref());

        Ok(ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitLab,
                project_key: self.project_id.clone(),
                review_id: self.merge_request_iid.clone(),
                web_url: Some(self.merge_request.url.clone()),
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitLab,
                namespace,
                name,
                web_url: repository_url,
            },
            title: self.merge_request.title.clone(),
            description: self.merge_request.description.clone(),
            author: Actor {
                username: self.merge_request.author.clone(),
                display_name: None,
            },
            participants: self
                .merge_request
                .existing_reviewers
                .iter()
                .cloned()
                .map(|username| Actor {
                    username,
                    display_name: None,
                })
                .collect(),
            changed_files: self
                .merge_request
                .changed_files
                .iter()
                .map(FixtureChangedFile::to_changed_file)
                .collect::<Result<Vec<_>>>()?,
            labels: self.merge_request.labels.clone(),
            is_draft: self.merge_request.is_draft,
            default_branch: self.merge_request.default_branch.clone(),
            metadata: ReviewMetadata {
                source_branch: Some(self.merge_request.source_branch.clone()),
                target_branch: Some(self.merge_request.target_branch.clone()),
                commit_count: None,
                approvals_required: None,
                approvals_given: None,
            },
        })
    }

    pub fn to_rule_outcome(&self) -> Result<RuleOutcome> {
        let findings = self
            .findings
            .iter()
            .map(FixtureFinding::to_rule_finding)
            .collect::<Result<Vec<_>>>()?;
        let mut action_plan = ActionPlan::new();

        for action in &self.actions {
            action_plan.push(action.to_review_action()?);
        }

        Ok(RuleOutcome {
            findings,
            action_plan,
        })
    }
}

impl FixtureChangedFile {
    fn to_changed_file(&self) -> Result<ChangedFile> {
        Ok(ChangedFile {
            path: self.path.clone(),
            change_type: parse_change_type(&self.change_type)?,
            additions: self.additions,
            deletions: self.deletions,
        })
    }
}

impl FixtureFinding {
    fn to_rule_finding(&self) -> Result<RuleFinding> {
        let severity = match self.severity.trim() {
            "info" => FindingSeverity::Info,
            "warning" => FindingSeverity::Warning,
            "blocking" => FindingSeverity::Blocking,
            other => bail!("unsupported fixture finding severity '{}'", other),
        };

        Ok(RuleFinding {
            severity,
            message: self.message.clone(),
        })
    }
}

impl FixtureAction {
    fn to_review_action(&self) -> Result<ReviewAction> {
        match self.kind.trim() {
            "assign-reviewers" => Ok(ReviewAction::AssignReviewers {
                reviewers: self
                    .reviewers
                    .iter()
                    .cloned()
                    .map(|username| Actor {
                        username,
                        display_name: None,
                    })
                    .collect(),
            }),
            "add-labels" => Ok(ReviewAction::AddLabels {
                labels: self.labels.clone(),
            }),
            "remove-labels" => Ok(ReviewAction::RemoveLabels {
                labels: self.labels.clone(),
            }),
            "fail-pipeline" => Ok(ReviewAction::FailPipeline {
                reason: self
                    .reason
                    .clone()
                    .filter(|reason| !reason.trim().is_empty())
                    .ok_or_else(|| anyhow!("fixture action 'fail-pipeline' requires a reason"))?,
            }),
            other => bail!("unsupported fixture action '{}'", other),
        }
    }
}

fn parse_pipeline_source(raw: &str) -> Result<PipelineSource> {
    match raw.trim() {
        "merge_request_event" => Ok(PipelineSource::MergeRequestEvent),
        "push" => Ok(PipelineSource::Push),
        "schedule" => Ok(PipelineSource::Schedule),
        "unknown" => Ok(PipelineSource::Unknown),
        other => bail!("unsupported fixture pipeline_source '{}'", other),
    }
}

fn parse_change_type(raw: &str) -> Result<ChangeType> {
    match raw.trim() {
        "added" => Ok(ChangeType::Added),
        "modified" => Ok(ChangeType::Modified),
        "deleted" => Ok(ChangeType::Deleted),
        "renamed" => Ok(ChangeType::Renamed),
        "unknown" => Ok(ChangeType::Unknown),
        other => bail!("unsupported fixture change_type '{}'", other),
    }
}

fn repository_url_from_review_url(review_url: &str) -> Option<String> {
    review_url
        .split_once("/-/merge_requests/")
        .map(|(prefix, _)| prefix.to_string())
        .filter(|value| !value.trim().is_empty())
}

fn repository_identity_from_url(repository_url: Option<&str>) -> (String, String) {
    let Some(repository_url) = repository_url else {
        return ("group".to_string(), "project".to_string());
    };

    let trimmed = repository_url.trim_end_matches('/');
    let parts = trimmed.split('/').collect::<Vec<_>>();
    let name = parts.last().copied().unwrap_or("project").to_string();
    let namespace = if parts.len() > 1 {
        parts[parts.len().saturating_sub(2)].to_string()
    } else {
        "group".to_string()
    };

    (namespace, name)
}

fn default_pipeline_source() -> String {
    "merge_request_event".to_string()
}

fn default_change_type() -> String {
    "modified".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_into_runtime_models() {
        let fixture = toml::from_str::<ReviewFixture>(
            r#"
project_id = "412"
merge_request_iid = "3995"
notification_variant = "first"

[merge_request]
title = "Frontend adjustments"
url = "https://gitlab.example.com/group/project/-/merge_requests/3995"
author = "arthur"
source_branch = "feat/buttons"
target_branch = "develop"
labels = ["frontend"]
existing_reviewers = ["principal-reviewer"]

[[merge_request.changed_files]]
path = "apps/frontend/button.tsx"

[[findings]]
severity = "warning"
message = "Missing label."

[[actions]]
kind = "assign-reviewers"
reviewers = ["bob"]
"#,
        )
        .expect("fixture should parse");

        let ctx = fixture.to_ci_context().expect("context should build");
        let snapshot = fixture.to_review_snapshot().expect("snapshot should build");
        let outcome = fixture.to_rule_outcome().expect("outcome should build");

        assert!(ctx.is_merge_request_pipeline());
        assert_eq!(snapshot.review_ref.review_id, "3995");
        assert_eq!(snapshot.changed_files.len(), 1);
        assert_eq!(outcome.findings.len(), 1);
        assert_eq!(outcome.action_plan.actions.len(), 1);
        assert_eq!(
            fixture.notification_template_variant(),
            Some(NotificationTemplateVariant::First)
        );
    }

    #[test]
    fn rejects_unknown_action_kind() {
        let action = FixtureAction {
            kind: "something-else".to_string(),
            reviewers: Vec::new(),
            labels: Vec::new(),
            reason: None,
        };

        let error = action.to_review_action().expect_err("action should fail");
        assert!(error.to_string().contains("unsupported fixture action"));
    }
}
