#[cfg(feature = "github")]
pub mod github;
#[cfg(feature = "gitlab")]
pub mod gitlab;

#[cfg(any(
    feature = "slack-app",
    feature = "slack-workflow",
    feature = "teams",
    feature = "discord"
))]
pub mod notifications;
