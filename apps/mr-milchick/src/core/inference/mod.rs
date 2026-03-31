use std::time::Duration;

use async_trait::async_trait;
#[cfg(any(feature = "llm-local", test))]
use serde::Deserialize;
use tokio::time::timeout;

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
use llama_cpp_2::model::{AddBos, LlamaModel};
#[cfg(feature = "llm-local")]
use llama_cpp_2::sampling::LlamaSampler;

#[cfg(feature = "llm-local")]
const LOCAL_CONTEXT_TOKEN_LIMIT: usize = 4_096;
#[cfg(feature = "llm-local")]
const LOCAL_BATCH_CAPACITY: usize = 512;
#[cfg(feature = "llm-local")]
const LOCAL_MAX_GENERATION_TOKENS: usize = 384;
#[cfg(any(feature = "llm-local", test))]
const LOCAL_MAX_RECOMMENDATIONS: usize = 4;

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
}

#[async_trait]
pub trait ReviewInferenceEngine: Send + Sync {
    async fn analyze(
        &self,
        snapshot: &ReviewSnapshot,
    ) -> Result<ReviewInferenceOutcome, ReviewInferenceError>;
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
    match timeout(timeout_duration, engine.analyze(snapshot)).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(err)) => ReviewInferenceOutcome::failed(err.to_string()),
        Err(_) => ReviewInferenceOutcome::timed_out(timeout_duration),
    }
}

#[cfg(feature = "llm-local")]
#[derive(Debug, Clone)]
pub struct LocalLlamaReviewInferenceEngine {
    model_path: PathBuf,
    max_patch_bytes: usize,
}

#[cfg(feature = "llm-local")]
impl LocalLlamaReviewInferenceEngine {
    pub fn new(
        model_path: impl Into<PathBuf>,
        max_patch_bytes: usize,
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
        let prompt = build_local_inference_prompt(snapshot, self.max_patch_bytes);
        let model_path = self.model_path.clone();

        task::spawn_blocking(move || {
            run_local_inference(model_path.as_path(), &prompt).map(ReviewInferenceOutcome::ready)
        })
        .await
        .map_err(|err| {
            ReviewInferenceError::Analysis(format!("local inference task failed: {err}"))
        })?
    }
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
fn build_local_inference_prompt(snapshot: &ReviewSnapshot, max_patch_bytes: usize) -> String {
    let labels = if snapshot.labels.is_empty() {
        "none".to_string()
    } else {
        snapshot.labels.join(", ")
    };
    let mut prompt = String::new();
    prompt.push_str("You are Mr. Milchick, a careful code review assistant running inside CI.\n");
    prompt.push_str("Analyze the review snapshot and return JSON only.\n");
    prompt.push_str(
        "Schema: {\"summary\":\"string or null\",\"recommendations\":[{\"category\":\"ReviewFocus|Risk|TestGap\",\"message\":\"string\"}]}\n",
    );
    prompt.push_str("Rules:\n");
    prompt.push_str("- Keep the output advisory and recommendation-only.\n");
    prompt.push_str("- Base conclusions only on the supplied snapshot and diff excerpts.\n");
    prompt.push_str("- Use at most 4 recommendations.\n");
    prompt.push_str("- Keep each recommendation message under 160 characters.\n");
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
fn parse_local_inference_output(raw: &str) -> Result<ReviewInsights, ReviewInferenceError> {
    let json_payload = extract_first_json_object(raw).ok_or_else(|| {
        ReviewInferenceError::Analysis(
            "local review model did not return a JSON object".to_string(),
        )
    })?;

    let response: LlmReviewResponse = serde_json::from_str(json_payload).map_err(|err| {
        ReviewInferenceError::Analysis(format!("local review model returned invalid JSON: {err}"))
    })?;

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

    Ok(ReviewInsights {
        summary,
        recommendations,
    })
}

#[cfg(any(feature = "llm-local", test))]
fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let normalized = value.trim();
        if normalized.is_empty() || normalized.eq_ignore_ascii_case("null") {
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

#[cfg(any(feature = "llm-local", test))]
fn extract_first_json_object(raw: &str) -> Option<&str> {
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
                    let start = start_index?;
                    return Some(&raw[start..index + ch.len_utf8()]);
                }
            }
            _ => {}
        }
    }

    None
}

#[cfg(feature = "llm-local")]
fn run_local_inference(
    model_path: &Path,
    prompt: &str,
) -> Result<ReviewInsights, ReviewInferenceError> {
    let mut backend = LlamaBackend::init().map_err(|err| {
        ReviewInferenceError::Analysis(format!("failed to initialize llama backend: {err}"))
    })?;
    backend.void_logs();

    let model_params = build_local_model_params()?;
    let model = LlamaModel::load_from_file(&backend, model_path, &model_params).map_err(|err| {
        ReviewInferenceError::Analysis(format!(
            "failed to load GGUF model '{}': {err}",
            model_path.display()
        ))
    })?;

    let prompt_tokens = model.str_to_token(prompt, AddBos::Always).map_err(|err| {
        ReviewInferenceError::Analysis(format!("failed to tokenize review prompt: {err}"))
    })?;

    if prompt_tokens.is_empty() {
        return Err(ReviewInferenceError::Analysis(
            "review prompt tokenized to an empty sequence".to_string(),
        ));
    }

    let context_token_limit = requested_context_size(prompt_tokens.len());
    let batch_capacity = prompt_tokens.len().clamp(1, LOCAL_BATCH_CAPACITY);
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

    let threads = recommended_thread_count();
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

    decode_prompt_tokens(&mut context, &prompt_tokens, batch_capacity)?;

    let mut sampler = LlamaSampler::greedy();
    sampler.accept_many(prompt_tokens.iter());

    let mut rendered_output = String::new();
    let mut position = prompt_tokens.len();

    for _ in 0..LOCAL_MAX_GENERATION_TOKENS {
        let token = sampler.sample(&context, -1);
        if model.is_eog_token(token) {
            break;
        }

        sampler.accept(token);
        rendered_output.push_str(&token_to_piece_lossy(&model, token)?);
        if extract_first_json_object(&rendered_output).is_some() {
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
) -> Result<(), ReviewInferenceError> {
    let total_chunks = prompt_tokens.chunks(batch_capacity).len();
    let mut absolute_position = 0usize;

    for (chunk_index, chunk) in prompt_tokens.chunks(batch_capacity).enumerate() {
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
fn requested_context_size(prompt_token_count: usize) -> usize {
    let requested = prompt_token_count
        .saturating_add(LOCAL_MAX_GENERATION_TOKENS)
        .saturating_add(64);
    requested
        .next_power_of_two()
        .clamp(1_024, LOCAL_CONTEXT_TOKEN_LIMIT)
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

        let prompt = build_local_inference_prompt(&snapshot, 8);

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
        let err = LocalLlamaReviewInferenceEngine::new("/tmp/does-not-exist.gguf", 1024)
            .expect_err("missing GGUF path should fail");

        assert!(err.to_string().contains("does not point to a GGUF file"));
    }
}
