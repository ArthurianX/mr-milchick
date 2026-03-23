use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::model::SlackConfig;

#[derive(Debug, Clone)]
pub struct SlackNotifier {
    http: Client,
    config: SlackConfig,
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
    ts: Option<String>,
}

impl SlackNotifier {
    pub fn new(config: SlackConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    #[cfg(test)]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.bot_token.is_some() && self.config.channel.is_some()
    }

    pub async fn send_review_request(&self, summary: &str, thread: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let bot_token = self
            .config
            .bot_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing Slack bot token"))?;
        let channel = self
            .config
            .channel
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing Slack channel"))?;

        if summary.trim().is_empty() {
            bail!("Slack review request summary must not be empty");
        }

        if thread.trim().is_empty() {
            bail!("Slack review request thread message must not be empty");
        }

        let root = self
            .post_message(bot_token, channel, summary, None)
            .await
            .context("failed to send top-level Slack review request")?;

        self.post_message(bot_token, channel, thread, Some(root.as_str()))
            .await
            .context("failed to send threaded Slack review request")?;

        Ok(())
    }

    async fn post_message(
        &self,
        bot_token: &str,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<String> {
        let response = self
            .http
            .post(self.method_url("chat.postMessage"))
            .bearer_auth(bot_token)
            .json(&SlackPostMessageRequest {
                channel,
                text,
                thread_ts,
            })
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

        payload
            .ts
            .ok_or_else(|| anyhow::anyhow!("Slack chat.postMessage did not return a ts"))
    }
    fn method_url(&self, method: &str) -> String {
        format!("{}/{}", self.config.base_url.trim_end_matches('/'), method)
    }
}

pub fn render_review_request_summary(iid: u64, web_url: &str, author_username: &str) -> String {
    format!(
        ":gitlab: Reviews Needed for <{}|MR #{}>, by @{} :pepe-review:",
        web_url, iid, author_username
    )
}

pub fn render_review_request_thread(
    tone_line: &str,
    title: &str,
    web_url: &str,
    reviewers: &[String],
) -> String {
    let reviewers_text = if reviewers.is_empty() {
        "_Assign reviewers_ already covered.".to_string()
    } else {
        let formatted_reviewers = reviewers
            .iter()
            .map(|reviewer| format!("*{}*", reviewer))
            .collect::<Vec<_>>()
            .join(" ");
        format!("_Assign reviewers_ {}", formatted_reviewers)
    };

    format!("*{}*\nReview requested for: <{}|{}>\n{}", tone_line, web_url, title, reviewers_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_review_request_summary_line() {
        let message = render_review_request_summary(
            4048,
            "https://gitlab.example.com/group/project/-/merge_requests/1",
            "arthur",
        );

        assert_eq!(
            message,
            ":gitlab: Reviews Needed for <https://gitlab.example.com/group/project/-/merge_requests/1|MR #4048>, by @arthur :pepe-review:"
        );
    }

    #[test]
    fn renders_review_request_thread_with_reviewers() {
        let message = render_review_request_thread(
            "The department has a request.",
            "Improve branch policy",
            "https://gitlab.example.com/group/project/-/merge_requests/1",
            &["@alice".to_string(), "@bob".to_string()],
        );

        assert!(message.contains("*The department has a request.*"));
        assert!(message.contains("Review requested for: <https://gitlab.example.com/group/project/-/merge_requests/1|Improve branch policy>"));
        assert!(message.contains("_Assign reviewers_ *@alice* *@bob*"));
    }

    #[test]
    fn notifier_requires_complete_enabled_config() {
        let notifier = SlackNotifier::new(SlackConfig {
            enabled: true,
            base_url: "https://slack.com/api".to_string(),
            bot_token: Some("xoxb-test".to_string()),
            channel: None,
        });

        assert!(!notifier.is_enabled());
    }

    #[test]
    fn summary_prefers_mrkdwn_link() {
        let message = render_review_request_summary(
            3995,
            "https://gitlab.example.com/group/project/-/merge_requests/3995",
            "alice",
        );

        assert!(message.contains(
            "<https://gitlab.example.com/group/project/-/merge_requests/3995|MR #3995>"
        ));
    }
}
