use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use milchick_core::model::{
    MessageSection, NotificationAudience, NotificationDeliveryReport, NotificationMessage,
    NotificationSinkKind,
};
use milchick_runtime::{ConnectorError, ConnectorResult, NotificationSink};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackAppConfig {
    pub enabled: bool,
    pub base_url: String,
    pub bot_token: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SlackAppSink {
    http: Client,
    config: SlackAppConfig,
}

#[derive(Debug, Serialize)]
struct SlackPostMessageRequest<'a> {
    channel: &'a str,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_ts: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct SlackPostMessageResponse {
    ok: bool,
    error: Option<String>,
    channel: Option<String>,
    ts: Option<String>,
}

impl SlackAppSink {
    pub fn new(config: SlackAppConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.bot_token.is_some() && self.config.channel.is_some()
    }
}

#[async_trait]
impl NotificationSink for SlackAppSink {
    fn kind(&self) -> NotificationSinkKind {
        NotificationSinkKind::SlackApp
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

        let bot_token =
            self.config.bot_token.as_deref().ok_or_else(|| {
                ConnectorError::Configuration("missing Slack bot token".to_string())
            })?;

        let channel = match &notification.audience {
            NotificationAudience::Default => self.config.channel.as_deref().ok_or_else(|| {
                ConnectorError::Configuration("missing Slack channel".to_string())
            })?,
            NotificationAudience::Channel(channel) => channel.as_str(),
            NotificationAudience::User(user) | NotificationAudience::Group(user) => user.as_str(),
        };

        let text = render_slack_app_message(notification);
        let root_ts = post_message(
            &self.http,
            &format!(
                "{}/chat.postMessage",
                self.config.base_url.trim_end_matches('/')
            ),
            Some(bot_token),
            channel,
            &notification.subject,
            None,
        )
        .await
        .map_err(|err| ConnectorError::Request(err.to_string()))?;

        if !text.trim().is_empty() {
            post_message(
                &self.http,
                &format!(
                    "{}/chat.postMessage",
                    self.config.base_url.trim_end_matches('/')
                ),
                Some(bot_token),
                channel,
                &text,
                root_ts.as_deref(),
            )
            .await
            .map_err(|err| ConnectorError::Request(err.to_string()))?;
        }

        Ok(NotificationDeliveryReport {
            sink: self.kind(),
            delivered: true,
            destination: root_ts,
            detail: Some("sent".to_string()),
        })
    }
}

async fn post_message(
    http: &Client,
    endpoint: &str,
    bearer_token: Option<&str>,
    channel: &str,
    text: &str,
    thread_ts: Option<&str>,
) -> Result<Option<String>> {
    if text.trim().is_empty() {
        bail!("Slack message must not be empty");
    }

    let mut request = http.post(endpoint).json(&SlackPostMessageRequest {
        channel,
        text,
        thread_ts,
    });

    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }

    let response = request
        .send()
        .await
        .context("failed to send Slack chat.postMessage request")?;

    let response = response
        .error_for_status()
        .context("Slack chat.postMessage returned an error status")?;

    let payload = response
        .json::<SlackPostMessageResponse>()
        .await
        .context("failed to deserialize Slack chat.postMessage response")?;

    if !payload.ok {
        bail!(
            "Slack chat.postMessage failed: {}",
            payload.error.unwrap_or_else(|| "unknown error".to_string())
        );
    }

    Ok(payload.ts.or(payload.channel))
}

pub fn render_slack_app_message(notification: &NotificationMessage) -> String {
    let mut lines = Vec::new();

    if let Some(title) = &notification.body.title {
        lines.push(format!("*{}*", title));
    }

    for section in &notification.body.sections {
        match section {
            MessageSection::Paragraph(text) => lines.push(text.clone()),
            MessageSection::BulletList(items) => {
                for item in items {
                    lines.push(format!("• {}", item));
                }
            }
            MessageSection::KeyValueList(items) => {
                for (key, value) in items {
                    lines.push(format!("*{}*: {}", key, value));
                }
            }
            MessageSection::CodeBlock { content, .. } => {
                lines.push(format!("```{}```", content));
            }
        }
    }

    if let Some(footer) = &notification.body.footer {
        lines.push(format!("_{}_", footer));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use milchick_core::model::{NotificationSeverity, RenderedMessage};

    #[test]
    fn renders_structured_slack_app_message() {
        let message = NotificationMessage {
            subject: "Review requested".to_string(),
            body: RenderedMessage {
                title: Some("MR #12".to_string()),
                sections: vec![MessageSection::BulletList(vec!["@alice".to_string()])],
                footer: Some("Kind regards.".to_string()),
            },
            audience: NotificationAudience::Default,
            severity: NotificationSeverity::Info,
        };

        let rendered = render_slack_app_message(&message);
        assert!(rendered.contains("*MR #12*"));
        assert!(rendered.contains("MR #12"));
        assert!(rendered.contains("• @alice"));
        assert!(rendered.contains("_Kind regards._"));
    }
}
