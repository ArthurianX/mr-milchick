use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;

use crate::core::model::{
    NotificationAudience, NotificationDeliveryReport, NotificationMessage, NotificationSinkKind,
};
use crate::runtime::{ConnectorError, ConnectorResult, NotificationSink};

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

        let title = notification.subject.clone();
        let thread = notification.body.clone();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{NotificationAudience, NotificationSeverity, NotificationSinkKind};

    #[test]
    fn preserves_plain_workflow_message_payload() {
        let message = NotificationMessage {
            sink: NotificationSinkKind::SlackWorkflow,
            subject: "Review requested".to_string(),
            body: "MR #12\nAssign reviewers @alice\nOpen MR (https://example.test)".to_string(),
            audience: NotificationAudience::Default,
            severity: NotificationSeverity::Info,
        };

        let rendered = message.body.clone();
        assert!(rendered.contains("MR #12"));
        assert!(rendered.contains("Assign reviewers @alice"));
        assert!(rendered.contains("Open MR (https://example.test)"));
        assert!(!rendered.contains('*'));
        assert!(!rendered.contains('<'));
    }
}
