use crate::config::NotificationPolicy;
use crate::core::inference::ReviewInferenceOutcome;
use crate::core::model::{
    NotificationDelivery, NotificationDeliveryReport, NotificationMessage, NotificationSinkKind,
    ReviewAction, ReviewActionKind, ReviewPlatformKind,
};
use anyhow::Result;

use crate::runtime::executor::{
    ExecutionReport, NotificationSink, PlatformConnector, ReviewInferenceConnector,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Observe,
    Refine,
    Explain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStrategy {
    DryRun,
    Real,
}

impl ExecutionStrategy {
    pub fn from_dry_run(dry_run: bool) -> Self {
        if dry_run { Self::DryRun } else { Self::Real }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCapabilities {
    pub platform_connector: ReviewPlatformKind,
    pub notification_sinks: Vec<NotificationSinkKind>,
    pub inference_available: bool,
}

pub struct RuntimeWiring {
    pub platform_connector: Box<dyn PlatformConnector>,
    pub inference_connector: Option<Box<dyn ReviewInferenceConnector>>,
    pub notification_sinks: Vec<Box<dyn NotificationSink>>,
    pub capabilities: RuntimeCapabilities,
}

impl RuntimeWiring {
    pub fn new(
        platform_connector: Box<dyn PlatformConnector>,
        inference_connector: Option<Box<dyn ReviewInferenceConnector>>,
        notification_sinks: Vec<Box<dyn NotificationSink>>,
    ) -> Self {
        let capabilities = RuntimeCapabilities {
            platform_connector: platform_connector.kind(),
            notification_sinks: notification_sinks.iter().map(|sink| sink.kind()).collect(),
            inference_available: inference_connector.is_some(),
        };

        Self {
            platform_connector,
            inference_connector,
            notification_sinks,
            capabilities,
        }
    }

    pub async fn analyze_review(
        &self,
        snapshot: &crate::core::model::ReviewSnapshot,
    ) -> Result<ReviewInferenceOutcome> {
        let connector = self.inference_connector.as_ref().ok_or_else(|| {
            anyhow::anyhow!("review inference is not wired for this execution mode")
        })?;

        Ok(connector.analyze(snapshot).await?)
    }

    pub async fn execute_review_actions(
        &self,
        strategy: ExecutionStrategy,
        review_actions: &[ReviewAction],
    ) -> Result<crate::core::model::ReviewActionReport> {
        Ok(if strategy == ExecutionStrategy::DryRun {
            dry_run_review_report(review_actions)
        } else {
            self.platform_connector
                .apply_review_actions(review_actions)
                .await?
        })
    }

    pub async fn deliver_notifications(
        &self,
        strategy: ExecutionStrategy,
        notification_policy: NotificationPolicy,
        notifications: &[NotificationMessage],
        review_report: &crate::core::model::ReviewActionReport,
    ) -> Result<Vec<NotificationDeliveryReport>> {
        let mut notification_reports = Vec::new();
        let notification_skip_reason =
            notification_skip_reason(notification_policy, strategy, review_report);

        for sink in &self.notification_sinks {
            for notification in notifications
                .iter()
                .filter(|notification| notification.sink == sink.kind())
            {
                let managed_slack_lifecycle = matches!(
                    notification.delivery,
                    NotificationDelivery::UpsertThreadReply { .. }
                        | NotificationDelivery::AppendCommitLog { .. }
                );
                let report = if let Some(reason) = notification_skip_reason
                    .as_ref()
                    .filter(|_| !managed_slack_lifecycle)
                {
                    crate::core::model::NotificationDeliveryReport {
                        sink: sink.kind(),
                        delivered: false,
                        destination: None,
                        detail: Some(reason.clone()),
                    }
                } else if strategy == ExecutionStrategy::DryRun {
                    crate::core::model::NotificationDeliveryReport {
                        sink: sink.kind(),
                        delivered: false,
                        destination: None,
                        detail: Some("dry-run".to_string()),
                    }
                } else {
                    match sink.send(notification).await {
                        Ok(report) => report,
                        Err(err) => crate::core::model::NotificationDeliveryReport {
                            sink: sink.kind(),
                            delivered: false,
                            destination: None,
                            detail: Some(err.to_string()),
                        },
                    }
                };
                notification_reports.push(report);
            }
        }

        Ok(notification_reports)
    }

    pub async fn execute(
        &self,
        strategy: ExecutionStrategy,
        notification_policy: NotificationPolicy,
        review_actions: &[ReviewAction],
        notifications: &[NotificationMessage],
    ) -> Result<ExecutionReport> {
        let review_report = self
            .execute_review_actions(strategy, review_actions)
            .await?;
        let notification_reports = self
            .deliver_notifications(strategy, notification_policy, notifications, &review_report)
            .await?;

        Ok(ExecutionReport {
            review_report,
            notification_reports,
        })
    }
}

fn dry_run_review_report(actions: &[ReviewAction]) -> crate::core::model::ReviewActionReport {
    let mut report = crate::core::model::ReviewActionReport::default();

    for action in actions {
        report
            .applied
            .push(crate::core::model::AppliedReviewAction {
                action: action.kind(),
                detail: Some("dry-run".to_string()),
            });
    }

    report
}

fn notification_skip_reason(
    notification_policy: NotificationPolicy,
    strategy: ExecutionStrategy,
    review_report: &crate::core::model::ReviewActionReport,
) -> Option<String> {
    if strategy == ExecutionStrategy::DryRun || notification_policy == NotificationPolicy::Always {
        return None;
    }

    if review_report
        .applied
        .iter()
        .any(|action| action.action != ReviewActionKind::UpsertExplain)
    {
        return None;
    }

    review_report
        .skipped
        .iter()
        .find(|action| action.action != ReviewActionKind::UpsertExplain)
        .map(|action| format!("skipped because {}", action.reason))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        NotificationAudience, NotificationDeliveryReport, NotificationMessage,
        NotificationSeverity, ReviewActionReport, ReviewSnapshot,
    };
    use crate::runtime::{ConnectorError, ConnectorResult};

    struct FailingSlackSink;

    #[async_trait::async_trait]
    impl NotificationSink for FailingSlackSink {
        fn kind(&self) -> NotificationSinkKind {
            NotificationSinkKind::SlackApp
        }

        async fn send(
            &self,
            _notification: &NotificationMessage,
        ) -> ConnectorResult<NotificationDeliveryReport> {
            Err(ConnectorError::Request("Slack unavailable".to_string()))
        }
    }

    struct NoopPlatformConnector;

    #[async_trait::async_trait]
    impl PlatformConnector for NoopPlatformConnector {
        fn kind(&self) -> ReviewPlatformKind {
            ReviewPlatformKind::GitLab
        }

        async fn load_snapshot(&self) -> ConnectorResult<ReviewSnapshot> {
            Err(ConnectorError::Other("unused in test".to_string()))
        }

        async fn load_managed_comment(
            &self,
            _marker: &str,
        ) -> ConnectorResult<Option<crate::core::model::ManagedReviewComment>> {
            Ok(None)
        }

        async fn apply_review_actions(
            &self,
            _actions: &[ReviewAction],
        ) -> ConnectorResult<ReviewActionReport> {
            Ok(ReviewActionReport::default())
        }
    }

    fn wiring_with_failing_slack_sink() -> RuntimeWiring {
        RuntimeWiring::new(
            Box::new(NoopPlatformConnector),
            None,
            vec![Box::new(FailingSlackSink)],
        )
    }

    fn sample_slack_append_notification() -> NotificationMessage {
        NotificationMessage {
            sink: NotificationSinkKind::SlackApp,
            subject: "Mr. Milchick advisory explain for MR #3995".to_string(),
            body: "Advisory review details".to_string(),
            audience: NotificationAudience::Default,
            severity: NotificationSeverity::Info,
            thread_key: Some("MR #3995".to_string()),
            prefer_thread_reply: true,
            conversation_key: Some("gitlab:412:3995".to_string()),
            delivery: NotificationDelivery::Append,
        }
    }

    #[tokio::test]
    async fn reports_notification_sink_failure_without_failing_delivery() {
        let reports = wiring_with_failing_slack_sink()
            .deliver_notifications(
                ExecutionStrategy::Real,
                NotificationPolicy::Always,
                &[sample_slack_append_notification()],
                &ReviewActionReport::default(),
            )
            .await
            .expect("notification failures should be reported, not returned");

        assert_eq!(reports.len(), 1);
        assert!(!reports[0].delivered);
        assert!(
            reports[0]
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("Slack unavailable"))
        );
    }
}
