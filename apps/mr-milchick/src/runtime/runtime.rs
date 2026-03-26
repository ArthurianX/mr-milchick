use crate::config::model::NotificationPolicy;
use crate::core::model::{
    NotificationMessage, NotificationSinkKind, ReviewAction, ReviewActionKind, ReviewPlatformKind,
};
use anyhow::Result;

use crate::runtime::executor::{ExecutionReport, NotificationSink, ReviewConnector};

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
    pub fn from_env() -> Self {
        let dry_run = std::env::var("MR_MILCHICK_DRY_RUN")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);

        if dry_run { Self::DryRun } else { Self::Real }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCapabilities {
    pub review_platform: ReviewPlatformKind,
    pub notification_sinks: Vec<NotificationSinkKind>,
}

pub struct RuntimeWiring {
    pub review_connector: Box<dyn ReviewConnector>,
    pub notification_sinks: Vec<Box<dyn NotificationSink>>,
    pub capabilities: RuntimeCapabilities,
}

impl RuntimeWiring {
    pub fn new(
        review_connector: Box<dyn ReviewConnector>,
        notification_sinks: Vec<Box<dyn NotificationSink>>,
    ) -> Self {
        let capabilities = RuntimeCapabilities {
            review_platform: review_connector.kind(),
            notification_sinks: notification_sinks.iter().map(|sink| sink.kind()).collect(),
        };

        Self {
            review_connector,
            notification_sinks,
            capabilities,
        }
    }

    pub async fn execute(
        &self,
        strategy: ExecutionStrategy,
        notification_policy: NotificationPolicy,
        review_actions: &[ReviewAction],
        notifications: &[NotificationMessage],
    ) -> Result<ExecutionReport> {
        let review_report = if strategy == ExecutionStrategy::DryRun {
            dry_run_review_report(review_actions)
        } else {
            self.review_connector
                .apply_review_actions(review_actions)
                .await?
        };

        let mut notification_reports = Vec::new();
        let notification_skip_reason =
            notification_skip_reason(notification_policy, strategy, &review_report);

        for sink in &self.notification_sinks {
            for notification in notifications {
                let report = if let Some(reason) = &notification_skip_reason {
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
        .any(|action| action.action == ReviewActionKind::UpsertSummary)
    {
        return None;
    }

    review_report
        .skipped
        .iter()
        .find(|action| action.action == ReviewActionKind::UpsertSummary)
        .map(|action| format!("skipped because {}", action.reason))
}
