use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tracing::{debug, warn};

use crate::core::model::{
    NotificationAudience, NotificationDelivery, NotificationDeliveryReport, NotificationMessage,
    NotificationSinkKind, ReviewCommit,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<SlackMessageMetadata<'a>>,
}

#[derive(Debug, Serialize)]
struct SlackUpdateMessageRequest<'a> {
    channel: &'a str,
    ts: &'a str,
    text: &'a str,
    metadata: SlackMessageMetadata<'a>,
}

#[derive(Debug, Serialize)]
struct SlackMessageMetadata<'a> {
    event_type: &'static str,
    event_payload: SlackEventPayload<'a>,
}

#[derive(Debug, Serialize)]
struct SlackEventPayload<'a> {
    conversation_key: &'a str,
    message_key: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit_ids: Option<&'a [String]>,
}

#[derive(Debug, Deserialize)]
struct SlackPostMessageResponse {
    ok: bool,
    error: Option<String>,
    channel: Option<String>,
    ts: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackConversationsHistoryResponse {
    ok: bool,
    error: Option<String>,
    #[serde(default)]
    messages: Vec<SlackConversationMessage>,
    response_metadata: Option<SlackResponseMetadata>,
}

#[derive(Debug, Deserialize)]
struct SlackConversationMessage {
    text: Option<String>,
    ts: Option<String>,
    thread_ts: Option<String>,
    metadata: Option<SlackConversationMetadata>,
}

#[derive(Debug, Deserialize)]
struct SlackConversationMetadata {
    event_type: Option<String>,
    event_payload: Option<SlackConversationEventPayload>,
}

#[derive(Debug, Deserialize)]
struct SlackConversationEventPayload {
    conversation_key: Option<String>,
    message_key: Option<String>,
    #[serde(default)]
    commit_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SlackResponseMetadata {
    next_cursor: Option<String>,
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
        let base_url = self.config.base_url.trim_end_matches('/');
        let post_message_endpoint = format!("{base_url}/chat.postMessage");
        let update_message_endpoint = format!("{base_url}/chat.update");
        let conversation_key = notification
            .conversation_key
            .as_deref()
            .or(notification.thread_key.as_deref())
            .unwrap_or("mr-milchick");

        if notification.prefer_thread_reply {
            if let Some(thread_key) = notification.thread_key.as_deref() {
                match find_existing_thread_root_ts(
                    &self.http,
                    &format!("{base_url}/conversations.history"),
                    Some(bot_token),
                    channel,
                    thread_key,
                    conversation_key,
                )
                .await
                {
                    Ok(Some(root_ts)) => {
                        return deliver_thread_notification(
                            &self.http,
                            &post_message_endpoint,
                            &update_message_endpoint,
                            &format!("{base_url}/conversations.replies"),
                            Some(bot_token),
                            channel,
                            &root_ts,
                            conversation_key,
                            notification,
                            &subject,
                            &text,
                        )
                        .await
                        .map_err(|err| ConnectorError::Request(err.to_string()));
                    }
                    Ok(None) => {}
                    Err(err) => warn!(
                        channel,
                        thread_key,
                        error = %err,
                        "failed to resolve existing Slack thread; falling back to root post"
                    ),
                }
            }
        }

        let root_ts = post_message(
            &self.http,
            &post_message_endpoint,
            Some(bot_token),
            channel,
            &subject,
            None,
            Some(message_metadata(conversation_key, "root", None)),
        )
        .await
        .map_err(|err| ConnectorError::Request(err.to_string()))?;

        if let Some(root_ts) = root_ts.as_deref() {
            return deliver_thread_notification(
                &self.http,
                &post_message_endpoint,
                &update_message_endpoint,
                &format!("{base_url}/conversations.replies"),
                Some(bot_token),
                channel,
                root_ts,
                conversation_key,
                notification,
                &subject,
                &text,
            )
            .await
            .map_err(|err| ConnectorError::Request(err.to_string()));
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
    metadata: Option<SlackMessageMetadata<'_>>,
) -> Result<Option<String>> {
    if text.trim().is_empty() {
        bail!("Slack message must not be empty");
    }

    let mut request = http.post(endpoint).json(&SlackPostMessageRequest {
        channel,
        text,
        thread_ts,
        metadata,
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

#[allow(clippy::too_many_arguments)]
async fn deliver_thread_notification(
    http: &Client,
    post_endpoint: &str,
    update_endpoint: &str,
    replies_endpoint: &str,
    bearer_token: Option<&str>,
    channel: &str,
    root_ts: &str,
    conversation_key: &str,
    notification: &NotificationMessage,
    subject: &str,
    text: &str,
) -> ConnectorResult<NotificationDeliveryReport> {
    let reply_text = if text.trim().is_empty() {
        subject
    } else {
        text
    };
    let messages = match &notification.delivery {
        NotificationDelivery::Append => Vec::new(),
        NotificationDelivery::UpsertThreadReply { .. }
        | NotificationDelivery::AppendCommitLog { .. } => {
            load_thread_messages(http, replies_endpoint, bearer_token, channel, root_ts)
                .await
                .map_err(|err| ConnectorError::Request(err.to_string()))?
        }
    };
    debug!(
        channel,
        root_ts,
        conversation_key,
        message_count = messages.len(),
        managed_metadata_count = messages
            .iter()
            .filter(|message| message
                .metadata
                .as_ref()
                .is_some_and(|metadata| metadata.event_type.as_deref() == Some("mr_milchick")))
            .count(),
        "loaded Slack thread messages for managed delivery"
    );

    let (delivered, destination, detail) = match &notification.delivery {
        NotificationDelivery::Append => {
            let ts = post_message(
                http,
                post_endpoint,
                bearer_token,
                channel,
                reply_text,
                Some(root_ts),
                None,
            )
            .await
            .map_err(|err| ConnectorError::Request(err.to_string()))?;
            (true, ts.or_else(|| Some(root_ts.to_string())), "sent")
        }
        NotificationDelivery::UpsertThreadReply { slot } => {
            let message_key = format!("slot:{slot}");
            if let Some(existing_ts) = find_message_ts(&messages, conversation_key, &message_key) {
                debug!(
                    channel,
                    root_ts,
                    conversation_key,
                    message_key,
                    existing_ts,
                    "updating existing managed Slack reply"
                );
                update_message(
                    http,
                    update_endpoint,
                    bearer_token,
                    channel,
                    existing_ts,
                    reply_text,
                    message_metadata(conversation_key, &message_key, None),
                )
                .await
                .map_err(|err| ConnectorError::Request(err.to_string()))?;
                (true, Some(existing_ts.to_string()), "updated")
            } else {
                debug!(
                    channel,
                    root_ts,
                    conversation_key,
                    message_key,
                    "managed Slack reply was not found; creating it"
                );
                let ts = post_message(
                    http,
                    post_endpoint,
                    bearer_token,
                    channel,
                    reply_text,
                    Some(root_ts),
                    Some(message_metadata(conversation_key, &message_key, None)),
                )
                .await
                .map_err(|err| ConnectorError::Request(err.to_string()))?;
                (true, ts.or_else(|| Some(root_ts.to_string())), "sent")
            }
        }
        NotificationDelivery::AppendCommitLog {
            commits,
            ci_summary,
        } => {
            let known_ids = known_commit_ids(&messages, conversation_key);
            let new_commits = select_new_commits(commits, &known_ids);
            debug!(
                channel,
                root_ts,
                conversation_key,
                commit_count = commits.len(),
                known_commit_count = known_ids.len(),
                new_commit_count = new_commits.len(),
                head_commit_id = commits.last().map(|commit| commit.id.as_str()),
                "selected commits for Slack activity line"
            );
            if new_commits.is_empty() {
                (false, Some(root_ts.to_string()), "already present")
            } else {
                let commit_ids = new_commits
                    .iter()
                    .map(|commit| commit.id.clone())
                    .collect::<Vec<_>>();
                let message_key = format!(
                    "commits:{}",
                    commit_ids.last().map(String::as_str).unwrap_or_default()
                );
                let commit_text = render_commit_log_message(ci_summary, &new_commits);
                let ts = post_message(
                    http,
                    post_endpoint,
                    bearer_token,
                    channel,
                    &commit_text,
                    Some(root_ts),
                    Some(message_metadata(
                        conversation_key,
                        &message_key,
                        Some(&commit_ids),
                    )),
                )
                .await
                .map_err(|err| ConnectorError::Request(err.to_string()))?;
                (true, ts.or_else(|| Some(root_ts.to_string())), "sent")
            }
        }
    };

    Ok(NotificationDeliveryReport {
        sink: NotificationSinkKind::SlackApp,
        delivered,
        destination,
        detail: Some(detail.to_string()),
    })
}

fn message_metadata<'a>(
    conversation_key: &'a str,
    message_key: &'a str,
    commit_ids: Option<&'a [String]>,
) -> SlackMessageMetadata<'a> {
    SlackMessageMetadata {
        event_type: "mr_milchick",
        event_payload: SlackEventPayload {
            conversation_key,
            message_key,
            commit_ids,
        },
    }
}

async fn update_message(
    http: &Client,
    endpoint: &str,
    bearer_token: Option<&str>,
    channel: &str,
    ts: &str,
    text: &str,
    metadata: SlackMessageMetadata<'_>,
) -> Result<()> {
    let mut request = http.post(endpoint).json(&SlackUpdateMessageRequest {
        channel,
        ts,
        text,
        metadata,
    });
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }

    let payload = request
        .send()
        .await
        .context("failed to send Slack chat.update request")?
        .error_for_status()
        .context("Slack chat.update returned an error status")?
        .json::<SlackPostMessageResponse>()
        .await
        .context("failed to deserialize Slack chat.update response")?;

    if !payload.ok {
        bail!(
            "Slack chat.update failed: {}",
            payload.error.unwrap_or_else(|| "unknown error".to_string())
        );
    }
    Ok(())
}

async fn load_thread_messages(
    http: &Client,
    endpoint: &str,
    bearer_token: Option<&str>,
    channel: &str,
    root_ts: &str,
) -> Result<Vec<SlackConversationMessage>> {
    let mut cursor: Option<String> = None;
    let mut messages = Vec::new();

    loop {
        let mut request = http.get(endpoint).query(&[
            ("channel", channel),
            ("ts", root_ts),
            ("limit", "200"),
            ("include_all_metadata", "true"),
        ]);
        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }
        if let Some(next_cursor) = cursor.as_deref() {
            request = request.query(&[("cursor", next_cursor)]);
        }

        let payload = request
            .send()
            .await
            .context("failed to send Slack conversations.replies request")?
            .error_for_status()
            .context("Slack conversations.replies returned an error status")?
            .json::<SlackConversationsHistoryResponse>()
            .await
            .context("failed to deserialize Slack conversations.replies response")?;
        if !payload.ok {
            bail!(
                "Slack conversations.replies failed: {}",
                payload.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }
        messages.extend(payload.messages);
        let next_cursor = payload
            .response_metadata
            .and_then(|metadata| metadata.next_cursor)
            .filter(|value| !value.trim().is_empty());
        if next_cursor.is_none() {
            return Ok(messages);
        }
        cursor = next_cursor;
    }
}

fn find_message_ts<'a>(
    messages: &'a [SlackConversationMessage],
    conversation_key: &str,
    message_key: &str,
) -> Option<&'a str> {
    messages.iter().find_map(|message| {
        let metadata = message.metadata.as_ref()?;
        let payload = metadata.event_payload.as_ref()?;
        (metadata.event_type.as_deref() == Some("mr_milchick")
            && payload.conversation_key.as_deref() == Some(conversation_key)
            && payload.message_key.as_deref() == Some(message_key))
        .then_some(message.ts.as_deref())
        .flatten()
    })
}

fn known_commit_ids(
    messages: &[SlackConversationMessage],
    conversation_key: &str,
) -> BTreeSet<String> {
    messages
        .iter()
        .filter_map(|message| message.metadata.as_ref())
        .filter(|metadata| metadata.event_type.as_deref() == Some("mr_milchick"))
        .filter_map(|metadata| metadata.event_payload.as_ref())
        .filter(|payload| payload.conversation_key.as_deref() == Some(conversation_key))
        .flat_map(|payload| payload.commit_ids.iter().cloned())
        .collect()
}

fn select_new_commits<'a>(
    commits: &'a [ReviewCommit],
    known_ids: &BTreeSet<String>,
) -> Vec<&'a ReviewCommit> {
    if commits.is_empty() {
        return Vec::new();
    }
    if known_ids.is_empty() {
        return commits.last().into_iter().collect();
    }

    let last_known = commits
        .iter()
        .rposition(|commit| known_ids.contains(&commit.id));
    match last_known {
        Some(index) => commits[index + 1..].iter().collect(),
        None => commits.last().into_iter().collect(),
    }
}

fn render_commit_log_message(ci_summary: &str, commits: &[&ReviewCommit]) -> String {
    std::iter::once(ci_summary.trim().to_string())
        .chain(commits.iter().map(|commit| {
            let short_id = commit.id.chars().take(8).collect::<String>();
            let title = escape_slack_text(&commit.title);
            format!("• <{}|`{}` {}>", commit.web_url, short_id, title)
        }))
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_slack_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

async fn slack_response_or_error(response: Response, operation: &str) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status().to_string();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response
        .text()
        .await
        .unwrap_or_else(|error| format!("<unable to read Slack error response: {error}>"));

    bail!(
        "{}",
        format_slack_http_error(operation, &status, retry_after.as_deref(), &body)
    );
}

fn format_slack_http_error(
    operation: &str,
    status: &str,
    retry_after: Option<&str>,
    body: &str,
) -> String {
    const MAX_ERROR_BODY_CHARS: usize = 512;

    let body = body.trim();
    let body = if body.chars().count() > MAX_ERROR_BODY_CHARS {
        let prefix = body.chars().take(MAX_ERROR_BODY_CHARS).collect::<String>();
        format!("{prefix}…")
    } else {
        body.to_string()
    };
    let retry_after = retry_after
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("; retry_after={}s", value.trim()))
        .unwrap_or_default();

    format!("Slack {operation} returned HTTP {status}{retry_after}; body={body}")
}

async fn find_existing_thread_root_ts(
    http: &Client,
    endpoint: &str,
    bearer_token: Option<&str>,
    channel: &str,
    thread_key: &str,
    conversation_key: &str,
) -> Result<Option<String>> {
    let mut cursor: Option<String> = None;
    let mut fallback_match: Option<String> = None;

    loop {
        let mut request = http.get(endpoint).query(&[
            ("channel", channel),
            ("limit", "200"),
            ("include_all_metadata", "true"),
        ]);

        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }

        if let Some(next_cursor) = cursor.as_deref() {
            request = request.query(&[("cursor", next_cursor)]);
        }

        let response = request
            .send()
            .await
            .context("failed to send Slack conversations.history request")?;

        let response = slack_response_or_error(response, "conversations.history").await?;

        let payload = response
            .json::<SlackConversationsHistoryResponse>()
            .await
            .context("failed to deserialize Slack conversations.history response")?;

        if !payload.ok {
            bail!(
                "Slack conversations.history failed: {}",
                payload.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        for message in payload.messages {
            let Some(ts) = message.ts.as_deref() else {
                continue;
            };

            if !is_root_message(&message, ts) {
                continue;
            }

            let text = message.text.as_deref().unwrap_or_default();
            let managed_payload = message
                .metadata
                .as_ref()
                .filter(|metadata| metadata.event_type.as_deref() == Some("mr_milchick"))
                .and_then(|metadata| metadata.event_payload.as_ref());
            if let Some(payload) = managed_payload {
                if payload.conversation_key.as_deref() != Some(conversation_key)
                    || payload.message_key.as_deref() != Some("root")
                {
                    continue;
                }
            } else if !text.contains(thread_key) {
                continue;
            }

            fallback_match = Some(ts.to_string());
            if is_first_look_message(text) {
                return Ok(Some(ts.to_string()));
            }
        }

        let next_cursor = payload
            .response_metadata
            .and_then(|metadata| metadata.next_cursor)
            .filter(|value| !value.trim().is_empty());

        if next_cursor.is_none() {
            return Ok(fallback_match);
        }

        cursor = next_cursor;
    }
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

fn is_root_message(message: &SlackConversationMessage, ts: &str) -> bool {
    message
        .thread_ts
        .as_deref()
        .is_none_or(|thread_ts| thread_ts == ts)
}

fn is_first_look_message(text: &str) -> bool {
    text.to_ascii_lowercase().contains("first look")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{NotificationDelivery, NotificationSeverity, NotificationSinkKind};

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
            thread_key: None,
            prefer_thread_reply: false,
            conversation_key: None,
            delivery: NotificationDelivery::Append,
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

    #[test]
    fn identifies_root_messages_and_prefers_first_look_subjects() {
        let reply = SlackConversationMessage {
            text: Some("Mr. Milchick - updates on MR #3995".to_string()),
            ts: Some("1700000000.000003".to_string()),
            thread_ts: Some("1700000000.000001".to_string()),
            metadata: None,
        };
        let update_root = SlackConversationMessage {
            text: Some("Mr. Milchick - updates on MR #3995".to_string()),
            ts: Some("1700000000.000002".to_string()),
            thread_ts: None,
            metadata: None,
        };
        let first_root = SlackConversationMessage {
            text: Some("Mr. Milchick took a first look at MR #3995".to_string()),
            ts: Some("1700000000.000001".to_string()),
            thread_ts: None,
            metadata: None,
        };

        assert!(!is_root_message(&reply, "1700000000.000003"));
        assert!(is_root_message(&update_root, "1700000000.000002"));
        assert!(is_first_look_message(
            first_root.text.as_deref().unwrap_or_default()
        ));
        assert!(!is_first_look_message(
            update_root.text.as_deref().unwrap_or_default()
        ));
    }

    #[test]
    fn formats_rate_limit_errors_with_retry_guidance_and_a_bounded_body() {
        let rendered = format_slack_http_error(
            "conversations.history",
            "429 Too Many Requests",
            Some("60"),
            r#"{"ok":false,"error":"ratelimited"}"#,
        );

        assert_eq!(
            rendered,
            "Slack conversations.history returned HTTP 429 Too Many Requests; retry_after=60s; body={\"ok\":false,\"error\":\"ratelimited\"}"
        );
    }

    #[test]
    fn first_commit_log_records_only_the_current_head() {
        let commits = sample_commits();

        let selected = select_new_commits(&commits, &BTreeSet::new());

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "3333333333333333");
    }

    #[test]
    fn later_commit_log_records_every_commit_after_the_last_known_one() {
        let commits = sample_commits();
        let known = BTreeSet::from(["1111111111111111".to_string()]);

        let selected = select_new_commits(&commits, &known);

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].id, "2222222222222222");
        assert_eq!(selected[1].id, "3333333333333333");
    }

    #[test]
    fn renders_commit_and_ci_update_as_a_bulleted_list() {
        let commits = sample_commits();
        let selected = vec![&commits[1], &commits[2]];

        let rendered = render_commit_log_message("🟢 CI: 18/18 passed", &selected);

        assert_eq!(rendered.lines().count(), 3);
        assert_eq!(rendered.lines().next(), Some("🟢 CI: 18/18 passed"));
        assert!(rendered.contains("• <https://example.test/2|`22222222` Second commit>"));
        assert!(rendered.contains("• <https://example.test/3|`33333333` Third commit>"));
    }

    fn sample_commits() -> Vec<ReviewCommit> {
        vec![
            ReviewCommit {
                id: "1111111111111111".to_string(),
                title: "First commit".to_string(),
                web_url: "https://example.test/1".to_string(),
            },
            ReviewCommit {
                id: "2222222222222222".to_string(),
                title: "Second commit".to_string(),
                web_url: "https://example.test/2".to_string(),
            },
            ReviewCommit {
                id: "3333333333333333".to_string(),
                title: "Third commit".to_string(),
                web_url: "https://example.test/3".to_string(),
            },
        ]
    }
}
