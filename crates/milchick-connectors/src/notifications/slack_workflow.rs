use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;

use milchick_core::model::{
    MessageSection, NotificationAudience, NotificationDeliveryReport, NotificationMessage,
    NotificationSinkKind,
};
use milchick_runtime::{ConnectorError, ConnectorResult, NotificationSink};

use crate::notifications::simplify_slack_formatting;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackWorkflowConfig {
    pub enabled: bool,
    pub webhook_url: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SlackWorkflowSink {
    http: Client,
    config: SlackWorkflowConfig,
}

#[derive(Debug, Serialize)]
struct SlackWorkflowWebhookRequest<'a> {
    mr_milchick_talks_to: &'a str,
    mr_milchick_says: &'a str,
    mr_milchick_says_thread: &'a str,
}

impl SlackWorkflowSink {
    pub fn new(config: SlackWorkflowConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.webhook_url.is_some()
    }
}

#[async_trait]
impl NotificationSink for SlackWorkflowSink {
    fn kind(&self) -> NotificationSinkKind {
        NotificationSinkKind::SlackWorkflow
    }

    async fn send(
        &self,
        notification: &NotificationMessage,
    ) -> ConnectorResult<NotificationDeliveryReport> {
        if !self.is_enabled() {
            return Ok(NotificationDeliveryReport {
                sink: self.kind(),
                delivered: false,
                destination: None,
                detail: Some("disabled".to_string()),
            });
        }

        let webhook_url = self.config.webhook_url.as_deref().ok_or_else(|| {
            ConnectorError::Configuration("missing Slack workflow URL".to_string())
        })?;

        let channel = match &notification.audience {
            NotificationAudience::Default => self.config.channel.as_deref().ok_or_else(|| {
                ConnectorError::Configuration("missing Slack workflow channel".to_string())
            })?,
            NotificationAudience::Channel(channel) => channel.as_str(),
            NotificationAudience::User(user) | NotificationAudience::Group(user) => user.as_str(),
        };

        let title = render_slack_workflow_title(notification);
        let thread = render_slack_workflow_message(notification);

        post_workflow_webhook_message(&self.http, webhook_url, channel, &title, &thread)
            .await
            .map_err(|err| ConnectorError::Request(err.to_string()))?;

        Ok(NotificationDeliveryReport {
            sink: self.kind(),
            delivered: true,
            destination: Some(channel.to_string()),
            detail: Some("sent via Slack workflow".to_string()),
        })
    }
}

async fn post_workflow_webhook_message(
    http: &Client,
    webhook_url: &str,
    channel: &str,
    title: &str,
    text: &str,
) -> Result<Option<String>> {
    if title.trim().is_empty() {
        bail!("Slack workflow title must not be empty");
    }

    if text.trim().is_empty() {
        bail!("Slack workflow message must not be empty");
    }

    let response = http
        .post(webhook_url)
        .json(&SlackWorkflowWebhookRequest {
            mr_milchick_talks_to: channel,
            mr_milchick_says: title,
            mr_milchick_says_thread: text,
        })
        .send()
        .await
        .context("failed to send Slack workflow webhook request")?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read Slack workflow webhook response body")?;

    if !status.is_success() {
        if body.trim().is_empty() {
            bail!("Slack workflow webhook returned an error status");
        }
        bail!(
            "Slack workflow webhook returned an error status: {}",
            body.trim()
        );
    }

    let _ = body;
    Ok(None)
}

pub fn render_slack_workflow_title(notification: &NotificationMessage) -> String {
    if let Some(url) = extract_first_link(&notification.subject) {
        return format!("Review Needed {}", url);
    }

    format!(
        "Review Needed {}",
        simplify_slack_formatting(&notification.subject)
    )
}

pub fn render_slack_workflow_message(notification: &NotificationMessage) -> String {
    let mut lines = Vec::new();

    if let Some(title) = &notification.body.title {
        lines.push(simplify_slack_formatting(title));
    }

    for section in &notification.body.sections {
        match section {
            MessageSection::Paragraph(text) => lines.push(simplify_slack_formatting(text)),
            MessageSection::BulletList(items) => {
                for item in items {
                    lines.push(format!("- {}", simplify_slack_formatting(item)));
                }
            }
            MessageSection::KeyValueList(items) => {
                for (key, value) in items {
                    lines.push(format!(
                        "{}: {}",
                        simplify_slack_formatting(key),
                        simplify_slack_formatting(value)
                    ));
                }
            }
            MessageSection::CodeBlock { content, .. } => lines.push(content.clone()),
        }
    }

    if let Some(footer) = &notification.body.footer {
        lines.push(simplify_slack_formatting(footer));
    }

    lines.join("\n")
}

fn extract_first_link(text: &str) -> Option<String> {
    let start = text.find('<')?;
    let remainder = &text[start + 1..];
    let end = remainder.find('>')?;
    let inner = &remainder[..end];
    let url = inner.split('|').next()?.trim();

    (!url.is_empty()).then(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use milchick_core::model::{NotificationAudience, NotificationSeverity, RenderedMessage};

    #[test]
    fn renders_plain_slack_workflow_message() {
        let message = NotificationMessage {
            subject: "Review requested".to_string(),
            body: RenderedMessage {
                title: Some("*MR #12*".to_string()),
                sections: vec![MessageSection::Paragraph(
                    "_Assign reviewers_ *@alice*".to_string(),
                )],
                footer: Some("<https://example.test|Open MR>".to_string()),
            },
            audience: NotificationAudience::Default,
            severity: NotificationSeverity::Info,
        };

        let rendered = render_slack_workflow_message(&message);
        assert!(rendered.contains("MR #12"));
        assert!(rendered.contains("Assign reviewers @alice"));
        assert!(rendered.contains("Open MR (https://example.test)"));
        assert!(!rendered.contains('*'));
        assert!(!rendered.contains('<'));
    }

    #[test]
    fn renders_minimal_workflow_title_from_linked_subject() {
        let message = NotificationMessage {
            subject:
                ":gitlab: Reviews Needed for <https://example.test/mr/1|MR #1>, by @arthur :pepe-review:"
                    .to_string(),
            body: RenderedMessage::new(None),
            audience: NotificationAudience::Default,
            severity: NotificationSeverity::Info,
        };

        let rendered = render_slack_workflow_title(&message);
        assert_eq!(rendered, "Review Needed https://example.test/mr/1");
    }
}
