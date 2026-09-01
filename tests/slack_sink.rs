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
async fn append_notifications_never_update_managed_thread_messages() {
    let server = MockGitLabServer::start();
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map: BTreeMap::new(),
    });

    let root = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: "Mr. Milchick advisory explain for MR #3995".to_string(),
        body: "Initial advisory review".to_string(),
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
        thread_key: Some("MR #3995".to_string()),
        prefer_thread_reply: false,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::Append,
    };
    sink.send(&root).await.expect("root should be created");

    let append = NotificationMessage {
        prefer_thread_reply: true,
        body: "Advisory review details".to_string(),
        ..root
    };
    sink.send(&append)
        .await
        .expect("first append should succeed");
    sink.send(&append)
        .await
        .expect("second append should succeed");

    assert_eq!(
        server.request_count_prefix("GET", "/slack/api/conversations.history"),
        0
    );
    assert_eq!(server.request_count("POST", "/slack/api/chat.update"), 0);
    let posts = server.request_bodies("POST", "/slack/api/chat.postMessage");
    assert_eq!(posts.len(), 4);
    for post in posts.iter().skip(1) {
        let payload: Value = serde_json::from_str(post).expect("post should be JSON");
        assert_eq!(payload["thread_ts"], json!("1700000000.000001"));
        assert!(payload["metadata"].is_null());
    }
}

#[tokio::test]
async fn defers_thread_delivery_when_slack_history_lookup_fails() {
    let server = MockGitLabServer::start();
    server.fail_slack_history(429, r#"{"ok":false,"error":"ratelimited"}"#);
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map: BTreeMap::new(),
    });
    let notification = NotificationMessage {
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

    let report = sink
        .send(&notification)
        .await
        .expect("delivery should defer");

    assert!(!report.delivered);
    assert_eq!(
        report.detail.as_deref(),
        Some("deferred: Slack thread lookup failed")
    );
    assert_eq!(
        server.request_count("POST", "/slack/api/chat.postMessage"),
        0
    );
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
        0
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
async fn sends_pipeline_failure_to_author_dm_and_deduplicates_same_head() {
    let server = MockGitLabServer::start();
    let mut user_map = BTreeMap::new();
    user_map.insert("arthur".to_string(), "U01234567".to_string());
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map,
    });
    let commits = vec![commit("11111111", "First"), commit("22222222", "Second")];
    let mut notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: "Mr. Milchick is observing MR #3995".to_string(),
        body: "status detail must not leak".to_string(),
        audience: NotificationAudience::User("arthur".to_string()),
        severity: NotificationSeverity::Info,
        thread_key: None,
        prefer_thread_reply: false,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::DirectPipelineFailure {
            review_url: "https://gitlab.example.com/group/project/-/merge_requests/3995"
                .to_string(),
            review_label: "MR #3995".to_string(),
            head_commit: commits[1].clone(),
            failed_checks: vec!["lint".to_string()],
        },
    };

    sink.send(&notification)
        .await
        .expect("first failure send should succeed");
    notification.delivery = NotificationDelivery::DirectPipelineFailure {
        review_url: "https://gitlab.example.com/group/project/-/merge_requests/3995".to_string(),
        review_label: "MR #3995".to_string(),
        head_commit: commit("4444444444444444444444444444444444444444", "Fourth"),
        failed_checks: vec!["lint".to_string()],
    };
    sink.send(&notification)
        .await
        .expect("new head should send");
    notification.delivery = NotificationDelivery::DirectPipelineFailure {
        review_url: "https://gitlab.example.com/group/project/-/merge_requests/3995".to_string(),
        review_label: "MR #3995".to_string(),
        head_commit: commit("4444444444444444444444444444444444444444", "Fourth"),
        failed_checks: vec!["lint".to_string()],
    };
    let duplicate = sink
        .send(&notification)
        .await
        .expect("duplicate should succeed");
    assert!(!duplicate.delivered);
    assert_eq!(duplicate.detail.as_deref(), Some("already present"));

    let posts = server.request_bodies("POST", "/slack/api/chat.postMessage");
    assert_eq!(posts.len(), 2);
    let first = serde_json::from_str::<Value>(&posts[0]).expect("post should be JSON");
    let second = serde_json::from_str::<Value>(&posts[1]).expect("post should be JSON");
    for payload in [&first, &second] {
        assert_eq!(payload["channel"], json!("D01234567"));
        assert!(!payload.as_object().unwrap().contains_key("thread_ts"));
        assert_eq!(payload["metadata"]["event_type"], json!("mr_milchick"));
        assert_eq!(
            payload["metadata"]["event_payload"]["conversation_key"],
            json!("gitlab:412:3995")
        );
    }
    assert_eq!(
        first["text"],
        json!(
            ":red_circle: CI failed for <https://gitlab.example.com/group/project/-/merge_requests/3995|MR #3995>.\n*Change under test:* <https://gitlab.example.com/commit/22222222|`22222222` Second>\n*Failed checks:* lint"
        )
    );
    assert!(!first["text"].as_str().unwrap().contains("status detail"));
    assert_eq!(
        first["metadata"]["event_payload"]["message_key"],
        json!("pipeline-failure:22222222")
    );
    assert_eq!(
        second["metadata"]["event_payload"]["message_key"],
        json!("pipeline-failure:4444444444444444444444444444444444444444")
    );
    assert_eq!(
        server.request_count("POST", "/slack/api/conversations.open"),
        3
    );
    let opens = server.request_bodies("POST", "/slack/api/conversations.open");
    assert_eq!(opens.len(), 3);
    for open in opens {
        let payload: Value = serde_json::from_str(&open).expect("open should be JSON");
        assert_eq!(payload["users"], json!("U01234567"));
    }
}

#[tokio::test]
async fn skips_pipeline_failure_dm_without_valid_author_mapping_or_fallback() {
    let server = MockGitLabServer::start();
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map: BTreeMap::from([(String::from("arthur"), String::from("not-a-user-id"))]),
    });
    let notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: String::new(),
        body: String::new(),
        audience: NotificationAudience::User("arthur".to_string()),
        severity: NotificationSeverity::Critical,
        thread_key: None,
        prefer_thread_reply: false,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::DirectPipelineFailure {
            review_url: "https://gitlab.example.com/mr/3995".to_string(),
            review_label: "MR #3995".to_string(),
            head_commit: commit("2222222222222222222222222222222222222222", "Second"),
            failed_checks: vec!["lint".to_string()],
        },
    };

    let report = sink
        .send(&notification)
        .await
        .expect("skip should be reported");
    assert!(!report.delivered);
    assert_eq!(
        report.detail.as_deref(),
        Some("author Slack user mapping unavailable")
    );
    assert_eq!(
        server.request_count("POST", "/slack/api/conversations.open"),
        0
    );
    assert_eq!(
        server.request_count("POST", "/slack/api/chat.postMessage"),
        0
    );
}

#[tokio::test]
async fn skips_pipeline_failure_dm_when_author_mapping_is_missing() {
    let server = MockGitLabServer::start();
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map: BTreeMap::new(),
    });
    let notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: String::new(),
        body: String::new(),
        audience: NotificationAudience::User("arthur".to_string()),
        severity: NotificationSeverity::Critical,
        thread_key: None,
        prefer_thread_reply: false,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::DirectPipelineFailure {
            review_url: "https://gitlab.example.com/mr/3995".to_string(),
            review_label: "MR #3995".to_string(),
            head_commit: commit("2222222222222222222222222222222222222222", "Second"),
            failed_checks: vec!["lint".to_string()],
        },
    };

    let report = sink
        .send(&notification)
        .await
        .expect("missing mapping should be reported");
    assert!(!report.delivered);
    assert_eq!(
        report.detail.as_deref(),
        Some("author Slack user mapping unavailable")
    );
    assert_eq!(
        server.request_count("POST", "/slack/api/conversations.open"),
        0
    );
    assert_eq!(
        server.request_count("POST", "/slack/api/chat.postMessage"),
        0
    );
}

#[tokio::test]
async fn updates_durable_stage_reply_beyond_first_replies_page() {
    let server = MockGitLabServer::start();
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
        user_map: BTreeMap::new(),
    });

    let mut append_notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: "Mr. Milchick is observing MR #3995".to_string(),
        body: "Historical update".to_string(),
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
        thread_key: Some("MR #3995".to_string()),
        prefer_thread_reply: true,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::Append,
    };
    sink.send(&append_notification)
        .await
        .expect("root thread should be created");
    for index in 0..205 {
        append_notification.body = format!("Historical update {index}");
        sink.send(&append_notification)
            .await
            .expect("historical reply should be sent");
    }

    let mut stage_notification = NotificationMessage {
        sink: NotificationSinkKind::SlackApp,
        subject: "Mr. Milchick is considering MR #3995".to_string(),
        body: "Refine status: reviewers pending".to_string(),
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
        thread_key: Some("MR #3995".to_string()),
        prefer_thread_reply: true,
        conversation_key: Some("gitlab:412:3995".to_string()),
        delivery: NotificationDelivery::UpsertThreadReply {
            slot: "refine".to_string(),
        },
    };
    sink.send(&stage_notification)
        .await
        .expect("managed stage reply should be created after older replies");

    stage_notification.body = "Refine status: failing CI".to_string();
    sink.send(&stage_notification)
        .await
        .expect("managed stage reply should be updated through pagination");

    assert_eq!(server.request_count("POST", "/slack/api/chat.update"), 1);
    let updates = server.request_bodies("POST", "/slack/api/chat.update");
    let update: Value = serde_json::from_str(&updates[0]).expect("update should be JSON");
    assert_eq!(update["text"], json!("Refine status: failing CI"));
    assert_eq!(
        update["metadata"]["event_payload"]["message_key"],
        json!("slot:refine")
    );
    assert!(
        server.request_count_prefix("GET", "/slack/api/conversations.replies") > 1,
        "upsert should page through older replies to find the managed stage message"
    );
}

fn commit(id: &str, title: &str) -> ReviewCommit {
    ReviewCommit {
        id: id.to_string(),
        title: title.to_string(),
        web_url: format!("https://gitlab.example.com/commit/{id}"),
    }
}
