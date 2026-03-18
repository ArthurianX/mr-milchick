use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Serialize;

use crate::config::model::SlackConfig;

#[derive(Debug, Clone)]
pub struct SlackNotifier {
    http: Client,
    config: SlackConfig,
}

#[derive(Debug, Serialize)]
struct SlackWorkflowPayload<'a> {
    mr_milchick_talks_to: &'a str,
    mr_milchick_says: &'a str,
    mr_milchick_says_thread: &'a str,
}

impl SlackNotifier {
    pub fn new(config: SlackConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.config.webhook_url.is_some() && self.config.channel.is_some()
    }

    pub async fn send_review_request(&self, summary: &str, thread: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let webhook_url = self
            .config
            .webhook_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing Slack webhook URL"))?;
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

        self.http
            .post(webhook_url)
            .json(&SlackWorkflowPayload {
                mr_milchick_talks_to: channel,
                mr_milchick_says: summary,
                mr_milchick_says_thread: thread,
            })
            .send()
            .await
            .context("failed to send Slack review request")?
            .error_for_status()
            .context("Slack workflow returned an error status")?;

        Ok(())
    }
}

pub fn render_review_request_summary(title: &str, web_url: &str) -> String {
    format!(":gitlab: :noted2: Reviews Needed : {} > :thread:", web_url)
}

pub fn render_review_request_thread(
    tone_line: &str,
    title: &str,
    web_url: &str,
    reviewers: &[String],
) -> String {
    let reviewers_text = if reviewers.is_empty() {
        "Assigned reviewers are already in position.".to_string()
    } else {
        format!("Assigned reviewers: {}.", reviewers.join(", "))
    };

    format!(
        "{}\n\nReview requested for:\n{}\n{}\n\n{}",
        tone_line, title, web_url, reviewers_text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_review_request_summary_line() {
        let message = render_review_request_summary(
            "Improve branch policy",
            "https://gitlab.example.com/group/project/-/merge_requests/1",
        );

        assert_eq!(
            message,
            ":gitlab: :noted2: Reviews Needed : https://gitlab.example.com/group/project/-/merge_requests/1 > :thread:"
        );
    }

    #[test]
    fn renders_review_request_thread_with_reviewers() {
        let message = render_review_request_thread(
            "The department has a request.",
            "Improve branch policy",
            "https://gitlab.example.com/group/project/-/merge_requests/1",
            &["alice".to_string(), "bob".to_string()],
        );

        assert!(message.contains("The department has a request."));
        assert!(message.contains("Review requested for:\nImprove branch policy"));
        assert!(message.contains("https://gitlab.example.com/group/project/-/merge_requests/1"));
        assert!(message.contains("Assigned reviewers: alice, bob."));
    }

    #[test]
    fn notifier_requires_complete_enabled_config() {
        let notifier = SlackNotifier::new(SlackConfig {
            enabled: true,
            webhook_url: Some("https://hooks.slack.com/triggers/example".to_string()),
            channel: None,
        });

        assert!(!notifier.is_enabled());
    }
}
