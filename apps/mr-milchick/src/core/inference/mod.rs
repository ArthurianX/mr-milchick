use std::time::{Duration, Instant};

use async_trait::async_trait;
#[cfg(any(feature = "llm-local", test))]
use serde::Deserialize;
use tokio::time::timeout;
#[cfg(feature = "llm-local")]
use tracing::{debug, info, warn};

#[cfg(any(feature = "llm-local", test))]
use crate::core::model::ChangeType;
use crate::core::model::ReviewSnapshot;

#[cfg(feature = "llm-local")]
use std::num::NonZeroU32;
#[cfg(feature = "llm-local")]
use std::path::{Path, PathBuf};
#[cfg(feature = "llm-local")]
use tokio::task;

#[cfg(feature = "llm-local")]
use llama_cpp_2::LlamaBackendDevice;
#[cfg(feature = "llm-local")]
use llama_cpp_2::LlamaBackendDeviceType;
#[cfg(feature = "llm-local")]
use llama_cpp_2::TokenToStringError;
#[cfg(feature = "llm-local")]
use llama_cpp_2::context::{LlamaContext, params::LlamaContextParams};
#[cfg(feature = "llm-local")]
use llama_cpp_2::list_llama_ggml_backend_devices;
#[cfg(feature = "llm-local")]
use llama_cpp_2::llama_backend::LlamaBackend;
#[cfg(feature = "llm-local")]
use llama_cpp_2::llama_batch::{BatchAddError, LlamaBatch};
#[cfg(feature = "llm-local")]
use llama_cpp_2::model::params::{LlamaModelParams, LlamaSplitMode};
#[cfg(feature = "llm-local")]
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel};
#[cfg(feature = "llm-local")]
use llama_cpp_2::openai::OpenAIChatTemplateParams;
#[cfg(feature = "llm-local")]
use llama_cpp_2::sampling::LlamaSampler;

#[cfg(feature = "llm-local")]
#[cfg(feature = "llm-local")]
const LOCAL_BATCH_CAPACITY: usize = 512;
#[cfg(feature = "llm-local")]
const LOCAL_MAX_GENERATION_TOKENS: usize = 384;
#[cfg(feature = "llm-local")]
const LOCAL_QWEN_SAMPLER_SEED: u32 = 23_042;
#[cfg(any(feature = "llm-local", test))]
const LOCAL_MAX_RECOMMENDATIONS: usize = 4;

#[cfg(feature = "llm-local")]
struct RenderedPrompt {
    prompt: String,
    additional_stops: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewInferenceStatus {
    Disabled,
    Ready,
    TimedOut,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewInsights {
    pub summary: Option<String>,
    pub recommendations: Vec<ReviewRecommendation>,
}

impl ReviewInsights {
    pub fn is_empty(&self) -> bool {
        self.summary
            .as_deref()
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
            .is_none()
            && self.recommendations.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRecommendation {
    pub category: RecommendationCategory,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendationCategory {
    ReviewFocus,
    Risk,
    TestGap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewInferenceOutcome {
    pub status: ReviewInferenceStatus,
    pub insights: ReviewInsights,
    pub detail: Option<String>,
}

impl ReviewInferenceOutcome {
    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            status: ReviewInferenceStatus::Disabled,
            insights: ReviewInsights::default(),
            detail: Some(reason.into()),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            status: ReviewInferenceStatus::Unavailable,
            insights: ReviewInsights::default(),
            detail: Some(reason.into()),
        }
    }

    pub fn ready(insights: ReviewInsights) -> Self {
        Self {
            status: ReviewInferenceStatus::Ready,
            insights,
            detail: None,
        }
    }

    pub fn failed(reason: impl Into<String>) -> Self {
        Self {
            status: ReviewInferenceStatus::Failed,
            insights: ReviewInsights::default(),
            detail: Some(reason.into()),
        }
    }

    pub fn timed_out(timeout: Duration) -> Self {
        Self {
            status: ReviewInferenceStatus::TimedOut,
            insights: ReviewInsights::default(),
            detail: Some(format!(
                "review inference timed out after {} ms",
                timeout.as_millis()
            )),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewInferenceError {
    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("analysis failed: {0}")]
    Analysis(String),

    #[error("timed out after {} ms", .0.as_millis())]
    TimedOut(Duration),
}

#[async_trait]
pub trait ReviewInferenceEngine: Send + Sync {
    async fn analyze(
        &self,
        snapshot: &ReviewSnapshot,
    ) -> Result<ReviewInferenceOutcome, ReviewInferenceError>;

    fn handles_internal_timeout(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct NoopReviewInferenceEngine {
    outcome: ReviewInferenceOutcome,
}

impl NoopReviewInferenceEngine {
    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            outcome: ReviewInferenceOutcome::disabled(reason),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            outcome: ReviewInferenceOutcome::unavailable(reason),
        }
    }
}

#[async_trait]
impl ReviewInferenceEngine for NoopReviewInferenceEngine {
    async fn analyze(
        &self,
        _snapshot: &ReviewSnapshot,
    ) -> Result<ReviewInferenceOutcome, ReviewInferenceError> {
        Ok(self.outcome.clone())
    }
}

pub async fn analyze_with_timeout(
    engine: &(dyn ReviewInferenceEngine + Send + Sync),
    snapshot: &ReviewSnapshot,
    timeout_duration: Duration,
) -> ReviewInferenceOutcome {
    if engine.handles_internal_timeout() {
        match engine.analyze(snapshot).await {
            Ok(outcome) => outcome,
            Err(ReviewInferenceError::TimedOut(timeout)) => ReviewInferenceOutcome::timed_out(timeout),
            Err(err) => ReviewInferenceOutcome::failed(err.to_string()),
        }
    } else {
        match timeout(timeout_duration, engine.analyze(snapshot)).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(ReviewInferenceError::TimedOut(timeout))) => {
                ReviewInferenceOutcome::timed_out(timeout)
            }
            Ok(Err(err)) => ReviewInferenceOutcome::failed(err.to_string()),
            Err(_) => ReviewInferenceOutcome::timed_out(timeout_duration),
        }
    }
}

#[cfg(feature = "llm-local")]
#[derive(Debug, Clone)]
pub struct LocalLlamaReviewInferenceEngine {
    model_path: PathBuf,
    max_patch_bytes: usize,
    context_token_limit: usize,
    timeout: Duration,
}

#[cfg(feature = "llm-local")]
impl LocalLlamaReviewInferenceEngine {
    pub fn new(
        model_path: impl Into<PathBuf>,
        max_patch_bytes: usize,
        context_token_limit: usize,
        timeout: Duration,
    ) -> Result<Self, ReviewInferenceError> {
        let model_path = model_path.into();

        if model_path.as_os_str().is_empty() {
            return Err(ReviewInferenceError::Configuration(
                "LLM model path cannot be empty".to_string(),
            ));
        }

        if !model_path.is_file() {
            return Err(ReviewInferenceError::Configuration(format!(
                "LLM model path '{}' does not point to a GGUF file on disk",
                model_path.display()
            )));
        }

        Ok(Self {
            model_path,
            max_patch_bytes,
            context_token_limit: context_token_limit.max(1_024),
            timeout,
        })
    }
}

#[cfg(feature = "llm-local")]
#[async_trait]
impl ReviewInferenceEngine for LocalLlamaReviewInferenceEngine {
    async fn analyze(
        &self,
        snapshot: &ReviewSnapshot,
    ) -> Result<ReviewInferenceOutcome, ReviewInferenceError> {
        let model_path = self.model_path.clone();
        let max_patch_bytes = self.max_patch_bytes;
        let context_token_limit = self.context_token_limit;
        let timeout = self.timeout;
        let changed_files = snapshot.changed_files.len();
        let primary_prompt =
            build_local_inference_prompt(snapshot, self.max_patch_bytes, LocalPromptStyle::Primary);
        let retry_prompt = build_local_inference_prompt(
            snapshot,
            self.max_patch_bytes,
            LocalPromptStyle::StrictRetry,
        );

        task::spawn_blocking(move || {
            let deadline = Instant::now() + timeout;
            info!(
                model_path = %model_path.display(),
                changed_files,
                max_patch_bytes,
                configured_context_token_limit = context_token_limit,
                timeout_ms = timeout.as_millis() as u64,
                "starting llama.cpp advisory review run"
            );
            match run_local_inference(
                model_path.as_path(),
                &primary_prompt,
                context_token_limit,
                deadline,
                timeout,
            ) {
                Ok(insights) if !insights.is_empty() => {
                    info!(
                        summary_present = insights.summary.is_some(),
                        recommendation_count = insights.recommendations.len(),
                        "llama.cpp primary advisory review produced structured suggestions"
                    );
                    Ok(ReviewInferenceOutcome::ready(insights))
                }
                Ok(_) => {
                    warn!(
                        "llama.cpp primary advisory review returned empty structured output; retrying with strict prompt"
                    );
                    let insights = run_local_inference(
                        model_path.as_path(),
                        &retry_prompt,
                        context_token_limit,
                        deadline,
                        timeout,
                    )?;
                    if insights.is_empty() {
                        warn!(
                            "llama.cpp strict retry also returned empty structured output"
                        );
                    } else {
                        info!(
                            summary_present = insights.summary.is_some(),
                            recommendation_count = insights.recommendations.len(),
                            "llama.cpp strict retry produced structured suggestions"
                        );
                    }
                    Ok(ReviewInferenceOutcome::ready(insights))
                }
                Err(err) if should_retry_local_inference(&err) => {
                    warn!(error = %err, "llama.cpp primary advisory review failed in a retryable way; retrying with strict prompt");
                    let insights = run_local_inference(
                        model_path.as_path(),
                        &retry_prompt,
                        context_token_limit,
                        deadline,
                        timeout,
                    )?;
                    if insights.is_empty() {
                        warn!(
                            "llama.cpp strict retry finished but still returned empty structured output"
                        );
                    } else {
                        info!(
                            summary_present = insights.summary.is_some(),
                            recommendation_count = insights.recommendations.len(),
                            "llama.cpp strict retry recovered and produced structured suggestions"
                        );
                    }
                    Ok(ReviewInferenceOutcome::ready(insights))
                }
                Err(err) => {
                    warn!(error = %err, "llama.cpp advisory review failed without retry");
                    Err(err)
                }
            }
        })
        .await
        .map_err(|err| {
            ReviewInferenceError::Analysis(format!("local inference task failed: {err}"))
        })?
    }

    fn handles_internal_timeout(&self) -> bool {
        true
    }
}

#[cfg(feature = "llm-local")]
fn should_retry_local_inference(err: &ReviewInferenceError) -> bool {
    matches!(
        err,
        ReviewInferenceError::Analysis(message)
            if message.contains("did not return a JSON object")
                || message.contains("returned invalid JSON")
    )
}

#[cfg(any(feature = "llm-local", test))]
#[derive(Debug, Deserialize)]
struct LlmReviewResponse {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    recommendations: Vec<LlmReviewRecommendation>,
}

#[cfg(any(feature = "llm-local", test))]
#[derive(Debug, Deserialize)]
struct LlmReviewRecommendation {
    category: String,
    message: String,
}

#[cfg(any(feature = "llm-local", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalPromptStyle {
    Primary,
    StrictRetry,
}

#[cfg(any(feature = "llm-local", test))]
fn build_local_inference_prompt(
    snapshot: &ReviewSnapshot,
    max_patch_bytes: usize,
    style: LocalPromptStyle,
) -> String {
    let labels = if snapshot.labels.is_empty() {
        "none".to_string()
    } else {
        snapshot.labels.join(", ")
    };
    let review_cues = infer_review_cues(snapshot);
    let mut prompt = String::new();
    prompt.push_str("You are Mr. Milchick, a careful code review assistant running inside CI.\n");
    prompt.push_str("Analyze the review snapshot and return JSON only.\n");
    prompt.push_str(
        "Schema: {\"summary\":\"string or null\",\"recommendations\":[{\"category\":\"ReviewFocus|Risk|TestGap\",\"message\":\"string\"}]}\n",
    );
    prompt.push_str("Rules:\n");
    prompt.push_str("- Keep the output advisory and recommendation-only.\n");
    prompt.push_str("- Base conclusions only on the supplied snapshot and diff excerpts.\n");
    prompt.push_str("- Return strict JSON with double-quoted keys and string values.\n");
    prompt.push_str("- Do not wrap the JSON in markdown fences or commentary.\n");
    prompt.push_str("- Do not copy placeholder text from the schema like 'string or null'.\n");
    prompt.push_str(
        "- Every summary and recommendation must mention a concrete change from the diff.\n",
    );
    prompt.push_str(
        "- Pay extra attention to deleted tests, removed auth or validation checks, and raw HTML rendering changes.\n",
    );
    prompt.push_str("- Use at most 4 recommendations.\n");
    prompt.push_str("- Keep each recommendation message under 160 characters.\n");
    prompt.push_str("- If the diff shows an obvious risk, do not return an empty recommendations array.\n");
    prompt.push_str("- Use null for summary if nothing useful stands out.\n\n");
    prompt.push_str("Review snapshot:\n");
    prompt.push_str(&format!("Title: {}\n", snapshot.title));
    prompt.push_str(&format!("Author: @{}\n", snapshot.author.username));
    prompt.push_str(&format!(
        "Source branch: {}\n",
        snapshot
            .metadata
            .source_branch
            .as_deref()
            .unwrap_or("unknown")
    ));
    prompt.push_str(&format!(
        "Target branch: {}\n",
        snapshot
            .metadata
            .target_branch
            .as_deref()
            .unwrap_or("unknown")
    ));
    prompt.push_str(&format!("Draft: {}\n", snapshot.is_draft));
    prompt.push_str(&format!("Labels: {}\n", labels));
    prompt.push_str(&format!(
        "Changed files: {}\n",
        snapshot.changed_files.len()
    ));
    if let Some(description) = snapshot
        .description
        .as_deref()
        .map(str::trim)
        .filter(|description| !description.is_empty())
    {
        prompt.push_str("Description:\n");
        prompt.push_str(description);
        prompt.push('\n');
    }

    if !review_cues.is_empty() {
        prompt.push_str("\nPotential review cues from the diff:\n");
        for cue in &review_cues {
            prompt.push_str("- ");
            prompt.push_str(cue);
            prompt.push('\n');
        }
    }

    prompt.push_str("\nChanged files detail:\n");
    let mut remaining_patch_bytes = max_patch_bytes;

    for file in &snapshot.changed_files {
        prompt.push_str(&format!(
            "- File: {} [{}] (+{} -{})\n",
            file.path,
            format_change_type(file.change_type),
            file.additions.unwrap_or(0),
            file.deletions.unwrap_or(0)
        ));

        if let Some(previous_path) = file
            .previous_path
            .as_deref()
            .map(str::trim)
            .filter(|previous_path| !previous_path.is_empty())
        {
            prompt.push_str(&format!("  Previous path: {}\n", previous_path));
        }

        match file.patch.as_deref() {
            Some(patch) if remaining_patch_bytes > 0 => {
                let patch_excerpt = truncate_utf8(patch, remaining_patch_bytes);
                remaining_patch_bytes =
                    remaining_patch_bytes.saturating_sub(patch_excerpt.as_bytes().len());
                prompt.push_str("  Patch:\n");
                for line in patch_excerpt.lines() {
                    prompt.push_str("    ");
                    prompt.push_str(line);
                    prompt.push('\n');
                }
                if patch_excerpt.len() < patch.len() {
                    prompt.push_str("    [patch truncated to fit configured patch budget]\n");
                    remaining_patch_bytes = 0;
                }
            }
            Some(_) => {
                prompt.push_str("  Patch: [omitted after reaching configured patch budget]\n");
            }
            None => {
                prompt.push_str("  Patch: [not available]\n");
            }
        }
    }

    if matches!(style, LocalPromptStyle::StrictRetry) {
        prompt.push_str("\nReturn exactly one JSON object.\n");
        prompt.push_str("The first character of your reply must be { and the last character must be }.\n");
        prompt.push_str("Do not echo diff lines, code snippets, or brace fragments from the patch.\n");
        prompt.push_str(
            "If the snapshot includes deleted tests, removed auth or validation checks, or dangerous HTML rendering, include at least one recommendation.\n",
        );
        prompt.push_str(
            "Example valid reply: {\"summary\":\"Route lost admin enforcement.\",\"recommendations\":[{\"category\":\"Risk\",\"message\":\"Restore the admin guard on the role update route.\"}]}\n",
        );
    }

    prompt
}

#[cfg(any(feature = "llm-local", test))]
fn truncate_utf8(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }

    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }

    input[..end].to_string()
}

#[cfg(any(feature = "llm-local", test))]
fn format_change_type(change_type: ChangeType) -> &'static str {
    match change_type {
        ChangeType::Added => "Added",
        ChangeType::Modified => "Modified",
        ChangeType::Deleted => "Deleted",
        ChangeType::Renamed => "Renamed",
        ChangeType::Unknown => "Unknown",
    }
}

#[cfg(any(feature = "llm-local", test))]
fn infer_review_cues(snapshot: &ReviewSnapshot) -> Vec<String> {
    let mut cues = Vec::new();

    let deleted_test_file = snapshot.changed_files.iter().any(|file| {
        matches!(file.change_type, ChangeType::Deleted) && looks_like_test_path(&file.path)
    });
    if deleted_test_file {
        cues.push("A test file was deleted in this change.".to_string());
    }

    let removed_test_assertion = snapshot.changed_files.iter().any(|file| {
        file.patch
            .as_deref()
            .map(|patch| {
                patch.lines().any(|line| {
                    line.starts_with('-')
                        && !line.starts_with("---")
                        && (line.contains("expect(")
                            || line.contains("assert")
                            || line.contains("toBe(")
                            || line.contains("toEqual("))
                })
            })
            .unwrap_or(false)
    });
    if removed_test_assertion {
        cues.push("The diff removes an assertion or test expectation.".to_string());
    }

    let removed_auth_or_validation = snapshot.changed_files.iter().any(|file| {
        file.patch
            .as_deref()
            .map(|patch| {
                patch.lines().any(|line| {
                    line.starts_with('-')
                        && !line.starts_with("---")
                        && contains_any_ascii_case_insensitive(
                            line,
                            &[
                                "requireadmin",
                                "validate",
                                "authorization",
                                "permission",
                                "auth",
                                "guard",
                                "forbid",
                            ],
                        )
                })
            })
            .unwrap_or(false)
    });
    if removed_auth_or_validation {
        cues.push(
            "The diff removes code that looks like auth, permission, or validation logic."
                .to_string(),
        );
    }

    let dangerous_html_rendering = snapshot.changed_files.iter().any(|file| {
        file.patch
            .as_deref()
            .map(|patch| {
                contains_any_ascii_case_insensitive(
                    patch,
                    &["dangerouslysetinnerhtml", "innerhtml", "setinnerhtml"],
                )
            })
            .unwrap_or(false)
    });
    if dangerous_html_rendering {
        cues.push("The diff introduces raw HTML rendering.".to_string());
    }

    cues
}

#[cfg(any(feature = "llm-local", test))]
fn looks_like_test_path(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase();
    normalized.contains("/test")
        || normalized.contains("/tests/")
        || normalized.ends_with(".test.ts")
        || normalized.ends_with(".test.tsx")
        || normalized.ends_with(".test.js")
        || normalized.ends_with(".test.jsx")
        || normalized.ends_with(".spec.ts")
        || normalized.ends_with(".spec.tsx")
        || normalized.ends_with(".spec.js")
        || normalized.ends_with(".spec.jsx")
}

#[cfg(any(feature = "llm-local", test))]
fn contains_any_ascii_case_insensitive(haystack: &str, needles: &[&str]) -> bool {
    let normalized = haystack
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();

    needles.iter().any(|needle| normalized.contains(needle))
}

#[cfg(any(feature = "llm-local", test))]
fn parse_local_inference_output(raw: &str) -> Result<ReviewInsights, ReviewInferenceError> {
    let json_payloads = extract_json_objects(raw);
    if json_payloads.is_empty() {
        return Err(ReviewInferenceError::Analysis(format!(
            "local review model did not return a JSON object; raw output preview: {}",
            preview_inference_output(raw)
        )));
    }

    let response = json_payloads
        .iter()
        .find_map(|payload| serde_json::from_str::<LlmReviewResponse>(payload).ok())
        .ok_or_else(|| {
            let first_payload = json_payloads.first().copied().unwrap_or(raw);
            ReviewInferenceError::Analysis(format!(
                "local review model returned invalid JSON: payload preview: {}",
                preview_inference_output(first_payload)
            ))
        })?;

    Ok(llm_response_to_review_insights(response))
}

#[cfg(any(feature = "llm-local", test))]
fn first_non_empty_review_response(raw: &str) -> Option<ReviewInsights> {
    extract_json_objects(raw)
        .into_iter()
        .filter_map(|payload| serde_json::from_str::<LlmReviewResponse>(payload).ok())
        .map(llm_response_to_review_insights)
        .find(|insights| !insights.is_empty())
}

#[cfg(any(feature = "llm-local", test))]
fn preview_inference_output(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview = truncate_utf8(&collapsed, 240);
    if preview.len() < collapsed.len() {
        format!("{preview}...")
    } else {
        preview
    }
}

#[cfg(any(feature = "llm-local", test))]
fn extract_json_objects(raw: &str) -> Vec<&str> {
    let mut objects = Vec::new();
    let mut start_index = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;

    for (index, ch) in raw.char_indices() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }

            match ch {
                '\\' => escape = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' if start_index.is_some() => in_string = true,
            '{' => {
                if start_index.is_none() {
                    start_index = Some(index);
                }
                depth += 1;
            }
            '}' if start_index.is_some() => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(start) = start_index.take() {
                        objects.push(&raw[start..index + ch.len_utf8()]);
                    }
                }
            }
            _ => {}
        }
    }

    objects
}

#[cfg(any(feature = "llm-local", test))]
#[cfg(test)]
fn contains_parseable_review_response(raw: &str) -> bool {
    extract_json_objects(raw)
        .into_iter()
        .any(|payload| serde_json::from_str::<LlmReviewResponse>(payload).is_ok())
}

#[cfg(any(feature = "llm-local", test))]
fn llm_response_to_review_insights(response: LlmReviewResponse) -> ReviewInsights {
    let summary = normalize_optional_text(response.summary);
    let recommendations = response
        .recommendations
        .into_iter()
        .filter_map(|recommendation| {
            let category = parse_recommendation_category(&recommendation.category)?;
            let message = normalize_optional_text(Some(recommendation.message))?;
            Some(ReviewRecommendation { category, message })
        })
        .take(LOCAL_MAX_RECOMMENDATIONS)
        .collect();

    ReviewInsights {
        summary,
        recommendations,
    }
}

#[cfg(any(feature = "llm-local", test))]
fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let normalized = value.trim();
        let lower = normalized.to_ascii_lowercase();
        if normalized.is_empty()
            || lower == "null"
            || lower == "string or null"
            || lower == "null or string"
        {
            None
        } else {
            Some(normalized.to_string())
        }
    })
}

#[cfg(any(feature = "llm-local", test))]
fn parse_recommendation_category(raw: &str) -> Option<RecommendationCategory> {
    let normalized = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();

    match normalized.as_str() {
        "reviewfocus" => Some(RecommendationCategory::ReviewFocus),
        "risk" => Some(RecommendationCategory::Risk),
        "testgap" => Some(RecommendationCategory::TestGap),
        _ => None,
    }
}

#[cfg(feature = "llm-local")]
fn run_local_inference(
    model_path: &Path,
    base_prompt: &str,
    requested_context_token_limit: usize,
    deadline: Instant,
    timeout: Duration,
) -> Result<ReviewInsights, ReviewInferenceError> {
    ensure_within_deadline(deadline, timeout)?;
    let mut backend = LlamaBackend::init().map_err(|err| {
        ReviewInferenceError::Analysis(format!("failed to initialize llama backend: {err}"))
    })?;
    backend.void_logs();

    ensure_within_deadline(deadline, timeout)?;
    let model_params = build_local_model_params()?;
    let model = LlamaModel::load_from_file(&backend, model_path, &model_params).map_err(|err| {
        ReviewInferenceError::Analysis(format!(
            "failed to load GGUF model '{}': {err}",
            model_path.display()
        ))
    })?;

    ensure_within_deadline(deadline, timeout)?;
    let rendered_prompt = render_prompt_for_model(&model, base_prompt);
    let prompt = rendered_prompt.prompt;

    let prompt_tokens = model.str_to_token(&prompt, AddBos::Always).map_err(|err| {
        ReviewInferenceError::Analysis(format!("failed to tokenize review prompt: {err}"))
    })?;

    if prompt_tokens.is_empty() {
        return Err(ReviewInferenceError::Analysis(
            "review prompt tokenized to an empty sequence".to_string(),
        ));
    }

    let context_token_limit = requested_context_size(prompt_tokens.len(), requested_context_token_limit);
    let batch_capacity = prompt_tokens.len().clamp(1, LOCAL_BATCH_CAPACITY);
    debug!(
        model_path = %model_path.display(),
        prompt_token_count = prompt_tokens.len(),
        context_token_limit,
        batch_capacity,
        "prepared llama.cpp prompt for advisory review"
    );
    let threads = recommended_thread_count();
    info!(
        model_path = %model_path.display(),
        llama_threads = threads,
        llama_threads_batch = threads,
        prompt_token_count = prompt_tokens.len(),
        context_token_limit,
        "configuring llama.cpp thread usage for advisory review"
    );
    let available_prompt_tokens = context_token_limit
        .checked_sub(LOCAL_MAX_GENERATION_TOKENS)
        .expect("context limit should exceed generation limit");
    if prompt_tokens.len() > available_prompt_tokens {
        return Err(ReviewInferenceError::Analysis(format!(
            "review snapshot exceeds local inference context budget ({} tokens > {})",
            prompt_tokens.len(),
            available_prompt_tokens
        )));
    }

    let context_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(context_token_limit as u32))
        .with_n_batch(batch_capacity as u32)
        .with_n_ubatch(batch_capacity as u32)
        .with_op_offload(false)
        .with_n_threads(threads)
        .with_n_threads_batch(threads);
    let mut context = model.new_context(&backend, context_params).map_err(|err| {
        ReviewInferenceError::Analysis(format!("failed to create llama context: {err}"))
    })?;

    decode_prompt_tokens(&mut context, &prompt_tokens, batch_capacity, deadline, timeout)?;

    let mut sampler = build_local_sampler(&model);
    sampler.accept_many(prompt_tokens.iter());

    let mut rendered_output = String::new();
    let mut position = prompt_tokens.len();

    for _ in 0..LOCAL_MAX_GENERATION_TOKENS {
        ensure_within_deadline(deadline, timeout)?;
        let token = sampler.sample(&context, -1);
        if model.is_eog_token(token) {
            break;
        }

        sampler.accept(token);
        rendered_output.push_str(&token_to_piece_lossy(&model, token)?);
        if trim_stop_sequence_suffix(&mut rendered_output, &rendered_prompt.additional_stops) {
            break;
        }
        if first_non_empty_review_response(&rendered_output).is_some() {
            break;
        }

        let mut batch = LlamaBatch::new(1, 1);
        batch
            .add(
                token,
                i32::try_from(position).map_err(|_| {
                    ReviewInferenceError::Analysis(
                        "generated sequence exceeded positional limits".to_string(),
                    )
                })?,
                &[0],
                true,
            )
            .map_err(batch_add_error)?;
        context.decode(&mut batch).map_err(|err| {
            ReviewInferenceError::Analysis(format!("failed during llama generation decode: {err}"))
        })?;
        position += 1;
    }

    parse_local_inference_output(&rendered_output)
}

#[cfg(feature = "llm-local")]
fn render_prompt_for_model(model: &LlamaModel, base_prompt: &str) -> RenderedPrompt {
    render_prompt_with_model_template(model, base_prompt).unwrap_or_else(|_| {
        if is_qwen_family_model(model) {
            RenderedPrompt {
                prompt: manual_qwen_prompt(base_prompt),
                additional_stops: Vec::new(),
            }
        } else {
            RenderedPrompt {
                prompt: base_prompt.to_string(),
                additional_stops: Vec::new(),
            }
        }
    })
}

#[cfg(feature = "llm-local")]
fn is_qwen_family_model(model: &LlamaModel) -> bool {
    let tokenizer_pre = model
        .meta_val_str("tokenizer.ggml.pre")
        .ok()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let architecture = model
        .meta_val_str("general.architecture")
        .ok()
        .unwrap_or_default()
        .to_ascii_lowercase();

    tokenizer_pre.contains("qwen") || architecture.contains("qwen")
}

#[cfg(feature = "llm-local")]
fn render_prompt_with_model_template(
    model: &LlamaModel,
    base_prompt: &str,
) -> Result<RenderedPrompt, ReviewInferenceError> {
    let template = model.chat_template(None).map_err(|err| {
        ReviewInferenceError::Analysis(format!(
            "failed to read model chat template for local inference: {err}"
        ))
    })?;

    if let Ok(rendered_prompt) =
        render_prompt_with_oaicompat_template(model, &template, base_prompt)
    {
        return Ok(rendered_prompt);
    }

    render_prompt_with_basic_template(model, &template, base_prompt)
}

#[cfg(feature = "llm-local")]
fn render_prompt_with_oaicompat_template(
    model: &LlamaModel,
    template: &llama_cpp_2::model::LlamaChatTemplate,
    base_prompt: &str,
) -> Result<RenderedPrompt, ReviewInferenceError> {
    let system_prompt = if is_qwen_family_model(model) {
        "You are Mr. Milchick, a careful CI code review assistant. Return exactly one compact JSON object. Do not emit <think> tags or reasoning text."
    } else {
        "You are Mr. Milchick, a careful CI code review assistant. Return exactly one compact JSON object."
    };

    let messages_json = serde_json::to_string(&vec![
        serde_json::json!({
            "role": "system",
            "content": system_prompt
        }),
        serde_json::json!({
            "role": "user",
            "content": base_prompt
        }),
    ])
    .map_err(|err| {
        ReviewInferenceError::Analysis(format!(
            "failed to serialize local inference chat messages: {err}"
        ))
    })?;

    let template_result = model
        .apply_chat_template_oaicompat(
            template,
            &OpenAIChatTemplateParams {
                messages_json: &messages_json,
                tools_json: None,
                tool_choice: None,
                json_schema: None,
                grammar: None,
                reasoning_format: None,
                chat_template_kwargs: Some("{\"enable_thinking\":false}"),
                add_generation_prompt: true,
                use_jinja: true,
                parallel_tool_calls: false,
                enable_thinking: false,
                add_bos: false,
                add_eos: false,
                parse_tool_calls: false,
            },
        )
        .map_err(|err| {
            ReviewInferenceError::Analysis(format!(
                "failed to apply model chat template for local inference: {err}"
            ))
        })?;

    Ok(RenderedPrompt {
        prompt: template_result.prompt,
        additional_stops: template_result.additional_stops,
    })
}

#[cfg(feature = "llm-local")]
fn render_prompt_with_basic_template(
    model: &LlamaModel,
    template: &llama_cpp_2::model::LlamaChatTemplate,
    base_prompt: &str,
) -> Result<RenderedPrompt, ReviewInferenceError> {
    let system_prompt = if is_qwen_family_model(model) {
        "You are Mr. Milchick, a careful CI code review assistant. Return exactly one compact JSON object. Do not emit <think> tags or reasoning text."
    } else {
        "You are Mr. Milchick, a careful CI code review assistant. Return exactly one compact JSON object."
    };

    let messages = vec![
        LlamaChatMessage::new("system".to_string(), system_prompt.to_string()).map_err(
            |err| {
                ReviewInferenceError::Analysis(format!(
                    "failed to build local inference system message: {err}"
                ))
            },
        )?,
        LlamaChatMessage::new("user".to_string(), base_prompt.to_string()).map_err(|err| {
            ReviewInferenceError::Analysis(format!(
                "failed to build local inference user message: {err}"
            ))
        })?,
    ];

    let prompt = model.apply_chat_template(template, &messages, true).map_err(|err| {
        ReviewInferenceError::Analysis(format!(
            "failed to apply basic model chat template for local inference: {err}"
        ))
    })?;

    Ok(RenderedPrompt {
        prompt,
        additional_stops: Vec::new(),
    })
}

#[cfg(feature = "llm-local")]
fn manual_qwen_prompt(base_prompt: &str) -> String {
    format!(
        "<|im_start|>system\nYou are Mr. Milchick, a careful CI code review assistant. Reply with JSON only. Do not emit <think> tags or reasoning text.<|im_end|>\n<|im_start|>user\n{base_prompt}<|im_end|>\n<|im_start|>assistant\n"
    )
}

#[cfg(feature = "llm-local")]
fn trim_stop_sequence_suffix(output: &mut String, stop_sequences: &[String]) -> bool {
    for stop in stop_sequences {
        if stop.is_empty() {
            continue;
        }

        if output.ends_with(stop) {
            let trimmed_len = output.len().saturating_sub(stop.len());
            output.truncate(trimmed_len);
            return true;
        }
    }

    false
}

#[cfg(feature = "llm-local")]
fn build_local_sampler(model: &LlamaModel) -> LlamaSampler {
    if qwen_prefers_stochastic_decoder(model) {
        // Smaller Qwen-family checkpoints benefited from light stochastic decoding in the
        // benchmark, while the 4B variants regressed. Keep this repeatable with a fixed seed.
        LlamaSampler::chain_simple([
            LlamaSampler::top_k(20),
            LlamaSampler::top_p(0.95, 1),
            LlamaSampler::temp(0.6),
            LlamaSampler::dist(LOCAL_QWEN_SAMPLER_SEED),
        ])
    } else {
        LlamaSampler::greedy()
    }
}

#[cfg(feature = "llm-local")]
fn qwen_prefers_stochastic_decoder(model: &LlamaModel) -> bool {
    if !is_qwen_family_model(model) {
        return false;
    }

    let size_label = model
        .meta_val_str("general.size_label")
        .ok()
        .unwrap_or_default()
        .to_ascii_lowercase();

    if size_label.contains("0.8b") || size_label.contains("1b") || size_label.contains("2b") {
        return true;
    }

    model.n_params() > 0 && model.n_params() < 3_000_000_000
}

#[cfg(feature = "llm-local")]
fn build_local_model_params() -> Result<LlamaModelParams, ReviewInferenceError> {
    let params = LlamaModelParams::default()
        .with_n_gpu_layers(0)
        .with_split_mode(LlamaSplitMode::None);
    configure_cpu_model_params(
        params,
        select_cpu_backend_device_index(&list_llama_ggml_backend_devices()),
    )
}

#[cfg(feature = "llm-local")]
fn configure_cpu_model_params(
    params: LlamaModelParams,
    cpu_device_index: Option<usize>,
) -> Result<LlamaModelParams, ReviewInferenceError> {
    match cpu_device_index {
        Some(cpu_device_index) => params.with_devices(&[cpu_device_index]).map_err(|err| {
            ReviewInferenceError::Analysis(format!(
                "failed to select CPU backend device {cpu_device_index}: {err}"
            ))
        }),
        None => Ok(params),
    }
}

#[cfg(feature = "llm-local")]
fn select_cpu_backend_device_index(devices: &[LlamaBackendDevice]) -> Option<usize> {
    devices
        .iter()
        .find(|device| matches!(device.device_type, LlamaBackendDeviceType::Cpu))
        .map(|device| device.index)
}

#[cfg(feature = "llm-local")]
fn decode_prompt_tokens(
    context: &mut LlamaContext<'_>,
    prompt_tokens: &[llama_cpp_2::token::LlamaToken],
    batch_capacity: usize,
    deadline: Instant,
    timeout: Duration,
) -> Result<(), ReviewInferenceError> {
    let total_chunks = prompt_tokens.chunks(batch_capacity).len();
    let mut absolute_position = 0usize;

    for (chunk_index, chunk) in prompt_tokens.chunks(batch_capacity).enumerate() {
        ensure_within_deadline(deadline, timeout)?;
        let is_last_chunk = chunk_index + 1 == total_chunks;
        let mut batch = LlamaBatch::new(chunk.len(), 1);

        for (offset, token) in chunk.iter().enumerate() {
            let logits = is_last_chunk && offset + 1 == chunk.len();
            let position = i32::try_from(absolute_position + offset).map_err(|_| {
                ReviewInferenceError::Analysis(
                    "prompt exceeded llama positional limits".to_string(),
                )
            })?;
            batch
                .add(*token, position, &[0], logits)
                .map_err(batch_add_error)?;
        }

        context.decode(&mut batch).map_err(|err| {
            ReviewInferenceError::Analysis(format!("failed to decode review prompt: {err}"))
        })?;
        absolute_position += chunk.len();
    }

    Ok(())
}

#[cfg(feature = "llm-local")]
fn ensure_within_deadline(
    deadline: Instant,
    timeout: Duration,
) -> Result<(), ReviewInferenceError> {
    if Instant::now() >= deadline {
        Err(ReviewInferenceError::TimedOut(timeout))
    } else {
        Ok(())
    }
}

#[cfg(feature = "llm-local")]
fn token_to_piece_lossy(
    model: &LlamaModel,
    token: llama_cpp_2::token::LlamaToken,
) -> Result<String, ReviewInferenceError> {
    let bytes = match model.token_to_piece_bytes(token, 8, false, None) {
        Ok(bytes) => bytes,
        Err(TokenToStringError::InsufficientBufferSpace(size)) => model
            .token_to_piece_bytes(
                token,
                (-size)
                    .try_into()
                    .expect("reported token size should be positive"),
                false,
                None,
            )
            .map_err(token_error)?,
        Err(err) => return Err(token_error(err)),
    };

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(feature = "llm-local")]
fn batch_add_error(err: BatchAddError) -> ReviewInferenceError {
    ReviewInferenceError::Analysis(format!("failed to build llama token batch: {err}"))
}

#[cfg(feature = "llm-local")]
fn token_error(err: TokenToStringError) -> ReviewInferenceError {
    ReviewInferenceError::Analysis(format!("failed to decode llama token output: {err}"))
}

#[cfg(feature = "llm-local")]
fn recommended_thread_count() -> i32 {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(4)
        .min(i32::MAX as usize) as i32
}

#[cfg(feature = "llm-local")]
fn requested_context_size(
    prompt_token_count: usize,
    requested_context_token_limit: usize,
) -> usize {
    let requested = prompt_token_count
        .saturating_add(LOCAL_MAX_GENERATION_TOKENS)
        .saturating_add(64);
    requested
        .next_power_of_two()
        .clamp(1_024, requested_context_token_limit.max(1_024))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        Actor, ChangeType, ChangedFile, RepositoryRef, ReviewMetadata, ReviewPlatformKind,
        ReviewRef,
    };

    fn sample_snapshot() -> ReviewSnapshot {
        ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitLab,
                project_key: "123".to_string(),
                review_id: "456".to_string(),
                web_url: None,
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitLab,
                namespace: "group".to_string(),
                name: "project".to_string(),
                web_url: None,
            },
            title: "Test".to_string(),
            description: None,
            author: Actor {
                username: "alice".to_string(),
                display_name: None,
            },
            participants: Vec::new(),
            changed_files: vec![ChangedFile {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                change_type: ChangeType::Modified,
                additions: Some(12),
                deletions: Some(3),
                patch: Some("@@ -1,2 +1,2 @@".to_string()),
            }],
            labels: Vec::new(),
            is_draft: false,
            default_branch: Some("main".to_string()),
            metadata: ReviewMetadata::default(),
        }
    }

    struct SleepyEngine;

    #[async_trait]
    impl ReviewInferenceEngine for SleepyEngine {
        async fn analyze(
            &self,
            _snapshot: &ReviewSnapshot,
        ) -> Result<ReviewInferenceOutcome, ReviewInferenceError> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(ReviewInferenceOutcome::ready(ReviewInsights::default()))
        }
    }

    #[tokio::test]
    async fn noop_engine_returns_configured_disabled_outcome() {
        let engine = NoopReviewInferenceEngine::disabled("disabled by configuration");
        let outcome = engine
            .analyze(&sample_snapshot())
            .await
            .expect("noop engine should not fail");

        assert_eq!(outcome.status, ReviewInferenceStatus::Disabled);
        assert_eq!(outcome.detail.as_deref(), Some("disabled by configuration"));
    }

    #[tokio::test]
    async fn analyze_with_timeout_reports_timeout_when_engine_takes_too_long() {
        let outcome =
            analyze_with_timeout(&SleepyEngine, &sample_snapshot(), Duration::from_millis(5)).await;

        assert_eq!(outcome.status, ReviewInferenceStatus::TimedOut);
        assert!(
            outcome
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("timed out")
        );
    }

    #[test]
    fn prompt_builder_truncates_patch_budget_across_files() {
        let mut snapshot = sample_snapshot();
        snapshot.changed_files.push(ChangedFile {
            path: "src/other.rs".to_string(),
            previous_path: Some("src/old.rs".to_string()),
            change_type: ChangeType::Renamed,
            additions: Some(1),
            deletions: Some(1),
            patch: Some("second patch".to_string()),
        });

        let prompt = build_local_inference_prompt(&snapshot, 8, LocalPromptStyle::Primary);

        assert!(prompt.contains("File: src/lib.rs [Modified]"));
        assert!(prompt.contains("[patch truncated to fit configured patch budget]"));
        assert!(prompt.contains("Patch: [omitted after reaching configured patch budget]"));
        assert!(prompt.contains("Previous path: src/old.rs"));
    }

    #[test]
    fn parses_json_payload_with_wrapper_text() {
        let insights = parse_local_inference_output(
            "Here you go {\"summary\":\"Touches reviewer routing\",\"recommendations\":[{\"category\":\"ReviewFocus\",\"message\":\"Check whether reviewer selection stays deterministic.\"},{\"category\":\"Risk\",\"message\":\"Watch for duplicate reviewers when existing participants are merged.\"}]} trailing",
        )
        .expect("output should parse");

        assert_eq!(
            insights.summary.as_deref(),
            Some("Touches reviewer routing")
        );
        assert_eq!(insights.recommendations.len(), 2);
        assert_eq!(
            insights.recommendations[0].category,
            RecommendationCategory::ReviewFocus
        );
    }

    #[test]
    fn parser_skips_non_schema_braces_before_valid_json() {
        let insights = parse_local_inference_output(
            "notes { role } more notes {\"summary\":\"Auth middleware was removed.\",\"recommendations\":[{\"category\":\"Risk\",\"message\":\"Verify the route still enforces admin permissions.\"}]} trailing",
        )
        .expect("parser should skip non-schema brace groups");

        assert_eq!(
            insights.summary.as_deref(),
            Some("Auth middleware was removed.")
        );
        assert_eq!(insights.recommendations.len(), 1);
        assert_eq!(
            insights.recommendations[0].category,
            RecommendationCategory::Risk
        );
    }

    #[test]
    fn parseable_response_detection_ignores_non_schema_braces() {
        assert!(!contains_parseable_review_response(
            "prefix { role } suffix"
        ));
        assert!(contains_parseable_review_response(
            "prefix { role } suffix {\"summary\":null,\"recommendations\":[]}"
        ));
    }

    #[test]
    fn first_non_empty_review_response_skips_empty_json_objects() {
        let insights = first_non_empty_review_response(
            "{\"summary\":null,\"recommendations\":[]} trailing {\"summary\":\"Auth middleware was removed.\",\"recommendations\":[{\"category\":\"Risk\",\"message\":\"Restore the admin guard.\"}]}",
        )
        .expect("should find the later non-empty response");

        assert_eq!(
            insights.summary.as_deref(),
            Some("Auth middleware was removed.")
        );
        assert_eq!(insights.recommendations.len(), 1);
    }

    #[test]
    fn prompt_builder_surfaces_inferred_review_cues() {
        let snapshot = ReviewSnapshot {
            review_ref: ReviewRef {
                platform: ReviewPlatformKind::GitHub,
                project_key: "ArthurianX/mr-milchick".to_string(),
                review_id: "42".to_string(),
                web_url: None,
            },
            repository: RepositoryRef {
                platform: ReviewPlatformKind::GitHub,
                namespace: "ArthurianX".to_string(),
                name: "mr-milchick".to_string(),
                web_url: None,
            },
            title: "Test".to_string(),
            description: None,
            author: Actor {
                username: "alice".to_string(),
                display_name: None,
            },
            participants: Vec::new(),
            changed_files: vec![
                ChangedFile {
                    path: "apps/api/routes/admin.js".to_string(),
                    previous_path: None,
                    change_type: ChangeType::Modified,
                    additions: Some(0),
                    deletions: Some(2),
                    patch: Some("-const requireAdmin = require(\"../auth/requireAdmin\");\n-router.post(\"/users/:id/role\", requireAdmin, updateRole);".to_string()),
                },
                ChangedFile {
                    path: "apps/frontend/src/ProfileBio.tsx".to_string(),
                    previous_path: None,
                    change_type: ChangeType::Modified,
                    additions: Some(1),
                    deletions: Some(0),
                    patch: Some("+return <div dangerouslySetInnerHTML={{ __html: bioHtml }} />;".to_string()),
                },
                ChangedFile {
                    path: "apps/frontend/src/ProfileBio.test.tsx".to_string(),
                    previous_path: None,
                    change_type: ChangeType::Deleted,
                    additions: Some(0),
                    deletions: Some(1),
                    patch: Some("-expect(screen.getByText(\"Loading...\")).toBeInTheDocument();".to_string()),
                },
            ],
            labels: Vec::new(),
            is_draft: false,
            default_branch: Some("main".to_string()),
            metadata: ReviewMetadata::default(),
        };

        let prompt = build_local_inference_prompt(&snapshot, 4_096, LocalPromptStyle::StrictRetry);

        assert!(prompt.contains("Potential review cues from the diff:"));
        assert!(prompt.contains("A test file was deleted"));
        assert!(prompt.contains("auth, permission, or validation logic"));
        assert!(prompt.contains("raw HTML rendering"));
        assert!(prompt.contains("The first character of your reply must be {"));
    }

    #[test]
    fn parsing_ignores_schema_placeholder_summary_text() {
        let insights =
            parse_local_inference_output("{\"summary\":\"string or null\",\"recommendations\":[]}")
                .expect("output should parse");

        assert!(insights.summary.is_none());
        assert!(insights.recommendations.is_empty());
    }

    #[test]
    fn parsing_skips_empty_and_unknown_recommendations() {
        let insights = parse_local_inference_output(
            "{\"summary\":\" \",\"recommendations\":[{\"category\":\"unknown\",\"message\":\"skip me\"},{\"category\":\"TestGap\",\"message\":\"Add coverage for timeout handling.\"},{\"category\":\"Risk\",\"message\":\"   \"}]}",
        )
        .expect("output should parse");

        assert!(insights.summary.is_none());
        assert_eq!(insights.recommendations.len(), 1);
        assert_eq!(
            insights.recommendations[0].category,
            RecommendationCategory::TestGap
        );
    }

    #[cfg(feature = "llm-local")]
    #[test]
    fn configure_cpu_model_params_disables_gpu_split_when_no_cpu_device_is_provided() {
        let params = configure_cpu_model_params(
            LlamaModelParams::default()
                .with_n_gpu_layers(0)
                .with_split_mode(LlamaSplitMode::None),
            None,
        )
        .expect("configuring CPU-only params without an explicit CPU device should succeed");

        assert_eq!(params.n_gpu_layers(), 0);
        assert_eq!(
            params.split_mode().expect("split mode should parse"),
            LlamaSplitMode::None
        );
        assert!(params.devices().is_empty());
    }

    #[cfg(feature = "llm-local")]
    #[test]
    fn select_cpu_backend_device_index_prefers_cpu_devices() {
        let devices = vec![
            LlamaBackendDevice {
                index: 0,
                name: "Metal".to_string(),
                description: "Apple GPU".to_string(),
                backend: "MTL".to_string(),
                memory_total: 0,
                memory_free: 0,
                device_type: LlamaBackendDeviceType::Gpu,
            },
            LlamaBackendDevice {
                index: 1,
                name: "CPU".to_string(),
                description: "Apple M1 Max".to_string(),
                backend: "CPU".to_string(),
                memory_total: 0,
                memory_free: 0,
                device_type: LlamaBackendDeviceType::Cpu,
            },
        ];

        assert_eq!(select_cpu_backend_device_index(&devices), Some(1));
    }

    #[cfg(feature = "llm-local")]
    #[test]
    fn local_engine_requires_existing_model_file() {
        let err = LocalLlamaReviewInferenceEngine::new(
            "/tmp/does-not-exist.gguf",
            1024,
            4096,
            Duration::from_secs(1),
        )
        .expect_err("missing GGUF path should fail");

        assert!(err.to_string().contains("does not point to a GGUF file"));
    }
}
