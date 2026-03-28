#![cfg(feature = "slack-workflow")]

#[path = "support/mock_server.rs"]
mod mock_server;

use mock_server::MockGitLabServer;
use mr_milchick::connectors::notifications::slack_workflow::{
    SlackWorkflowConfig, SlackWorkflowSink,
};
use mr_milchick::core::model::{
    NotificationAudience, NotificationMessage, NotificationSeverity, NotificationSinkKind,
};
use mr_milchick::runtime::NotificationSink;
use serde_json::{Value, json};

#[tokio::test]
async fn sends_workflow_messages_with_simple_formatting_and_threading() {
    let server = MockGitLabServer::start();
    let sink = SlackWorkflowSink::new(SlackWorkflowConfig {
        enabled: true,
        webhook_url: Some(format!("{}/webhook/test", server.slack_api_base_url())),
        channel: Some("C0ALY38CW3X".to_string()),
    });

    let notification = NotificationMessage {
        sink: NotificationSinkKind::SlackWorkflow,
        subject: "Reviews Needed for MR #3995 (https://gitlab.example.com/group/project/-/merge_requests/3995), by @arthur :pepe-review:".to_string(),
        body: "The department has a request.\nReview requested for: Frontend adjustments (https://gitlab.example.com/group/project/-/merge_requests/3995)\nAssigned reviewers @principal-reviewer @bob".to_string(),
        audience: NotificationAudience::Default,
        severity: NotificationSeverity::Info,
    };

    let report = sink.send(&notification).await.expect("send should succeed");
    assert!(report.delivered);
    assert_eq!(report.detail.as_deref(), Some("sent via Slack workflow"));

    let bodies = server.request_bodies("POST", "/slack/api/webhook/test");
    assert_eq!(bodies.len(), 1);

    let payload: Value = serde_json::from_str(&bodies[0]).expect("workflow payload should parse");
    assert_eq!(payload["mr_milchick_talks_to"], json!("C0ALY38CW3X"));
    assert_eq!(
        payload["mr_milchick_says"],
        json!(
            "Reviews Needed for MR #3995 (https://gitlab.example.com/group/project/-/merge_requests/3995), by @arthur :pepe-review:"
        )
    );

    let thread_message = payload["mr_milchick_says_thread"]
        .as_str()
        .expect("thread message should be a string");
    assert!(thread_message.contains("The department has a request."));
    assert!(thread_message.contains("Review requested for: Frontend adjustments (https://gitlab.example.com/group/project/-/merge_requests/3995)"));
    assert!(thread_message.contains("Assigned reviewers @principal-reviewer @bob"));
    assert!(!thread_message.contains('*'));
    assert!(!thread_message.contains('<'));
}
