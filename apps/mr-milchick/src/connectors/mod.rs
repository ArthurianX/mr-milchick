#[cfg(feature = "github")]
pub mod github;
#[cfg(feature = "gitlab")]
pub mod gitlab;

#[cfg(any(feature = "slack-app", feature = "teams", feature = "discord"))]
pub mod notifications;
