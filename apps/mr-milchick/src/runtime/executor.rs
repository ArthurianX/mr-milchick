use crate::core::model::{
    NotificationDeliveryReport, NotificationMessage, ReviewAction, ReviewActionReport,
    ReviewPlatformKind,
};
use anyhow::Result;
use async_trait::async_trait;
use thiserror::Error;

pub type ConnectorResult<T> = Result<T, ConnectorError>;

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("authentication failed: {0}")]
    Authentication(String),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("unsupported capability: {0}")]
    Unsupported(String),

    #[error("request failed: {0}")]
    Request(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("unknown connector error: {0}")]
    Other(String),
}

#[async_trait]
pub trait ReviewConnector: Send + Sync {
    fn kind(&self) -> ReviewPlatformKind;

    async fn load_snapshot(&self) -> ConnectorResult<crate::core::model::ReviewSnapshot>;

    async fn apply_review_actions(
        &self,
        actions: &[ReviewAction],
    ) -> ConnectorResult<ReviewActionReport>;
}

#[async_trait]
pub trait NotificationSink: Send + Sync {
    fn kind(&self) -> crate::core::model::NotificationSinkKind;

    async fn send(
        &self,
        notification: &NotificationMessage,
    ) -> ConnectorResult<NotificationDeliveryReport>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionReport {
    pub review_report: ReviewActionReport,
    pub notification_reports: Vec<NotificationDeliveryReport>,
}
