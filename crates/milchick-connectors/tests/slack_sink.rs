#![cfg(feature = "slack-app")]

#[path = "support/mock_server.rs"]
mod mock_server;

use milchick_connectors::notifications::slack_app::{SlackAppConfig, SlackAppSink};
use milchick_core::model::{
    MessageSection, NotificationAudience, NotificationMessage, NotificationSeverity,
    RenderedMessage,
};
use milchick_runtime::NotificationSink;
use mock_server::MockGitLabServer;
use serde_json::{Value, json};

#[tokio::test]
async fn sends_compact_slack_message_and_thread_payload() {
    let server = MockGitLabServer::start();
    let sink = SlackAppSink::new(SlackAppConfig {
        enabled: true,
        base_url: server.slack_api_base_url(),
        bot_token: Some("xoxb-test".to_string()),
        channel: Some("C0ALY38CW3X".to_string()),
    });

    let notification = NotificationMessage {
        subject: ":gitlab: Reviews Needed for <https://gitlab.example.com/group/project/-/merge_requests/3995|MR #3995>, by @arthur :pepe-review:".to_string(),
        body: RenderedMessage {
            title: Some("The department has a request.".to_string()),
            sections: vec![
                MessageSection::Paragraph(
                    "Review requested for: <https://gitlab.example.com/group/project/-/merge_requests/3995|Frontend adjustments>".to_string(),
                ),
                MessageSection::Paragraph(
                    "_Assign reviewers_ *@principal-reviewer* *@bob*".to_string(),
                ),
            ],
            footer: None,
        },
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
    };

    let report = sink.send(&notification).await.expect("send should succeed");
    assert!(report.delivered);

    let bodies = server.request_bodies("POST", "/slack/api/chat.postMessage");
    assert_eq!(bodies.len(), 2);

    let payload: Value =
        serde_json::from_str(&bodies[0]).expect("top-level Slack payload should parse");
    assert_eq!(payload["channel"], json!("C0ALY38CW3X"));
    assert_eq!(payload["text"], json!(notification.subject));
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
    assert!(thread_message.contains("_Assign reviewers_ *@principal-reviewer* *@bob*"));
}
