#![cfg(feature = "slack-app")]

#[path = "support/mock_server.rs"]
mod mock_server;

use mock_server::MockGitLabServer;
use mr_milchick::connectors::notifications::slack_app::{SlackAppConfig, SlackAppSink};
use mr_milchick::core::model::{
    NotificationAudience, NotificationDelivery, NotificationMessage, NotificationSeverity,
    NotificationSinkKind, ReviewCommit,
};
use mr_milchick::runtime::NotificationSink;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[tokio::test]
async fn sends_compact_slack_message_and_thread_payload() {
    let server = MockGitLabServer::start();
    let mut user_map = BTreeMap::new();
    user_map.insert("arthur".to_string(), "U01AUTHOR1".to_string());
    user_map.insert("principal-reviewer".to_string(), "U01REVIEW1".to_string());
    user_map.insert("bob".to_string(), "U01REVIEW2".to_string());
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map,
    });

    let notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: "Reviews Needed for <https://gitlab.example.com/group/project/-/merge_requests/3995|MR #3995>, by @arthur :pepe-review:".to_string(),
        body: "*The department has a request.*\nReview requested for: <https://gitlab.example.com/group/project/-/merge_requests/3995|Frontend adjustments>\n_Assigned reviewers_ *@principal-reviewer* *@bob*".to_string(),
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
        thread_key: Some("MR #3995".to_string()),
        prefer_thread_reply: false,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::Append,
    };

    let report = sink.send(&notification).await.expect("send should succeed");
    assert!(report.delivered);

    let bodies = server.request_bodies("POST", "/slack/api/chat.postMessage");
    assert_eq!(bodies.len(), 2);

    let payload: Value =
        serde_json::from_str(&bodies[0]).expect("top-level Slack payload should parse");
    assert_eq!(payload["channel"], json!("C0ALY38CW3X"));
    assert_eq!(
        payload["text"],
        json!(
            "Reviews Needed for <https://gitlab.example.com/group/project/-/merge_requests/3995|MR #3995>, by <@U01AUTHOR1> :pepe-review:"
        )
    );
    assert!(payload["thread_ts"].is_null());

    let thread_payload: Value =
        serde_json::from_str(&bodies[1]).expect("thread Slack payload should parse");
    assert_eq!(thread_payload["channel"], json!("C0ALY38CW3X"));
    assert_eq!(thread_payload["thread_ts"], json!("1700000000.000001"));

    let thread_message = thread_payload["text"]
        .as_str()
        .expect("thread message should be a string");
    assert!(thread_message.starts_with('*'));
    assert!(thread_message.contains("Review requested for: <https://gitlab.example.com/group/project/-/merge_requests/3995|Frontend adjustments>"));
    assert!(thread_message.contains("_Assigned reviewers_ *<@U01REVIEW1>* *<@U01REVIEW2>*"));
}

#[tokio::test]
async fn replies_to_existing_new_mr_thread_for_update_notifications() {
    let server = MockGitLabServer::start();
    let mut user_map = BTreeMap::new();
    user_map.insert("arthur".to_string(), "U01AUTHOR1".to_string());
    user_map.insert("principal-reviewer".to_string(), "U01REVIEW1".to_string());
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map,
    });

    let first_notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: "Mr. Milchick took a first look at <https://gitlab.example.com/group/project/-/merge_requests/3995|MR #3995>, by @arthur".to_string(),
        body: "*The department has a request.*\nReview requested for: <https://gitlab.example.com/group/project/-/merge_requests/3995|Frontend adjustments>\n_Assigned reviewers_ *@principal-reviewer*".to_string(),
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
        thread_key: Some("MR #3995".to_string()),
        prefer_thread_reply: false,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::UpsertThreadReply {
            slot: "observe".to_string(),
        },
    };

    let update_notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: "Mr. Milchick - updates on <https://gitlab.example.com/group/project/-/merge_requests/3995|MR #3995>".to_string(),
        body: "Merge request: <https://gitlab.example.com/group/project/-/merge_requests/3995|Frontend adjustments>\nNo findings were produced.".to_string(),
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
        thread_key: Some("MR #3995".to_string()),
        prefer_thread_reply: true,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::UpsertThreadReply {
            slot: "refine".to_string(),
        },
    };

    sink.send(&first_notification)
        .await
        .expect("first send should succeed");
    sink.send(&update_notification)
        .await
        .expect("update send should succeed");

    assert_eq!(
        server.request_count_prefix("GET", "/slack/api/conversations.history"),
        1
    );

    let bodies = server.request_bodies("POST", "/slack/api/chat.postMessage");
    assert_eq!(bodies.len(), 3);

    let update_payload: Value =
        serde_json::from_str(&bodies[2]).expect("update Slack payload should parse");
    assert_eq!(update_payload["channel"], json!("C0ALY38CW3X"));
    assert_eq!(update_payload["thread_ts"], json!("1700000000.000001"));

    let update_text = update_payload["text"]
        .as_str()
        .expect("update text should be a string");
    assert!(update_text.contains("Merge request: <https://gitlab.example.com/group/project/-/merge_requests/3995|Frontend adjustments>"));
    assert!(update_text.contains("No findings were produced."));
}

#[tokio::test]
async fn updates_durable_stage_reply_instead_of_appending_another_message() {
    let server = MockGitLabServer::start();
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map: BTreeMap::new(),
    });
    let mut notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: "Mr. Milchick is observing MR #3995".to_string(),
        body: "Observe status: ready".to_string(),
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
        thread_key: Some("MR #3995".to_string()),
        prefer_thread_reply: true,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::UpsertThreadReply {
            slot: "observe".to_string(),
        },
    };

    sink.send(&notification)
        .await
        .expect("initial send should succeed");
    notification.body = "Observe status: blocked".to_string();
    sink.send(&notification)
        .await
        .expect("update should succeed");

    assert_eq!(
        server.request_count("POST", "/slack/api/chat.postMessage"),
        2
    );
    assert_eq!(server.request_count("POST", "/slack/api/chat.update"), 1);
    let updates = server.request_bodies("POST", "/slack/api/chat.update");
    let update: Value = serde_json::from_str(&updates[0]).expect("update should be JSON");
    assert_eq!(update["text"], json!("Observe status: blocked"));
    assert_eq!(
        update["metadata"]["event_payload"]["message_key"],
        json!("slot:observe")
    );
}

#[tokio::test]
async fn appends_only_new_commits_as_a_ci_summary_with_a_bulleted_list() {
    let server = MockGitLabServer::start();
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map: BTreeMap::new(),
    });
    let commits = vec![commit("11111111", "First"), commit("22222222", "Second")];
    let mut notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: "Mr. Milchick is observing MR #3995".to_string(),
        body: String::new(),
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
        thread_key: Some("MR #3995".to_string()),
        prefer_thread_reply: true,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::AppendCommitLog {
            commits,
            ci_summary: "🟢 CI: 18/18 passed".to_string(),
        },
    };

    sink.send(&notification)
        .await
        .expect("first commit send should succeed");
    notification.delivery = NotificationDelivery::AppendCommitLog {
        commits: vec![
            commit("11111111", "First"),
            commit("22222222", "Second"),
            commit("33333333", "Third"),
            commit("44444444", "Fourth"),
        ],
        ci_summary: "🔴 CI: 16/18 passed".to_string(),
    };
    sink.send(&notification)
        .await
        .expect("new commits should send");
    sink.send(&notification)
        .await
        .expect("duplicate should be skipped");

    let posts = server.request_bodies("POST", "/slack/api/chat.postMessage");
    assert_eq!(posts.len(), 3);
    let first_update: Value = serde_json::from_str(&posts[1]).expect("post should be JSON");
    assert!(
        first_update["text"]
            .as_str()
            .unwrap()
            .contains("`22222222` Second")
    );
    assert!(!first_update["text"].as_str().unwrap().contains("First"));
    let second_update: Value = serde_json::from_str(&posts[2]).expect("post should be JSON");
    let text = second_update["text"].as_str().unwrap();
    assert!(text.starts_with("🔴 CI: 16/18 passed\n"));
    assert!(text.contains("`33333333` Third"));
    assert!(text.contains("`44444444` Fourth"));
    assert_eq!(text.lines().count(), 3);
}

fn commit(id: &str, title: &str) -> ReviewCommit {
    ReviewCommit {
        id: id.to_string(),
        title: title.to_string(),
        web_url: format!("https://gitlab.example.com/commit/{id}"),
    }
}
