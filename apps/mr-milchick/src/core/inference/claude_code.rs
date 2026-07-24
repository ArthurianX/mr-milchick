use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use super::{
    ReviewInferenceEngine, ReviewInferenceError, ReviewInferenceOutcome, ReviewPromptStyle,
    ReviewSnapshot, build_review_prompt,
};

#[derive(Debug, Deserialize)]
struct ClaudeCodeEnvelope {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    result: String,
    #[serde(default)]
    structured_output: Option<serde_json::Value>,
}

const REVIEW_JSON_SCHEMA: &str = r#"{
  "type":"object",
  "additionalProperties":false,
  "properties":{
    "summary":{"type":["string","null"]},
    "recommendations":{
      "type":"array",
      "maxItems":4,
      "items":{
        "type":"object",
        "additionalProperties":false,
        "properties":{
          "category":{"type":"string","enum":["ReviewFocus","Risk","TestGap"]},
          "message":{"type":"string"}
        },
        "required":["category","message"]
      }
    }
  },
  "required":["summary","recommendations"]
}"#;
const MIN_CLAUDE_CODE_VERSION: [u64; 3] = [2, 1, 218];
const MAX_CAPTURE_BYTES: usize = 1024 * 1024;

pub struct ClaudeCodeReviewInferenceEngine {
    model: String,
    oauth_token: Option<String>,
    max_patch_bytes: usize,
    timeout: Duration,
}

impl ClaudeCodeReviewInferenceEngine {
    pub fn new(
        model: impl Into<String>,
        oauth_token: Option<String>,
        max_patch_bytes: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            model: model.into(),
            oauth_token,
            max_patch_bytes,
            timeout,
        }
    }

    async fn run_prompt(
        &self,
        prompt: String,
    ) -> Result<ReviewInferenceOutcome, ReviewInferenceError> {
        let token = self
            .oauth_token
            .as_deref()
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| {
                ReviewInferenceError::Configuration(
                    "Claude Code inference requires CLAUDE_CODE_OAUTH_TOKEN".to_string(),
                )
            })?;
        let isolated = create_isolated_directory()?;
        let mut command = self.base_command(&isolated);
        command.env("CLAUDE_CODE_OAUTH_TOKEN", token);
        command.args([
            "--print",
            "--model",
            self.model.as_str(),
            "--tools",
            "",
            "--safe-mode",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--permission-mode",
            "dontAsk",
            "--output-format",
            "json",
            "--json-schema",
            REVIEW_JSON_SCHEMA,
        ]);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);

        let mut child = command.spawn().map_err(|error| {
            ReviewInferenceError::Analysis(format!("failed to start Claude Code: {error}"))
        })?;
        let mut stdin = child.stdin.take().ok_or_else(|| {
            ReviewInferenceError::Analysis("failed to open Claude Code stdin".to_string())
        })?;
        stdin.write_all(prompt.as_bytes()).await.map_err(|error| {
            ReviewInferenceError::Analysis(format!("failed to write Claude Code prompt: {error}"))
        })?;
        drop(stdin);

        let output = timeout(self.timeout, child.wait_with_output())
            .await
            .map_err(|_| ReviewInferenceError::TimedOut(self.timeout))?
            .map_err(|error| {
                ReviewInferenceError::Analysis(format!("failed to wait for Claude Code: {error}"))
            })?;
        let _ = std::fs::remove_dir_all(&isolated);
        if output.stdout.len() > MAX_CAPTURE_BYTES || output.stderr.len() > MAX_CAPTURE_BYTES {
            return Err(ReviewInferenceError::Analysis(
                "Claude Code output exceeded the configured capture limit".to_string(),
            ));
        }
        if !output.status.success() {
            return Err(ReviewInferenceError::Analysis(format!(
                "Claude Code exited with status {}",
                output.status
            )));
        }
        let envelope: ClaudeCodeEnvelope =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                ReviewInferenceError::Analysis(format!(
                    "Claude Code returned an invalid JSON envelope: {error}"
                ))
            })?;
        if envelope.is_error {
            return Err(ReviewInferenceError::Analysis(
                "Claude Code reported an error".to_string(),
            ));
        }
        let raw = match envelope.structured_output {
            Some(value) => serde_json::to_string(&value).map_err(|error| {
                ReviewInferenceError::Analysis(format!(
                    "failed to serialize Claude Code structured output: {error}"
                ))
            })?,
            None => envelope.result,
        };
        Ok(ReviewInferenceOutcome::ready(
            super::output::parse_review_output(&raw)?,
        ))
    }

    async fn check_version(&self) -> Result<(), ReviewInferenceError> {
        let isolated = create_isolated_directory()?;
        let mut command = self.base_command(&isolated);
        command.arg("--version");
        let output = timeout(self.timeout, command.output())
            .await
            .map_err(|_| ReviewInferenceError::TimedOut(self.timeout))?
            .map_err(|error| {
                ReviewInferenceError::Configuration(format!(
                    "Claude Code executable is unavailable: {error}"
                ))
            })?;
        let _ = std::fs::remove_dir_all(&isolated);
        let raw = String::from_utf8_lossy(&output.stdout);
        let parts = raw
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                ReviewInferenceError::Configuration(
                    "Claude Code returned an invalid version".to_string(),
                )
            })?;
        let [major, minor, patch] = parts.as_slice() else {
            return Err(ReviewInferenceError::Configuration(
                "Claude Code returned an invalid version".to_string(),
            ));
        };
        if [*major, *minor, *patch] < MIN_CLAUDE_CODE_VERSION {
            return Err(ReviewInferenceError::Configuration(
                "Claude Code 2.1.218 or newer is required".to_string(),
            ));
        }
        Ok(())
    }

    fn base_command(&self, isolated: &PathBuf) -> Command {
        let mut command = Command::new("claude");
        command.env_clear();
        for name in [
            "PATH",
            "LANG",
            "LC_ALL",
            "TZ",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "NODE_EXTRA_CA_CERTS",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "no_proxy",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.env("HOME", isolated);
        command.env("CLAUDE_CONFIG_DIR", isolated.join(".claude"));
        command.current_dir(isolated);
        command
    }
}

fn create_isolated_directory() -> Result<PathBuf, ReviewInferenceError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ReviewInferenceError::Analysis(format!("clock error: {error}")))?
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("mr-milchick-claude-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&path).map_err(|error| {
        ReviewInferenceError::Analysis(format!(
            "failed to create isolated Claude directory: {error}"
        ))
    })?;
    Ok(path)
}

#[async_trait]
impl ReviewInferenceEngine for ClaudeCodeReviewInferenceEngine {
    async fn analyze(
        &self,
        snapshot: &ReviewSnapshot,
    ) -> Result<ReviewInferenceOutcome, ReviewInferenceError> {
        if self
            .oauth_token
            .as_deref()
            .is_none_or(|token| token.trim().is_empty())
        {
            return Ok(ReviewInferenceOutcome::unavailable(
                "Claude Code inference requires CLAUDE_CODE_OAUTH_TOKEN",
            ));
        }
        if let Err(error) = self.check_version().await {
            return Ok(ReviewInferenceOutcome::unavailable(error.to_string()));
        }
        let primary = self
            .run_prompt(build_review_prompt(
                snapshot,
                self.max_patch_bytes,
                ReviewPromptStyle::Primary,
            ))
            .await?;
        if !primary.insights.is_empty() {
            return Ok(primary);
        }

        self.run_prompt(build_review_prompt(
            snapshot,
            self.max_patch_bytes,
            ReviewPromptStyle::StrictRetry,
        ))
        .await
    }

    fn handles_internal_timeout(&self) -> bool {
        true
    }
}
