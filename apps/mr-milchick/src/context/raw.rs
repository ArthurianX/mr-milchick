use std::{env, fs};

use serde::Deserialize;

const PROJECT_KEY_OVERRIDE_ENV: &str = "MR_MILCHICK_PROJECT_KEY";
const REVIEW_ID_OVERRIDE_ENV: &str = "MR_MILCHICK_REVIEW_ID";
const PIPELINE_SOURCE_OVERRIDE_ENV: &str = "MR_MILCHICK_PIPELINE_SOURCE";
const SOURCE_BRANCH_OVERRIDE_ENV: &str = "MR_MILCHICK_SOURCE_BRANCH";
const TARGET_BRANCH_OVERRIDE_ENV: &str = "MR_MILCHICK_TARGET_BRANCH";
const LABELS_OVERRIDE_ENV: &str = "MR_MILCHICK_LABELS";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCiEnv {
    pub project_key: Option<String>,
    pub review_id: Option<String>,
    pub pipeline_source: Option<String>,
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub labels: Option<String>,
}

impl RawCiEnv {
    pub fn load() -> Self {
        let overrides = load_override_context();
        let provider = if env::var("GITHUB_ACTIONS")
            .map(|value| value == "true")
            .unwrap_or(false)
        {
            load_github_actions_context()
        } else {
            load_gitlab_ci_context()
        };

        Self {
            project_key: overrides.project_key.or(provider.project_key),
            review_id: overrides.review_id.or(provider.review_id),
            pipeline_source: overrides.pipeline_source.or(provider.pipeline_source),
            source_branch: overrides.source_branch.or(provider.source_branch),
            target_branch: overrides.target_branch.or(provider.target_branch),
            labels: overrides.labels.or(provider.labels),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct GithubPullRequestEvent {
    #[serde(default)]
    number: Option<u64>,
    #[serde(default)]
    pull_request: Option<GithubPullRequestPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct GithubPullRequestPayload {
    #[serde(default)]
    number: Option<u64>,
    head: GithubBranchRef,
    base: GithubBranchRef,
    #[serde(default)]
    labels: Vec<GithubLabel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct GithubBranchRef {
    #[serde(default)]
    #[serde(rename = "ref")]
    branch_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct GithubLabel {
    #[serde(default)]
    name: String,
}

fn load_override_context() -> RawCiEnv {
    RawCiEnv {
        project_key: env::var(PROJECT_KEY_OVERRIDE_ENV).ok(),
        review_id: env::var(REVIEW_ID_OVERRIDE_ENV).ok(),
        pipeline_source: env::var(PIPELINE_SOURCE_OVERRIDE_ENV).ok(),
        source_branch: env::var(SOURCE_BRANCH_OVERRIDE_ENV).ok(),
        target_branch: env::var(TARGET_BRANCH_OVERRIDE_ENV).ok(),
        labels: env::var(LABELS_OVERRIDE_ENV).ok(),
    }
}

fn load_gitlab_ci_context() -> RawCiEnv {
    RawCiEnv {
        project_key: env::var("CI_PROJECT_ID").ok(),
        review_id: env::var("CI_MERGE_REQUEST_IID").ok(),
        pipeline_source: env::var("CI_PIPELINE_SOURCE").ok(),
        source_branch: env::var("CI_MERGE_REQUEST_SOURCE_BRANCH_NAME").ok(),
        target_branch: env::var("CI_MERGE_REQUEST_TARGET_BRANCH_NAME").ok(),
        labels: env::var("CI_MERGE_REQUEST_LABELS").ok(),
    }
}

fn load_github_actions_context() -> RawCiEnv {
    let event_name = env::var("GITHUB_EVENT_NAME").ok();
    let repository = env::var("GITHUB_REPOSITORY").ok();
    let event = load_github_event_payload();
    let fallback_number = event.as_ref().and_then(|payload| payload.number);
    let pull_request = event.and_then(|payload| payload.pull_request);
    let review_id = pull_request
        .as_ref()
        .and_then(|payload| payload.number.or(fallback_number))
        .map(|number| number.to_string());

    let labels = pull_request.as_ref().map(|payload| {
        payload
            .labels
            .iter()
            .filter_map(|label| {
                let name = label.name.trim();
                (!name.is_empty()).then(|| name.to_string())
            })
            .collect::<Vec<_>>()
            .join(",")
    });

    RawCiEnv {
        project_key: repository,
        review_id,
        pipeline_source: event_name,
        source_branch: pull_request
            .as_ref()
            .map(|payload| payload.head.branch_ref.clone())
            .or_else(|| env::var("GITHUB_HEAD_REF").ok()),
        target_branch: pull_request
            .as_ref()
            .map(|payload| payload.base.branch_ref.clone())
            .or_else(|| env::var("GITHUB_BASE_REF").ok()),
        labels,
    }
}

fn load_github_event_payload() -> Option<GithubPullRequestEvent> {
    let path = env::var("GITHUB_EVENT_PATH").ok()?;
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<GithubPullRequestEvent>(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_pull_request_event_payload() {
        let payload = serde_json::from_str::<GithubPullRequestEvent>(
            r#"{
              "number": 42,
              "pull_request": {
                "head": { "ref": "feat/github" },
                "base": { "ref": "master" },
                "labels": [{ "name": "backend" }, { "name": "qa" }]
              }
            }"#,
        )
        .expect("payload should parse");

        assert_eq!(payload.number, Some(42));
        let pr = payload.pull_request.expect("pull request should exist");
        assert_eq!(pr.head.branch_ref, "feat/github");
        assert_eq!(pr.base.branch_ref, "master");
        assert_eq!(pr.labels.len(), 2);
    }
}
