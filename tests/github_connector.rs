#![cfg(feature = "github")]

#[path = "support/mock_server.rs"]
mod mock_server;

use mock_server::{GITHUB_PROJECT_KEY, MockGitLabServer, PULL_REQUEST_NUMBER};
use mr_milchick::connectors::github::api::GitHubConfig;
use mr_milchick::connectors::github::{GitHubReviewConnector, MR_MILCHICK_MARKER};
use mr_milchick::core::model::{Actor, ReviewAction, ReviewActionKind};
use mr_milchick::runtime::ReviewConnector;
use serde_json::{Value, json};

fn connector(server: &MockGitLabServer) -> GitHubReviewConnector {
    GitHubReviewConnector::new(
        GitHubConfig {
            base_url: server.github_api_base_url(),
            token: "test-token".to_string(),
        },
        GITHUB_PROJECT_KEY,
        PULL_REQUEST_NUMBER,
        "feat/intentional-cleanup",
        "develop",
        vec!["frontend".to_string()],
    )
}

#[tokio::test]
async fn loads_neutral_snapshot_from_github() {
    let server = MockGitLabServer::start();
    let connector = connector(&server);

    let snapshot = connector
        .load_snapshot()
        .await
        .expect("snapshot should load");

    assert_eq!(snapshot.review_ref.review_id, "3995");
    assert_eq!(snapshot.title, "Frontend adjustments");
    assert_eq!(snapshot.author.username, "arthur");
    assert_eq!(snapshot.changed_files.len(), 1);
    assert_eq!(snapshot.changed_files[0].path, "apps/frontend/button.tsx");
    assert_eq!(snapshot.labels, vec!["frontend".to_string()]);
}

#[tokio::test]
async fn applies_review_actions_idempotently() {
    let server = MockGitLabServer::start();
    let connector = connector(&server);
    let pr_path = format!("/api/github/repos/{GITHUB_PROJECT_KEY}/pulls/{PULL_REQUEST_NUMBER}");
    let comments_path =
        format!("/api/github/repos/{GITHUB_PROJECT_KEY}/issues/{PULL_REQUEST_NUMBER}/comments");

    let actions = vec![
        ReviewAction::AssignReviewers {
            reviewers: vec![
                Actor {
                    username: "principal-reviewer".to_string(),
                    display_name: None,
                },
                Actor {
                    username: "bob".to_string(),
                    display_name: None,
                },
            ],
        },
        ReviewAction::UpsertSummary {
            markdown: "## Summary".to_string(),
        },
    ];

    let first = connector
        .apply_review_actions(&actions)
        .await
        .expect("first apply should succeed");
    assert_eq!(first.applied.len(), 2);
    assert_eq!(
        server.assigned_reviewers(),
        vec!["principal-reviewer", "bob"]
    );
    assert_eq!(server.note_bodies().len(), 1);
    assert!(server.note_bodies()[0].contains(MR_MILCHICK_MARKER));

    let second = connector
        .apply_review_actions(&actions)
        .await
        .expect("second apply should succeed");
    assert_eq!(second.applied[0].action, ReviewActionKind::AssignReviewers);
    assert_eq!(second.skipped[0].action, ReviewActionKind::UpsertSummary);

    assert_eq!(server.request_count("POST", &comments_path), 1);
    assert_eq!(server.request_count("GET", &pr_path), 2);

    let reviewer_assignment_bodies = server.request_bodies("POST", &format!("{pr_path}/requested_reviewers"));
    assert_eq!(
        serde_json::from_str::<Value>(&reviewer_assignment_bodies[0])
            .expect("reviewer assignment body should parse"),
        json!({"reviewers": ["principal-reviewer", "bob"]})
    );
}

#[tokio::test]
async fn paginates_changed_files() {
    let server = MockGitLabServer::start_with_github_file_count(101);
    let connector = connector(&server);

    let snapshot = connector
        .load_snapshot()
        .await
        .expect("snapshot should load");

    assert_eq!(snapshot.changed_files.len(), 101);
    let files_path = format!("/api/github/repos/{GITHUB_PROJECT_KEY}/pulls/{PULL_REQUEST_NUMBER}/files");
    assert_eq!(server.request_count_prefix("GET", &files_path), 2);
}
