#![cfg(feature = "gitlab")]

#[path = "support/mock_server.rs"]
mod mock_server;

use milchick_connectors::gitlab::api::GitLabConfig;
use milchick_connectors::gitlab::{GitLabReviewConnector, MR_MILCHICK_MARKER};
use milchick_core::model::{Actor, RenderedMessage, ReviewAction, ReviewActionKind};
use milchick_runtime::ReviewConnector;
use mock_server::{MERGE_REQUEST_IID, MockGitLabServer, PROJECT_ID};
use serde_json::{Value, json};

fn connector(server: &MockGitLabServer) -> GitLabReviewConnector {
    GitLabReviewConnector::new(
        GitLabConfig {
            base_url: server.api_base_url(),
            token: "test-token".to_string(),
        },
        PROJECT_ID,
        MERGE_REQUEST_IID,
        "feat/intentional-cleanup",
        "develop",
        Vec::new(),
    )
}

#[tokio::test]
async fn loads_neutral_snapshot_from_gitlab() {
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
}

#[tokio::test]
async fn applies_review_actions_idempotently() {
    let server = MockGitLabServer::start();
    let connector = connector(&server);
    let mr_path = format!("/api/v4/projects/{PROJECT_ID}/merge_requests/{MERGE_REQUEST_IID}");
    let notes_path = format!("{mr_path}/notes");

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
            message: RenderedMessage::new(Some("Summary".to_string())),
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

    assert_eq!(server.request_count("PUT", &mr_path), 2);
    assert_eq!(server.request_count("POST", &notes_path), 1);
    assert_eq!(server.request_count("GET", &notes_path), 2);

    let reviewer_assignment_bodies = server.request_bodies("PUT", &mr_path);
    assert_eq!(
        serde_json::from_str::<Value>(&reviewer_assignment_bodies[0])
            .expect("reviewer assignment body should parse"),
        json!({"reviewer_ids": [1001, 1002]})
    );
}

#[tokio::test]
async fn merges_existing_reviewers_instead_of_replacing_them() {
    let server = MockGitLabServer::start_with_reviewers(vec!["alice"]);
    let connector = connector(&server);
    let mr_path = format!("/api/v4/projects/{PROJECT_ID}/merge_requests/{MERGE_REQUEST_IID}");

    let actions = vec![ReviewAction::AssignReviewers {
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
    }];

    let report = connector
        .apply_review_actions(&actions)
        .await
        .expect("apply should succeed");

    assert_eq!(report.applied.len(), 1);
    assert_eq!(
        server.assigned_reviewers(),
        vec!["alice", "principal-reviewer", "bob"]
    );

    let reviewer_assignment_bodies = server.request_bodies("PUT", &mr_path);
    assert_eq!(
        serde_json::from_str::<Value>(&reviewer_assignment_bodies[0])
            .expect("reviewer assignment body should parse"),
        json!({"reviewer_ids": [1004, 1001, 1002]})
    );
}
