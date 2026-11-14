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
    assert_eq!(snapshot.changed_files[0].previous_path, None);
    assert_eq!(
        snapshot.changed_files[0].patch.as_deref(),
        Some("@@ -1,2 +1,2 @@")
    );
    assert_eq!(snapshot.labels, vec!["frontend".to_string()]);
    assert_eq!(snapshot.metadata.commits.len(), 1);
    assert_eq!(
        snapshot.metadata.commits[0].title,
        "Finish frontend adjustments"
    );
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

    let reviewer_assignment_bodies =
        server.request_bodies("POST", &format!("{pr_path}/requested_reviewers"));
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
    let files_path =
        format!("/api/github/repos/{GITHUB_PROJECT_KEY}/pulls/{PULL_REQUEST_NUMBER}/files");
    assert_eq!(server.request_count_prefix("GET", &files_path), 2);
}

#[tokio::test]
async fn risk_transition_removes_old_risk_labels_but_preserves_unrelated_labels() {
    let server = MockGitLabServer::start_with_labels(vec!["risk::high", "frontend"]);
    let connector = connector(&server);

    // Remove risk::high, add risk::low
    let actions = vec![
        ReviewAction::RemoveLabels {
            labels: vec!["risk::high".to_string()],
        },
        ReviewAction::AddLabels {
            labels: vec!["risk::low".to_string()],
        },
    ];

    connector
        .apply_review_actions(&actions)
        .await
        .expect("actions should succeed");

    assert_eq!(server.labels(), vec!["frontend", "risk::low"]);
}

#[tokio::test]
async fn removes_label_with_special_chars_and_preserves_unrelated() {
    let special_label = "risk #50% /?+";
    let server = MockGitLabServer::start_with_labels(vec![special_label, "frontend"]);
    let connector = connector(&server);

    let actions = vec![ReviewAction::RemoveLabels {
        labels: vec![special_label.to_string()],
    }];

    connector
        .apply_review_actions(&actions)
        .await
        .expect("actions should succeed");

    assert_eq!(server.labels(), vec!["frontend"]);
}

#[tokio::test]
async fn label_only_action_does_not_call_user_endpoint() {
    let server = MockGitLabServer::start_with_labels(vec!["frontend"]);
    let connector = connector(&server);

    // Label-only action should not trigger /user lookup
    let actions = vec![ReviewAction::AddLabels {
        labels: vec!["new-label".to_string()],
    }];

    connector
        .apply_review_actions(&actions)
        .await
        .expect("actions should succeed");

    assert_eq!(server.request_count("GET", "/api/github/user"), 0);
}

#[tokio::test]
async fn summary_identical_rerun_makes_one_post_total() {
    let server = MockGitLabServer::start();
    let connector = connector(&server);
    let comments_path =
        format!("/api/github/repos/{GITHUB_PROJECT_KEY}/issues/{PULL_REQUEST_NUMBER}/comments");

    let actions = vec![ReviewAction::UpsertSummary {
        markdown: "## Summary".to_string(),
    }];

    connector
        .apply_review_actions(&actions)
        .await
        .expect("first apply should succeed");

    assert_eq!(server.request_count("POST", &comments_path), 1);

    // Second run with same summary
    connector
        .apply_review_actions(&actions)
        .await
        .expect("second apply should succeed");

    assert_eq!(server.request_count("POST", &comments_path), 1);
}

#[tokio::test]
async fn changed_summary_patches_owned_existing_comment() {
    let server = MockGitLabServer::start();
    // Pre-populate a comment owned by the bot (github-actions[bot] in Actions context)
    // We need to simulate an existing comment. The mock server starts empty.
    // We can manually add a note via internal state or use POST first.

    let connector = connector(&server);
    let comments_path =
        format!("/api/github/repos/{GITHUB_PROJECT_KEY}/issues/{PULL_REQUEST_NUMBER}/comments");

    // First post
    let actions1 = vec![ReviewAction::UpsertSummary {
        markdown: "## Summary v1".to_string(),
    }];
    connector
        .apply_review_actions(&actions1)
        .await
        .expect("first apply should succeed");

    assert_eq!(server.request_count("POST", &comments_path), 1);
    assert_eq!(
        server.request_count(
            "PATCH",
            &format!("/api/github/repos/{GITHUB_PROJECT_KEY}/issues/comments/1")
        ),
        0
    );

    // Second run with changed summary
    let actions2 = vec![ReviewAction::UpsertSummary {
        markdown: "## Summary v2".to_string(),
    }];
    connector
        .apply_review_actions(&actions2)
        .await
        .expect("second apply should succeed");

    assert_eq!(server.request_count("POST", &comments_path), 1);
    assert_eq!(
        server.request_count(
            "PATCH",
            &format!("/api/github/repos/{GITHUB_PROJECT_KEY}/issues/comments/1")
        ),
        1
    );
}

#[tokio::test]
async fn spoofed_human_comment_with_summary_marker_is_ignored() {
    let server = MockGitLabServer::start();

    // Manually inject a comment with marker but wrong author (human)
    server.add_github_comment(
        99,
        format!(
            "{}\n\nSpoofed summary",
            mr_milchick::connectors::github::MR_MILCHICK_MARKER
        ),
        Some("human-user".to_string()),
    );

    let connector = connector(&server);
    let comments_path =
        format!("/api/github/repos/{GITHUB_PROJECT_KEY}/issues/{PULL_REQUEST_NUMBER}/comments");

    let actions = vec![ReviewAction::UpsertSummary {
        markdown: "## Real Summary".to_string(),
    }];

    connector
        .apply_review_actions(&actions)
        .await
        .expect("apply should succeed");

    // Should POST a new comment because the spoofed one is ignored
    assert_eq!(server.request_count("POST", &comments_path), 1);

    // Verify the new comment is owned by bot (in Actions context)
    let notes = server.note_bodies();
    assert_eq!(notes.len(), 2); // Spoofed + New
    assert!(notes[1].contains("Real Summary"));
}

#[tokio::test]
async fn load_managed_comment_ignores_spoofed_advisory_text() {
    let server = MockGitLabServer::start();

    // Inject spoofed comment
    server.add_github_comment(
        99,
        format!(
            "{}\n\nSpoofed",
            mr_milchick::connectors::github::MR_MILCHICK_MARKER
        ),
        Some("human-user".to_string()),
    );

    let connector = connector(&server);

    let managed = connector
        .load_managed_comment(mr_milchick::connectors::github::MR_MILCHICK_MARKER)
        .await
        .expect("load should succeed");
    assert!(managed.is_none(), "Should ignore spoofed comment");
}

#[tokio::test]
async fn summary_ignores_comment_with_missing_author_and_posts_new_owned_summary() {
    let server = MockGitLabServer::start();

    // Seed a comment with the marker but no author (deleted account scenario)
    server.add_github_comment(
        99,
        format!(
            "{}\n\nOrphaned summary",
            mr_milchick::connectors::github::MR_MILCHICK_MARKER
        ),
        None,
    );

    let connector = connector(&server);
    let comments_path =
        format!("/api/github/repos/{GITHUB_PROJECT_KEY}/issues/{PULL_REQUEST_NUMBER}/comments");

    let actions = vec![ReviewAction::UpsertSummary {
        markdown: "## New Summary".to_string(),
    }];

    // First apply should succeed despite missing author in existing comment
    connector
        .apply_review_actions(&actions)
        .await
        .expect("apply should succeed even with null user");

    // Assert no PATCH to the spoofed/unknown existing comment (id 99)
    assert_eq!(
        server.request_count(
            "PATCH",
            &format!("/api/github/repos/{GITHUB_PROJECT_KEY}/issues/comments/99")
        ),
        0
    );

    // Assert one POST of new owned summary
    assert_eq!(server.request_count("POST", &comments_path), 1);

    // Rerun yields no duplicate
    connector
        .apply_review_actions(&actions)
        .await
        .expect("second apply should succeed");

    assert_eq!(server.request_count("POST", &comments_path), 1);
}
