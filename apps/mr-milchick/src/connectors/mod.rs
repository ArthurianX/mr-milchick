#[cfg(feature = "gitlab")]
pub mod gitlab;
#[cfg(feature = "github")]
pub mod github;

#[cfg(any(
    feature = "slack-app",
    feature = "slack-workflow",
    feature = "teams",
    feature = "discord"
))]
pub mod notifications;
