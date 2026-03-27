use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::core::model::{
    NotificationAudience, NotificationDeliveryReport, NotificationMessage, NotificationSinkKind,
};
use crate::runtime::{ConnectorError, ConnectorResult, NotificationSink};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackAppConfig {
    pub enabled: bool,
    pub base_url: String,
    pub bot_token: Option<String>,
    pub channel: Option<String>,
    pub user_map: BTreeMap<String, String>,
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

        let subject = replace_gitlab_mentions(&notification.subject, &self.config.user_map);
        let text = replace_gitlab_mentions(&notification.body, &self.config.user_map);
        let root_ts = post_message(
            &self.http,
            &format!(
                "{}/chat.postMessage",
                self.config.base_url.trim_end_matches('/')
            ),
            Some(bot_token),
            channel,
            &subject,
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

fn replace_gitlab_mentions(text: &str, user_map: &BTreeMap<String, String>) -> String {
    let mut rendered = String::with_capacity(text.len());
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '@' {
            rendered.push(chars[index]);
            index += 1;
            continue;
        }

        let mention_start = index + 1;
        let mut mention_end = mention_start;
        while mention_end < chars.len() && is_gitlab_mention_char(chars[mention_end]) {
            mention_end += 1;
        }

        if mention_end == mention_start {
            rendered.push('@');
            index += 1;
            continue;
        }

        let username = chars[mention_start..mention_end].iter().collect::<String>();
        if let Some(slack_user_id) = normalize_slack_user_id(user_map.get(&username)) {
            rendered.push_str("<@");
            rendered.push_str(slack_user_id);
            rendered.push('>');
        } else {
            rendered.push('@');
            rendered.push_str(&username);
        }

        index = mention_end;
    }

    rendered
}

fn normalize_slack_user_id(slack_user_id: Option<&String>) -> Option<&str> {
    let slack_user_id = slack_user_id?.trim();
    let slack_user_id = slack_user_id
        .strip_prefix("<@")
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or(slack_user_id);

    is_valid_slack_user_id(slack_user_id).then_some(slack_user_id)
}

fn is_valid_slack_user_id(slack_user_id: &str) -> bool {
    let mut chars = slack_user_id.chars();
    matches!(chars.next(), Some('U' | 'W'))
        && chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

fn is_gitlab_mention_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{NotificationSeverity, NotificationSinkKind};

    #[test]
    fn rewrites_mentions_in_rendered_slack_app_message() {
        let mut user_map = BTreeMap::new();
        user_map.insert("alice".to_string(), "U01234567".to_string());
        let message = NotificationMessage {
            sink: NotificationSinkKind::SlackApp,
            subject: "Review requested".to_string(),
            body: "*MR #12*\nHello @alice\n• @alice\n_Kind regards._".to_string(),
            audience: NotificationAudience::Default,
            severity: NotificationSeverity::Info,
        };

        let rendered = replace_gitlab_mentions(&message.body, &user_map);
        assert!(rendered.contains("*MR #12*"));
        assert!(rendered.contains("Hello <@U01234567>"));
        assert!(rendered.contains("• <@U01234567>"));
        assert!(rendered.contains("_Kind regards._"));
    }

    #[test]
    fn rewrites_known_gitlab_mentions_to_slack_user_mentions() {
        let mut user_map = BTreeMap::new();
        user_map.insert("alice".to_string(), "U01234567".to_string());

        let rendered = replace_gitlab_mentions("Assign @alice and @bob", &user_map);

        assert_eq!(rendered, "Assign <@U01234567> and @bob");
    }

    #[test]
    fn ignores_invalid_slack_user_ids() {
        let mut user_map = BTreeMap::new();
        user_map.insert("alice".to_string(), "not-a-slack-id".to_string());

        let rendered = replace_gitlab_mentions("Assign @alice", &user_map);

        assert_eq!(rendered, "Assign @alice");
    }
}
