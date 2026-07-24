# Claude Code Remote Inference Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an explicitly selected, snapshot-only Claude Code advisory-review backend that authenticates with `CLAUDE_CODE_OAUTH_TOKEN` and preserves local inference behavior.

**Architecture:** Keep `ReviewInferenceEngine` as the provider boundary, extract the current prompt and output parsing into backend-neutral modules, and put llama.cpp and Claude Code behavior in focused provider modules. Resolve TOML into a backend-specific enum, isolate the Claude subprocess from the repository and unrelated CI secrets, and feed both providers into the existing `explain` rendering path.

**Tech Stack:** Rust 2024, Tokio process and async-I/O support, Serde/serde_json, llama-cpp-2, Claude Code CLI 2.1.218+, GitLab CI, TOML.

## Global Constraints

- Remote inference runs only during `explain`; `observe` and `refine` never wire it.
- `backend` is explicit for Claude Code; omitted `backend` remains backward-compatible local inference.
- `CLAUDE_CODE_OAUTH_TOKEN` is the only Claude credential and must never be logged, rendered, placed in TOML, or forwarded with unrelated CI secrets.
- Claude Code receives only the existing bounded review snapshot and has tools, skills, plugins, MCP discovery, project instructions, and session persistence disabled.
- There is no direct Anthropic Messages API integration and no `ANTHROPIC_API_KEY` support.
- There is no local/remote fallback.
- Operational inference failure is advisory and fail-open; invalid configuration still fails startup.
- Local timeout defaults to 15,000 ms; Claude Code timeout defaults to 120,000 ms.
- Claude Code model configuration accepts any non-empty CLI-supported value, while documentation recommends a full versioned model identifier.
- Claude Code 2.1.218 is the minimum supported CLI and the pinned demo version.
- All shell commands in this repository must be prefixed with `rtk`.

---

## File Structure

### Create

- `apps/mr-milchick/src/core/inference/prompt.rs`: shared bounded snapshot prompt, strict retry form, and JSON schema.
- `apps/mr-milchick/src/core/inference/output.rs`: shared response DTOs, extraction, validation, and normalization.
- `apps/mr-milchick/src/core/inference/local.rs`: all llama.cpp-specific engine and decoding behavior moved from the current module.
- `apps/mr-milchick/src/core/inference/claude_code.rs`: isolated CLI runner, readiness checks, Claude JSON envelope handling, and remote engine.
- `examples/gitlab/claude-code-explain.gitlab-ci.yml`: pinned, same-project-only GitLab consumer example.
- `docs/claude-code-llm.md`: focused remote backend setup and security guide.

### Modify

- `Cargo.toml`: Tokio process/I/O features and `llm-claude-code`.
- `Cargo.lock`: dependency feature resolution after the Tokio feature change.
- `.gitlab-ci.yml`: compile `llm-claude-code` into release artifacts.
- `apps/mr-milchick/src/config/schema.rs`: TOML backend enum and `model`.
- `apps/mr-milchick/src/config/env.rs`: load `CLAUDE_CODE_OAUTH_TOKEN` through the secret boundary.
- `apps/mr-milchick/src/config/resolver.rs`: backend-specific resolved config, validation, and timeouts.
- `apps/mr-milchick/src/config/mod.rs`: export resolved inference types and compiled capability helpers.
- `apps/mr-milchick/src/core/inference/mod.rs`: retain common contract/timeout code and export provider modules.
- `apps/mr-milchick/src/app.rs`: select exactly one compiled backend and report both capabilities.
- `apps/mr-milchick/src/core/message_templates.rs`: backend-neutral recommendation heading.
- `tests/cli_integration.rs`: supported-secret and capability regression coverage.
- `tests/llm_local_smoke.rs`: import the moved local engine without changing live smoke semantics.
- `mr-milchick.toml`: document mutually exclusive local and Claude Code configuration.
- `docs/config-reference.md`: backend matrix, defaults, validation, and secret input.
- `docs/ci-quickstart.md`: remote explain job and example link.
- `docs/connectors-and-capabilities.md`: feature/build/runtime requirements.
- `docs/architecture.md`: provider-neutral inference flow.
- `docs/local-llm.md`: scope clarification and remote cross-link.
- `docs/README.md`: remote guide navigation.

---

### Task 1: Resolve Backend-Specific Configuration and Secret Input

**Files:**
- Modify: `apps/mr-milchick/src/config/schema.rs`
- Modify: `apps/mr-milchick/src/config/env.rs`
- Modify: `apps/mr-milchick/src/config/resolver.rs`
- Modify: `apps/mr-milchick/src/config/mod.rs`

**Interfaces:**
- Consumes: existing `schema::InferenceConfig`, `SecretEnv`, and `resolve_config`.
- Produces: `InferenceBackendKind`, `ResolvedInferenceBackend`, `InferenceConfig::is_enabled()`, and `InferenceConfig::kind()`.

- [ ] **Step 1: Add failing resolver tests for default-local compatibility and backend-specific defaults**

Add these tests to `apps/mr-milchick/src/config/resolver.rs`:

```rust
#[test]
fn inference_without_backend_remains_local() {
    let file = toml::from_str::<schema::ConfigFile>(
        r#"
[inference]
enabled = true
model_path = "/models/review.gguf"
"#,
    )
    .expect("config should parse");

    let config = resolve_config(file, SecretEnv::default()).expect("config should resolve");

    assert_eq!(config.inference.kind(), InferenceBackendKind::Local);
    assert_eq!(config.inference.timeout_ms, 15_000);
    assert!(matches!(
        config.inference.backend,
        ResolvedInferenceBackend::Local {
            ref model_path,
            context_tokens: 4_096,
        } if model_path == "/models/review.gguf"
    ));
}

#[test]
fn claude_code_uses_remote_timeout_and_pipeline_token() {
    let file = toml::from_str::<schema::ConfigFile>(
        r#"
[inference]
enabled = true
backend = "claude-code"
model = "claude-versioned-model"
"#,
    )
    .expect("config should parse");

    let config = resolve_config(
        file,
        SecretEnv {
            claude_code_oauth_token: Some("oauth-secret".to_string()),
            ..SecretEnv::default()
        },
    )
    .expect("config should resolve");

    assert_eq!(config.inference.kind(), InferenceBackendKind::ClaudeCode);
    assert_eq!(config.inference.timeout_ms, 120_000);
    assert!(matches!(
        config.inference.backend,
        ResolvedInferenceBackend::ClaudeCode {
            ref model,
            oauth_token: Some(ref token),
        } if model == "claude-versioned-model" && token == "oauth-secret"
    ));
}
```

- [ ] **Step 2: Run the two tests and confirm the schema/types are absent**

Run:

```bash
rtk cargo test inference_without_backend_remains_local --locked
rtk cargo test claude_code_uses_remote_timeout_and_pipeline_token --locked
```

Expected: compilation fails because `backend`, `model`, `claude_code_oauth_token`, `InferenceBackendKind`, and `ResolvedInferenceBackend` do not exist.

- [ ] **Step 3: Add schema and secret-boundary types**

In `apps/mr-milchick/src/config/schema.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum InferenceBackendKind {
    #[default]
    Local,
    ClaudeCode,
}
```

Extend the schema-level `InferenceConfig` with:

```rust
#[serde(default)]
pub backend: InferenceBackendKind,
#[serde(default)]
pub model: Option<String>,
```

In `apps/mr-milchick/src/config/env.rs`, add:

```rust
pub const CLAUDE_CODE_OAUTH_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";
```

Extend `SecretEnv` and `load_secret_env()`:

```rust
pub struct SecretEnv {
    pub gitlab_token: Option<String>,
    pub github_token: Option<String>,
    pub slack_bot_token: Option<String>,
    pub claude_code_oauth_token: Option<String>,
}

pub fn load_secret_env() -> SecretEnv {
    SecretEnv {
        gitlab_token: read_optional_env(GITLAB_TOKEN_ENV),
        github_token: read_optional_env(GITHUB_TOKEN_ENV),
        slack_bot_token: read_optional_env(SLACK_BOT_TOKEN_ENV),
        claude_code_oauth_token: read_optional_env(CLAUDE_CODE_OAUTH_TOKEN_ENV),
    }
}
```

Do not add `CLAUDE_CODE_OAUTH_TOKEN` to `REMOVED_APP_CONFIG_ENV_VARS`; it is a supported secret, not runtime configuration.

- [ ] **Step 4: Add resolved backend types and backend-specific validation**

In `apps/mr-milchick/src/config/resolver.rs`, replace the flat inference fields with:

```rust
pub use super::schema::InferenceBackendKind;

const DEFAULT_LOCAL_LLM_TIMEOUT_MS: u64 = 15_000;
const DEFAULT_CLAUDE_CODE_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedInferenceBackend {
    Disabled {
        selected: InferenceBackendKind,
    },
    Local {
        model_path: String,
        context_tokens: usize,
    },
    ClaudeCode {
        model: String,
        oauth_token: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceConfig {
    pub backend: ResolvedInferenceBackend,
    pub timeout_ms: u64,
    pub max_patch_bytes: usize,
    pub trace: bool,
}

impl InferenceConfig {
    pub fn is_enabled(&self) -> bool {
        !matches!(self.backend, ResolvedInferenceBackend::Disabled { .. })
    }

    pub fn kind(&self) -> InferenceBackendKind {
        match self.backend {
            ResolvedInferenceBackend::Disabled { selected } => selected,
            ResolvedInferenceBackend::Local { .. } => InferenceBackendKind::Local,
            ResolvedInferenceBackend::ClaudeCode { .. } => InferenceBackendKind::ClaudeCode,
        }
    }
}
```

Change `resolve_config` to pass secrets:

```rust
inference: resolve_inference_config(&file.inference, &secrets)?,
```

Implement `resolve_inference_config` with these exact rules:

```rust
fn resolve_inference_config(
    file: &schema::InferenceConfig,
    secrets: &SecretEnv,
) -> Result<InferenceConfig> {
    let enabled = file.enabled.unwrap_or(false);
    let model_path = sanitize_optional(file.model_path.clone());
    let model = sanitize_optional(file.model.clone());

    match file.backend {
        schema::InferenceBackendKind::Local if model.is_some() => {
            bail!("'inference.model' is only valid for backend 'claude-code'");
        }
        schema::InferenceBackendKind::ClaudeCode if model_path.is_some() => {
            bail!("'inference.model_path' is only valid for backend 'local'");
        }
        schema::InferenceBackendKind::ClaudeCode if file.context_tokens.is_some() => {
            bail!("'inference.context_tokens' is only valid for backend 'local'");
        }
        _ => {}
    }

    let default_timeout = match file.backend {
        schema::InferenceBackendKind::Local => DEFAULT_LOCAL_LLM_TIMEOUT_MS,
        schema::InferenceBackendKind::ClaudeCode => DEFAULT_CLAUDE_CODE_TIMEOUT_MS,
    };
    let timeout_ms =
        resolve_positive_u64(file.timeout_ms, default_timeout, "inference.timeout_ms")?;
    let max_patch_bytes = resolve_positive_usize(
        file.max_patch_bytes,
        DEFAULT_LLM_MAX_PATCH_BYTES,
        "inference.max_patch_bytes",
    )?;

    let backend = if !enabled {
        ResolvedInferenceBackend::Disabled {
            selected: file.backend,
        }
    } else {
        match file.backend {
            schema::InferenceBackendKind::Local => ResolvedInferenceBackend::Local {
                model_path: model_path.ok_or_else(|| {
                    anyhow!("backend 'local' requires non-empty 'inference.model_path'")
                })?,
                context_tokens: resolve_positive_usize(
                    file.context_tokens,
                    DEFAULT_LLM_CONTEXT_TOKENS,
                    "inference.context_tokens",
                )?,
            },
            schema::InferenceBackendKind::ClaudeCode => {
                ResolvedInferenceBackend::ClaudeCode {
                    model: model.ok_or_else(|| {
                        anyhow!("backend 'claude-code' requires non-empty 'inference.model'")
                    })?,
                    oauth_token: secrets.claude_code_oauth_token.clone(),
                }
            }
        }
    };

    Ok(InferenceConfig {
        backend,
        timeout_ms,
        max_patch_bytes,
        trace: file.trace.unwrap_or(false),
    })
}
```

- [ ] **Step 5: Add failing validation-matrix tests, then make them pass**

Add table-driven cases in `resolver.rs` for:

```rust
#[test]
fn inference_rejects_backend_specific_field_mismatches() {
    for (raw, expected) in [
        (
            "[inference]\nenabled = true\nbackend = \"local\"\nmodel_path = \"/m.gguf\"\nmodel = \"sonnet\"\n",
            "'inference.model' is only valid for backend 'claude-code'",
        ),
        (
            "[inference]\nenabled = true\nbackend = \"claude-code\"\nmodel = \"sonnet\"\nmodel_path = \"/m.gguf\"\n",
            "'inference.model_path' is only valid for backend 'local'",
        ),
        (
            "[inference]\nenabled = true\nbackend = \"claude-code\"\nmodel = \"sonnet\"\ncontext_tokens = 4096\n",
            "'inference.context_tokens' is only valid for backend 'local'",
        ),
        (
            "[inference]\nenabled = true\nbackend = \"claude-code\"\nmodel = \"   \"\n",
            "backend 'claude-code' requires non-empty 'inference.model'",
        ),
    ] {
        let file = toml::from_str::<schema::ConfigFile>(raw).expect("config should parse");
        let error = resolve_config(file, SecretEnv::default()).expect_err("config should fail");
        assert!(error.to_string().contains(expected), "{error:#}");
    }
}
```

Run:

```bash
rtk cargo test inference_ --locked
```

Expected: all inference resolver tests pass.

- [ ] **Step 6: Export the new resolved types and update existing fixtures**

In `apps/mr-milchick/src/config/mod.rs`, export:

```rust
InferenceBackendKind, InferenceConfig, ResolvedInferenceBackend
```

Update all explicit `SecretEnv` fixtures to end with `..SecretEnv::default()` and replace flat `InferenceConfig` fixtures with:

```rust
InferenceConfig {
    backend: ResolvedInferenceBackend::Disabled {
        selected: InferenceBackendKind::Local,
    },
    timeout_ms: 15_000,
    max_patch_bytes: 32 * 1024,
    trace: false,
}
```

Run:

```bash
rtk cargo test config:: --locked
```

Expected: all configuration tests pass.

- [ ] **Step 7: Commit the configuration boundary**

```bash
rtk git add apps/mr-milchick/src/config
rtk git commit -m "feat: resolve selectable inference backends"
```

---

### Task 2: Extract Shared Prompt/Output Logic and Preserve Local Behavior

**Files:**
- Create: `apps/mr-milchick/src/core/inference/prompt.rs`
- Create: `apps/mr-milchick/src/core/inference/output.rs`
- Create: `apps/mr-milchick/src/core/inference/local.rs`
- Modify: `apps/mr-milchick/src/core/inference/mod.rs`
- Modify: `tests/llm_local_smoke.rs`

**Interfaces:**
- Consumes: existing `ReviewSnapshot`, inference result types, and llama.cpp implementation.
- Produces: `build_review_prompt(snapshot, max_patch_bytes, ReviewPromptStyle)`, `REVIEW_JSON_SCHEMA`, `parse_review_output(raw)`, `is_parseable_review_response(raw)`, and unchanged `LocalLlamaReviewInferenceEngine`.

- [ ] **Step 1: Add backend-neutral prompt/output tests before moving code**

Move the existing prompt/parser test fixtures into new inline test modules and rename their calls:

```rust
let prompt = build_review_prompt(&snapshot, 8, ReviewPromptStyle::Primary);
assert!(prompt.contains("File: src/lib.rs [Modified]"));
assert!(prompt.contains("[patch truncated to fit configured patch budget]"));
assert!(prompt.contains("Patch: [omitted after reaching configured patch budget]"));
```

Add a schema test in `prompt.rs`:

```rust
#[test]
fn review_schema_accepts_the_shared_response_shape() {
    let schema: serde_json::Value =
        serde_json::from_str(REVIEW_JSON_SCHEMA).expect("schema should be valid JSON");
    assert_eq!(schema["type"], "object");
    assert_eq!(
        schema["properties"]["recommendations"]["items"]["properties"]["category"]["enum"],
        serde_json::json!(["ReviewFocus", "Risk", "TestGap"])
    );
}
```

Add an output test in `output.rs`:

```rust
#[test]
fn parses_backend_neutral_review_output() {
    let insights = parse_review_output(
        r#"{"summary":"Focus on auth.","recommendations":[{"category":"Risk","message":"Validate the token scope."}]}"#,
    )
    .expect("output should parse");

    assert_eq!(insights.summary.as_deref(), Some("Focus on auth."));
    assert_eq!(insights.recommendations.len(), 1);
    assert_eq!(
        insights.recommendations[0].category,
        RecommendationCategory::Risk
    );
}
```

- [ ] **Step 2: Run inference tests and confirm the new modules/functions are absent**

Run:

```bash
rtk cargo test core::inference --locked
```

Expected: compilation fails because `prompt`, `output`, and the backend-neutral names do not exist.

- [ ] **Step 3: Create `prompt.rs` by moving the existing prompt behavior verbatim**

Move these items from the current `core/inference/mod.rs` into `prompt.rs`:

```text
LocalPromptStyle -> ReviewPromptStyle
build_local_inference_prompt -> build_review_prompt
truncate_utf8
format_change_type
infer_review_cues
looks_like_test_path
contains_any_ascii_case_insensitive
```

Expose only:

```rust
pub(crate) const REVIEW_JSON_SCHEMA: &str = r#"{
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReviewPromptStyle {
    Primary,
    StrictRetry,
}

pub(crate) fn build_review_prompt(
    snapshot: &ReviewSnapshot,
    max_patch_bytes: usize,
    style: ReviewPromptStyle,
) -> String
```

Keep every snapshot field, cue, UTF-8 truncation rule, four-recommendation limit, and 160-character message instruction unchanged.

- [ ] **Step 4: Create `output.rs` by moving normalization verbatim**

Move and backend-neutralize:

```text
LlmReviewResponse
LlmReviewRecommendation
LOCAL_MAX_RECOMMENDATIONS -> MAX_REVIEW_RECOMMENDATIONS
parse_local_inference_output -> parse_review_output
first_non_empty_review_response
preview_inference_output
extract_json_objects
contains_parseable_review_response -> is_retryable_review_output
llm_response_to_review_insights
normalize_optional_text
parse_recommendation_category
```

Expose:

```rust
pub(crate) fn parse_review_output(
    raw: &str,
) -> Result<ReviewInsights, ReviewInferenceError>;

pub(crate) fn first_non_empty_review_response(raw: &str) -> Option<ReviewInsights>;

pub(crate) fn is_parseable_review_response(raw: &str) -> bool;
```

Change error copy from “local review model” to “review model” so both providers receive truthful sanitized messages.

- [ ] **Step 5: Move llama.cpp-only code to `local.rs`**

Move the existing local engine and all llama-specific helpers without changing algorithms:

```text
RenderedPrompt
LOCAL_BATCH_CAPACITY
LOCAL_MAX_GENERATION_TOKENS
LOCAL_QWEN_SAMPLER_SEED
LocalLlamaReviewInferenceEngine and its impls
should_retry_local_inference
run_local_inference
render_prompt_for_model
is_qwen_family_model
render_prompt_with_model_template
render_prompt_with_oaicompat_template
render_prompt_with_basic_template
manual_qwen_prompt
trim_stop_sequence_suffix
build_local_sampler
qwen_prefers_stochastic_decoder
build_local_model_params
configure_cpu_model_params
select_cpu_backend_device_index
decode_prompt_tokens
ensure_within_deadline
token_to_piece_lossy
batch_add_error
token_error
recommended_thread_count
requested_context_size
```

Replace local calls with:

```rust
let primary_prompt =
    build_review_prompt(snapshot, self.max_patch_bytes, ReviewPromptStyle::Primary);
let retry_prompt =
    build_review_prompt(snapshot, self.max_patch_bytes, ReviewPromptStyle::StrictRetry);
```

and:

```rust
parse_review_output(&rendered_output)
```

- [ ] **Step 6: Reduce `mod.rs` to the provider-neutral contract**

Keep these items in `core/inference/mod.rs`:

```text
ReviewInferenceStatus
ReviewInsights
ReviewRecommendation
RecommendationCategory
ReviewInferenceOutcome and constructors
ReviewInferenceError
ReviewInferenceEngine
NoopReviewInferenceEngine
analyze_with_timeout
```

Add module wiring:

```rust
mod output;
mod prompt;

#[cfg(feature = "llm-local")]
mod local;

#[cfg(feature = "llm-local")]
pub use local::LocalLlamaReviewInferenceEngine;
```

Keep `prompt` and `output` compiled in normal test builds so their unit tests run without a GGUF model.

- [ ] **Step 7: Update the smoke-test import and verify no local behavior regressed**

Keep this public import working in `tests/llm_local_smoke.rs`:

```rust
use mr_milchick::core::inference::{
    LocalLlamaReviewInferenceEngine, RecommendationCategory, ReviewInferenceOutcome,
};
```

Run:

```bash
rtk cargo fmt
rtk cargo fmt --check
rtk cargo test core::inference --locked
rtk cargo test --test llm_local_smoke --no-default-features --features "gitlab llm-local" --locked
```

Expected: formatting passes; inference unit tests pass; the smoke-test binary compiles and its live tests remain ignored.

- [ ] **Step 8: Commit the provider-neutral refactor**

```bash
rtk git add apps/mr-milchick/src/core/inference tests/llm_local_smoke.rs
rtk git commit -m "refactor: split inference providers from shared review logic"
```

---

### Task 3: Build the Hardened Claude Code Process Runner

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `apps/mr-milchick/src/core/inference/claude_code.rs`
- Modify: `apps/mr-milchick/src/core/inference/mod.rs`

**Interfaces:**
- Consumes: Tokio process/I/O, an executable path, OAuth token, and an explicit environment allowlist.
- Produces: `ClaudeCodeCommand::check_version(deadline)` and `ClaudeCodeCommand::run_review(model, schema, prompt, deadline)`.

- [ ] **Step 1: Add failing fake-executable tests for version and argument construction**

In `claude_code.rs`, add Unix-only test helpers that write an executable shell fixture to a uniquely created temporary directory. The fixture must:

```sh
#!/bin/sh
if [ "${1:-}" = "--version" ]; then
  printf '%s\n' "${FAKE_CLAUDE_VERSION:-2.1.218} (Claude Code)"
  exit 0
fi
printf '%s\n' "$@" > "$FAKE_ARGS_FILE"
cat > "$FAKE_STDIN_FILE"
printf '%s\n' '{"is_error":false,"result":"","structured_output":{"summary":null,"recommendations":[]}}'
```

Define the fixture interface once so Tasks 3 and 4 share names:

```rust
struct FakeClaude {
    root: PathBuf,
    executable: PathBuf,
    args_file: PathBuf,
    stdin_files: Vec<PathBuf>,
    counter_file: PathBuf,
}

impl FakeClaude {
    fn new(version: &str) -> Self;
    fn command(&self, oauth_token: Option<&str>) -> ClaudeCodeCommand;
}
```

`FakeClaude::command` populates the test command's explicit forwarded environment with only `PATH`, `FAKE_CLAUDE_VERSION`, `FAKE_ARGS_FILE`, `FAKE_STDIN_FILE`, and `FAKE_COUNTER_FILE`. It must not mutate the test process environment.

Add tests asserting:

```rust
#[tokio::test]
async fn accepts_minimum_claude_code_version() {
    let fixture = FakeClaude::new("2.1.218");
    let command = fixture.command(Some("oauth-token"));
    command
        .check_version(Instant::now() + Duration::from_secs(2))
        .await
        .expect("minimum version should be accepted");
}

#[tokio::test]
async fn rejects_older_claude_code_version() {
    let fixture = FakeClaude::new("2.1.217");
    let error = fixture
        .command(Some("oauth-token"))
        .check_version(Instant::now() + Duration::from_secs(2))
        .await
        .expect_err("old version should fail");
    assert!(error.to_string().contains("requires Claude Code 2.1.218 or newer"));
}
```

- [ ] **Step 2: Enable Tokio process/I/O and the Cargo feature**

Change `Cargo.toml`:

```toml
tokio = { version = "1", features = ["io-util", "macros", "process", "rt-multi-thread", "time"] }

[features]
llm-claude-code = []
```

Run:

```bash
rtk cargo check --no-default-features --features "gitlab llm-claude-code"
rtk cargo check --no-default-features --features "gitlab llm-claude-code" --locked
```

Expected: the first command updates `Cargo.lock` if Tokio's newly enabled transitive components require it; the second proves the resulting lockfile is complete and reproducible.

- [ ] **Step 3: Implement version parsing without adding a runtime dependency**

Add:

```rust
const MIN_CLAUDE_CODE_VERSION: [u64; 3] = [2, 1, 218];
const MAX_CAPTURE_BYTES: usize = 1024 * 1024;

fn parse_claude_code_version(raw: &str) -> Result<[u64; 3], ReviewInferenceError> {
    let token = raw
        .split_whitespace()
        .next()
        .ok_or_else(|| ReviewInferenceError::Configuration(
            "Claude Code returned an empty version".to_string(),
        ))?;
    let parts = token
        .split('.')
        .map(|part| part.parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ReviewInferenceError::Configuration(format!(
            "could not parse Claude Code version from '{}'",
            sanitize_process_text(raw, None)
        )))?;
    match parts.as_slice() {
        [major, minor, patch] => Ok([*major, *minor, *patch]),
        _ => Err(ReviewInferenceError::Configuration(format!(
            "could not parse Claude Code version from '{}'",
            sanitize_process_text(raw, None)
        ))),
    }
}
```

Compare the array lexicographically to `MIN_CLAUDE_CODE_VERSION`.

- [ ] **Step 4: Implement a minimal-environment command builder**

Define:

```rust
struct ClaudeCodeCommand {
    executable: OsString,
    oauth_token: Option<String>,
    forwarded_env: BTreeMap<OsString, OsString>,
}

impl ClaudeCodeCommand {
    fn from_current_process(oauth_token: Option<String>) -> Self;

    async fn check_version(
        &self,
        deadline: Instant,
    ) -> Result<(), ReviewInferenceError>;

    async fn run_review(
        &self,
        model: &str,
        schema: &str,
        prompt: &str,
        deadline: Instant,
    ) -> Result<ClaudeProcessOutput, ReviewInferenceError>;

    #[cfg(test)]
    fn for_test(
        executable: impl Into<OsString>,
        oauth_token: Option<String>,
        forwarded_env: BTreeMap<OsString, OsString>,
    ) -> Self;

    fn base_command(&self, isolated_home: &Path) -> tokio::process::Command;
}
```

Define the bounded result consumed by Task 4:

```rust
struct ClaudeProcessOutput {
    status: std::process::ExitStatus,
    stdout: CapturedStream,
    stderr: CapturedStream,
}
```

`from_current_process` copies only:

```rust
const FORWARDED_ENV_NAMES: &[&str] = &[
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
];
```

`base_command` must call:

```rust
command.env_clear();
command.envs(&self.forwarded_env);
command.env("HOME", isolated_home);
command.env("CLAUDE_CONFIG_DIR", isolated_home.join(".claude"));
if let Some(token) = &self.oauth_token {
    command.env("CLAUDE_CODE_OAUTH_TOKEN", token);
}
command.kill_on_drop(true);
```

Do not derive `Debug` for any type containing the token.

- [ ] **Step 5: Implement safe temporary working-directory ownership**

Create an internal guard using `std::env::temp_dir()`, process ID, an atomic counter, and `create_dir` so each attempt receives an empty unique directory:

```rust
struct IsolatedDirectory {
    path: PathBuf,
}

impl IsolatedDirectory {
    fn create() -> Result<Self, ReviewInferenceError>;
    fn path(&self) -> &Path;
}

impl Drop for IsolatedDirectory {
    fn drop(&mut self) {
        if self.path.starts_with(std::env::temp_dir())
            && self.path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with("mr-milchick-claude-"))
        {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
```

Set both `current_dir` and isolated home/config paths beneath this guard.

- [ ] **Step 6: Implement bounded concurrent stdout/stderr collection**

Add:

```rust
struct CapturedStream {
    bytes: Vec<u8>,
    truncated: bool,
}

async fn capture_bounded<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> std::io::Result<CapturedStream>
```

Read until EOF in fixed-size chunks, append only up to `limit`, and continue draining after the limit so the child cannot block on a full pipe. Spawn stdout and stderr capture concurrently, write the prompt to piped stdin, close stdin, wait for the child, and join both capture tasks.

The review command arguments must be constructed exactly as:

```rust
command.args([
    "--print",
    "--model",
    model,
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
    schema,
]);
```

- [ ] **Step 7: Add environment-isolation and output-cap tests**

Make the fake executable serialize selected environment names and emit more than `MAX_CAPTURE_BYTES`. Assert:

```rust
assert_eq!(observed.get("CLAUDE_CODE_OAUTH_TOKEN"), Some(&"oauth-token".to_string()));
assert!(!observed.contains_key("GITLAB_TOKEN"));
assert!(!observed.contains_key("GITHUB_TOKEN"));
assert!(!observed.contains_key("MR_MILCHICK_SLACK_BOT_TOKEN"));
assert_ne!(observed.get("HOME"), original_home.as_ref());
assert!(output.stdout.truncated);
assert_eq!(output.stdout.bytes.len(), MAX_CAPTURE_BYTES);
```

Also assert the prompt appears only in the captured stdin fixture and not in the argument fixture.

- [ ] **Step 8: Run process-runner tests**

Run:

```bash
rtk cargo test claude_code::tests --no-default-features --features "gitlab llm-claude-code" --locked
```

Expected: version, argument, stdin, environment, isolation-directory, and capture-limit tests pass without network access.

- [ ] **Step 9: Commit the hardened runner**

```bash
rtk git add Cargo.toml Cargo.lock apps/mr-milchick/src/core/inference
rtk git commit -m "feat: add isolated Claude Code process runner"
```

---

### Task 4: Implement Claude Review Analysis, Retry, and Timeout Semantics

**Files:**
- Modify: `apps/mr-milchick/src/core/inference/claude_code.rs`
- Modify: `apps/mr-milchick/src/core/inference/mod.rs`

**Interfaces:**
- Consumes: `ClaudeCodeCommand`, shared prompt/schema/output functions, `ReviewInferenceEngine`.
- Produces: `ClaudeCodeReviewInferenceEngine::new(model, token, max_patch_bytes, timeout)` and `ReviewInferenceOutcome` parity with local inference.

- [ ] **Step 1: Add failing tests for structured output, retry boundaries, and status mapping**

Configure the fake executable with a counter file so sequential calls return controlled responses. Add:

```rust
impl FakeClaude {
    fn engine_with_responses(
        &self,
        responses: &[&str],
    ) -> ClaudeCodeReviewInferenceEngine;

    fn engine_with_exit(
        &self,
        status: i32,
        stderr: &str,
    ) -> ClaudeCodeReviewInferenceEngine;

    fn engine_without_token(&self) -> ClaudeCodeReviewInferenceEngine;

    fn review_call_count(&self) -> usize;
    fn second_stdin(&self) -> String;
}
```

```rust
#[tokio::test]
async fn returns_shared_insights_from_structured_output() {
    let engine = fixture.engine_with_responses(&[
        r#"{"is_error":false,"result":"","structured_output":{"summary":"Review auth.","recommendations":[{"category":"Risk","message":"Check token scope."}]}}"#,
    ]);

    let outcome = engine.analyze(&sample_snapshot()).await.expect("analysis should run");

    assert_eq!(outcome.status, ReviewInferenceStatus::Ready);
    assert_eq!(outcome.insights.summary.as_deref(), Some("Review auth."));
    assert_eq!(fixture.review_call_count(), 1);
}

#[tokio::test]
async fn retries_malformed_output_once_with_strict_prompt() {
    let engine = fixture.engine_with_responses(&[
        r#"{"is_error":false,"result":"not-json"}"#,
        r#"{"is_error":false,"result":"","structured_output":{"summary":null,"recommendations":[]}}"#,
    ]);

    let outcome = engine.analyze(&sample_snapshot()).await.expect("analysis should run");

    assert_eq!(outcome.status, ReviewInferenceStatus::Ready);
    assert_eq!(fixture.review_call_count(), 2);
    assert!(fixture.second_stdin().contains("The first character of your reply must be {"));
}

#[tokio::test]
async fn retries_a_valid_empty_first_review_once() {
    let engine = fixture.engine_with_responses(&[
        r#"{"is_error":false,"result":"","structured_output":{"summary":null,"recommendations":[]}}"#,
        r#"{"is_error":false,"result":"","structured_output":{"summary":"Review auth.","recommendations":[]}}"#,
    ]);

    let outcome = engine.analyze(&sample_snapshot()).await.expect("analysis should run");

    assert_eq!(outcome.status, ReviewInferenceStatus::Ready);
    assert_eq!(outcome.insights.summary.as_deref(), Some("Review auth."));
    assert_eq!(fixture.review_call_count(), 2);
}

#[tokio::test]
async fn does_not_retry_process_failures() {
    let engine = fixture.engine_with_exit(1, "authentication failed");
    let error = engine.analyze(&sample_snapshot()).await.expect_err("analysis should fail");
    assert!(error.to_string().contains("Claude Code exited with status 1"));
    assert_eq!(fixture.review_call_count(), 1);
}

#[tokio::test]
async fn missing_oauth_token_is_advisory_unavailable() {
    let engine = fixture.engine_without_token();
    let outcome = engine.analyze(&sample_snapshot()).await.expect("analysis should resolve");
    assert_eq!(outcome.status, ReviewInferenceStatus::Unavailable);
    assert!(
        outcome
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("CLAUDE_CODE_OAUTH_TOKEN"))
    );
    assert_eq!(fixture.review_call_count(), 0);
}
```

- [ ] **Step 2: Add the Claude CLI envelope and sanitized extraction**

Implement:

```rust
#[derive(Debug, Deserialize)]
struct ClaudeCodeEnvelope {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    result: String,
    #[serde(default)]
    structured_output: Option<serde_json::Value>,
}

fn extract_claude_review_output(raw: &str) -> Result<String, ReviewInferenceError> {
    let envelope: ClaudeCodeEnvelope = serde_json::from_str(raw).map_err(|error| {
        ReviewInferenceError::Analysis(format!(
            "Claude Code returned an invalid JSON envelope: {error}"
        ))
    })?;
    if envelope.is_error {
        return Err(ReviewInferenceError::Analysis(format!(
            "Claude Code reported an error: {}",
            sanitize_process_text(&envelope.result, None)
        )));
    }
    match envelope.structured_output {
        Some(value) => serde_json::to_string(&value).map_err(|error| {
            ReviewInferenceError::Analysis(format!(
                "failed to serialize Claude Code structured output: {error}"
            ))
        }),
        None => Ok(envelope.result),
    }
}
```

Implement `sanitize_process_text(raw: &str, oauth_token: Option<&str>)`. Limit text to a small fixed preview, replace control characters, and replace every exact non-empty OAuth-token occurrence with `[REDACTED]`. Pass the engine token for process stderr/stdout errors; envelope text receives `None` because the model response is already isolated from process secrets.

- [ ] **Step 3: Implement `ClaudeCodeReviewInferenceEngine`**

Add:

```rust
pub struct ClaudeCodeReviewInferenceEngine {
    command: ClaudeCodeCommand,
    model: String,
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
            command: ClaudeCodeCommand::from_current_process(oauth_token),
            model: model.into(),
            max_patch_bytes,
            timeout,
        }
    }

    #[cfg(test)]
    fn with_command(
        model: impl Into<String>,
        max_patch_bytes: usize,
        timeout: Duration,
        command: ClaudeCodeCommand,
    ) -> Self;
}
```

The test constructor must be the only path that accepts an arbitrary executable.

- [ ] **Step 4: Implement readiness and two-attempt analysis under one deadline**

The `ReviewInferenceEngine` implementation must:

```rust
let Some(_) = self.command.oauth_token.as_deref().filter(|token| !token.trim().is_empty()) else {
    return Ok(ReviewInferenceOutcome::unavailable(
        "Claude Code inference requires CLAUDE_CODE_OAUTH_TOKEN",
    ));
};

let deadline = Instant::now() + self.timeout;
match self.command.check_version(deadline).await {
    Ok(()) => {}
    Err(ReviewInferenceError::TimedOut(timeout)) => {
        return Err(ReviewInferenceError::TimedOut(timeout));
    }
    Err(error) => {
        return Ok(ReviewInferenceOutcome::unavailable(error.to_string()));
    }
}

let prompts = [
    build_review_prompt(snapshot, self.max_patch_bytes, ReviewPromptStyle::Primary),
    build_review_prompt(snapshot, self.max_patch_bytes, ReviewPromptStyle::StrictRetry),
];
```

For each prompt:

1. Run Claude under the remaining deadline.
2. Reject non-zero exit status as `ReviewInferenceError::Analysis` with bounded sanitized stderr.
3. Decode the Claude envelope.
4. Pass the extracted JSON to `parse_review_output`.
5. Return `Ready` immediately for a valid non-empty result.
6. Retry once when the first result is valid but contains no summary/recommendations, or when its review payload is malformed.
7. Return `Ready` after a valid second result, including a valid empty review.

If the second payload is invalid, return `ReviewInferenceError::Analysis`. Treat truncated stdout as an analysis failure because a complete JSON envelope cannot be proven; include only the bounded sanitized preview. If no time remains or the process deadline expires, return `ReviewInferenceError::TimedOut(self.timeout)`. Return `true` from `handles_internal_timeout()`.

- [ ] **Step 5: Add timeout/kill and token-redaction tests**

Use a fake executable that writes its PID, sleeps, and would create a sentinel after the sleep. Set a 50 ms timeout and assert:

```rust
assert!(matches!(error, ReviewInferenceError::TimedOut(_)));
tokio::time::sleep(Duration::from_millis(100)).await;
assert!(!sentinel.exists(), "timed-out child must be terminated");
```

Use a fake authentication error containing the literal token and assert:

```rust
let rendered = error.to_string();
assert!(!rendered.contains("oauth-secret"));
assert!(rendered.contains("[REDACTED]"));
```

- [ ] **Step 6: Export the engine behind its independent feature**

In `core/inference/mod.rs`:

```rust
#[cfg(feature = "llm-claude-code")]
mod claude_code;

#[cfg(feature = "llm-claude-code")]
pub use claude_code::ClaudeCodeReviewInferenceEngine;
```

- [ ] **Step 7: Run all Claude engine tests**

Run:

```bash
rtk cargo test claude_code::tests --no-default-features --features "gitlab llm-claude-code" --locked
```

Expected: all tests pass, no network call is made, and no real token is required.

- [ ] **Step 8: Commit remote analysis behavior**

```bash
rtk git add apps/mr-milchick/src/core/inference
rtk git commit -m "feat: analyze review snapshots with Claude Code"
```

---

### Task 5: Wire Runtime Selection, Capabilities, and Release Features

**Files:**
- Modify: `apps/mr-milchick/src/app.rs`
- Modify: `apps/mr-milchick/src/config/resolver.rs`
- Modify: `apps/mr-milchick/src/config/mod.rs`
- Modify: `tests/cli_integration.rs`
- Modify: `.gitlab-ci.yml`

**Interfaces:**
- Consumes: `ResolvedInferenceBackend`, both provider constructors, independent Cargo features.
- Produces: exactly one runtime connector, backend-specific capability output, and a release artifact with both backends.

- [ ] **Step 1: Replace app wiring tests with backend-selection cases**

In `app.rs`, replace the flat-model-path test with:

```rust
#[test]
fn disabled_inference_builds_an_unavailable_connector() {
    let config = sample_resolved_config();
    let (_, available) =
        build_inference_connector(&config).expect("disabled inference should build");
    assert!(!available);
}

#[cfg(feature = "llm-local")]
#[test]
fn local_backend_selects_the_local_engine() {
    let model = std::env::temp_dir().join(format!(
        "mr-milchick-model-path-test-{}.gguf",
        std::process::id()
    ));
    std::fs::write(&model, []).expect("model-path fixture should exist");
    let mut config = sample_resolved_config();
    config.inference.backend = ResolvedInferenceBackend::Local {
        model_path: model.display().to_string(),
        context_tokens: 4_096,
    };
    let (_, available) = build_inference_connector(&config).expect("connector should build");
    std::fs::remove_file(&model).expect("model-path fixture should be removed");
    assert!(available);
}

#[cfg(feature = "llm-claude-code")]
#[test]
fn claude_backend_is_available_when_compiled() {
    let mut config = sample_resolved_config();
    config.inference.backend = ResolvedInferenceBackend::ClaudeCode {
        model: "claude-versioned-model".to_string(),
        oauth_token: Some("oauth-token".to_string()),
    };
    let (_, available) = build_inference_connector(&config).expect("connector should build");
    assert!(available);
}
```

The local constructor currently validates only that the path is a file; the test must not call `analyze`, so the empty path fixture does not load llama.cpp or pretend to be a valid GGUF model.

- [ ] **Step 2: Make compiled capabilities backend-specific**

In `config/resolver.rs`, replace `llm_backend_compiled()` with:

```rust
pub fn compiled_inference_backends() -> Vec<InferenceBackendKind> {
    let backends = Vec::new();
    #[cfg(feature = "llm-local")]
    let backends = {
        let mut backends = backends;
        backends.push(InferenceBackendKind::Local);
        backends
    };
    #[cfg(feature = "llm-claude-code")]
    let backends = {
        let mut backends = backends;
        backends.push(InferenceBackendKind::ClaudeCode);
        backends
    };
    backends
}
```

Export it from `config/mod.rs`.

- [ ] **Step 3: Match resolved backends in `build_inference_connector`**

Import providers under independent feature gates and match:

```rust
match &config.inference.backend {
    ResolvedInferenceBackend::Disabled { .. } => disabled_connector(timeout),
    ResolvedInferenceBackend::Local {
        model_path,
        context_tokens,
    } => build_local_connector(
        model_path,
        *context_tokens,
        config.inference.max_patch_bytes,
        timeout,
    ),
    ResolvedInferenceBackend::ClaudeCode { model, oauth_token } => {
        build_claude_code_connector(
            model,
            oauth_token.clone(),
            config.inference.max_patch_bytes,
            timeout,
        )
    }
}
```

Each helper has a compiled and uncompiled `cfg` implementation. An uncompiled selected backend returns `NoopReviewInferenceEngine::unavailable` and `available = false`; it never selects the other provider.

- [ ] **Step 4: Update logs and version capability output**

Replace “local review” in provider-neutral lifecycle logs with “advisory AI review.” Print:

```text
- advisory inference: local llama.cpp
- advisory inference: Claude Code CLI
```

for each compiled provider, or:

```text
- advisory inference: not compiled
```

when neither exists. Do not print the model token or OAuth token.

- [ ] **Step 5: Prove the token is a supported secret at the CLI boundary**

Add a CLI integration test that supplies `CLAUDE_CODE_OAUTH_TOKEN` while running a command that does not wire inference:

```rust
#[test]
fn claude_code_oauth_token_is_not_rejected_as_legacy_config() {
    let output = run_cli(
        "version",
        &[],
        &[("CLAUDE_CODE_OAUTH_TOKEN", "oauth-secret")],
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!combined.contains("oauth-secret"));
}
```

- [ ] **Step 6: Compile both inference features into release artifacts**

Change `.gitlab-ci.yml`:

```yaml
- cargo build -p mr-milchick --release --locked --no-default-features --features "gitlab slack-app llm-local llm-claude-code"
```

- [ ] **Step 7: Run wiring and feature-matrix checks**

Run:

```bash
rtk cargo test app::tests::build_inference --locked
rtk cargo test --test cli_integration claude_code_oauth_token --locked
rtk cargo check --no-default-features --features "gitlab llm-claude-code" --locked
rtk cargo check --no-default-features --features "gitlab llm-local" --locked
rtk cargo check --no-default-features --features "gitlab llm-local llm-claude-code" --locked
```

Expected: tests and all three feature shapes pass.

- [ ] **Step 8: Commit runtime selection**

```bash
rtk git add apps/mr-milchick/src/app.rs apps/mr-milchick/src/config tests/cli_integration.rs .gitlab-ci.yml
rtk git commit -m "feat: select compiled inference backend at runtime"
```

---

### Task 6: Make Advisory Rendering Backend-Neutral

**Files:**
- Modify: `apps/mr-milchick/src/core/message_templates.rs`

**Interfaces:**
- Consumes: existing `InferenceTemplateContext`.
- Produces: provider-neutral `AI review recommendations` copy for GitLab, GitHub, and Slack styles.

- [ ] **Step 1: Change the rendering test first**

In `renders_default_gitlab_explain_template`, change:

```rust
assert!(rendered.contains("AI review recommendations"));
assert!(!rendered.contains("Local review recommendations"));
```

Add the same assertion to a Slack-style recommendations test so both heading branches are covered.

- [ ] **Step 2: Run the focused test and confirm it fails**

Run:

```bash
rtk cargo test renders_default_gitlab_explain_template --locked
```

Expected: failure because rendered output still contains `Local review recommendations`.

- [ ] **Step 3: Replace both local-only headings**

In `format_recommendations_block`, use:

```rust
match style {
    RenderStyle::GitLab | RenderStyle::GitHub => {
        lines.push("### AI review recommendations".to_string())
    }
    RenderStyle::SlackApp => lines.push("*AI review recommendations*".to_string()),
}
```

- [ ] **Step 4: Run message-template tests**

Run:

```bash
rtk cargo test core::message_templates --locked
```

Expected: all rendering tests pass.

- [ ] **Step 5: Commit the neutral copy**

```bash
rtk git add apps/mr-milchick/src/core/message_templates.rs
rtk git commit -m "fix: make inference recommendation copy backend neutral"
```

---

### Task 7: Document TOML, Security Boundary, and Pinned GitLab Demo

**Files:**
- Create: `examples/gitlab/claude-code-explain.gitlab-ci.yml`
- Create: `docs/claude-code-llm.md`
- Modify: `mr-milchick.toml`
- Modify: `docs/config-reference.md`
- Modify: `docs/ci-quickstart.md`
- Modify: `docs/connectors-and-capabilities.md`
- Modify: `docs/architecture.md`
- Modify: `docs/local-llm.md`
- Modify: `docs/README.md`

**Interfaces:**
- Consumes: the final TOML schema, feature names, CLI minimum version, and trust model.
- Produces: copyable root-config setup and a pinned same-project GitLab pipeline.

- [ ] **Step 1: Replace the root TOML inference comments with mutually exclusive examples**

Keep one `[inference]` table and document:

```toml
[inference]
# Inference runs only during `explain`.
# Omit `backend` for backward-compatible local inference.
# backend = "local"
# enabled = false

# Local-only fields; require the `llm-local` Cargo feature:
# model_path = "${CI_PROJECT_DIR}/models/review.gguf"
# context_tokens = 4096

# Claude Code-only fields; require the `llm-claude-code` Cargo feature:
# backend = "claude-code"
# model = "fully-versioned-model-name"
# The credential comes only from CLAUDE_CODE_OAUTH_TOKEN in the pipeline.
# Do not put the token in this file.

# Shared fields:
# timeout_ms = 120000
# max_patch_bytes = 32768
# trace = false
```

State immediately above the examples that `model` is mandatory for enabled Claude Code, aliases are accepted, and full versioned identifiers are recommended.

- [ ] **Step 2: Add the focused Claude Code guide**

Create `docs/claude-code-llm.md` with these exact sections:

```text
Claude Code Advisory Review
Configuration
Pipeline Secret
Installing the CLI
Snapshot-Only Security Boundary
Trusted Merge Requests Only
Failure Semantics
Troubleshooting
Upgrading the Model or CLI
```

Include:

```toml
[inference]
enabled = true
backend = "claude-code"
model = "fully-versioned-model-name"
timeout_ms = 120000
max_patch_bytes = 32768
trace = false
```

Explicitly state:

- `CLAUDE_CODE_OAUTH_TOKEN` is a Claude Code subscription/OAuth token, not an Anthropic API key.
- The job requires Claude Code 2.1.218 or newer.
- Tools are disabled and only the bounded snapshot is sent.
- Missing CLI/token, authentication, rate limits, and timeouts do not block governance.
- Forks and untrusted pipeline authors must never receive the token.
- The model and CLI version should be upgraded deliberately and tested together.

- [ ] **Step 3: Add the standalone GitLab example using the root TOML**

Create `examples/gitlab/claude-code-explain.gitlab-ci.yml` with:

```yaml
stages:
  - build
  - review

variables:
  CLAUDE_CODE_VERSION: "2.1.218"

build:milchick:
  stage: build
  image: rust:1.86
  script:
    - cargo build --release --locked --no-default-features --features "gitlab slack-app llm-claude-code"
    - mkdir -p dist
    - cp target/release/mr-milchick dist/
  artifacts:
    paths:
      - dist/mr-milchick

milchick:refine:
  stage: review
  image: debian:bookworm-slim
  needs: ["build:milchick"]
  script:
    - ./dist/mr-milchick refine
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'

milchick:explain:
  stage: review
  image: node:22-bookworm-slim
  needs: ["build:milchick", "milchick:refine"]
  before_script:
    - npm install --global "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}"
    - claude --version
  script:
    - ./dist/mr-milchick explain
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event" && $CI_MERGE_REQUEST_SOURCE_PROJECT_ID == $CI_PROJECT_ID && $CLAUDE_CODE_OAUTH_TOKEN'
```

Add top-of-file comments requiring the normal root `mr-milchick.toml`, `GITLAB_TOKEN`, a masked `CLAUDE_CODE_OAUTH_TOKEN`, trusted same-project authors, and immutable CI configuration for untrusted repositories.

- [ ] **Step 4: Update reference and architecture documentation**

In `docs/config-reference.md`, replace the inference table with:

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `enabled` | No | `false` | Enables advisory inference for `explain`. |
| `backend` | No | `local` | `local` or `claude-code`. |
| `model_path` | Local | none | Required only for enabled local inference. |
| `model` | Claude Code | none | Required and non-empty only for enabled Claude Code inference. |
| `timeout_ms` | No | backend-specific | `15000` local; `120000` Claude Code. |
| `max_patch_bytes` | No | `32768` | Shared bounded patch budget. |
| `context_tokens` | Local | `4096` | Rejected for Claude Code. |
| `trace` | No | `false` | Prints sanitized inference diagnostics. |

Add `CLAUDE_CODE_OAUTH_TOKEN` to the secret inputs and remove “local” from provider-neutral explain wording.

In `docs/architecture.md`, document the shared prompt/output path and two engine modules. In `docs/connectors-and-capabilities.md`, add `llm-claude-code = []`, build commands for either/both providers, the runtime CLI requirement, and separate capability output.

- [ ] **Step 5: Update quickstart/navigation and local-guide scope**

In `docs/ci-quickstart.md`, retain the local example, add the Claude TOML block, show the pinned npm installation, link `../examples/gitlab/claude-code-explain.gitlab-ci.yml`, and repeat the same-project trust rule.

In `docs/local-llm.md`, change the first paragraph to:

```markdown
This guide covers the local llama.cpp provider only. For subscription-backed remote review through the Claude Code CLI, see [Claude Code advisory review](claude-code-llm.md).
```

In `docs/README.md`, add:

```markdown
- [Claude Code advisory review](claude-code-llm.md): snapshot-only remote review, OAuth token handling, pinned CLI installation, and trusted-pipeline requirements.
```

- [ ] **Step 6: Verify docs and example consistency**

Run:

```bash
rtk proxy rg -n "Local review recommendations|llm_backend_compiled|advisory local review backend" apps mr-milchick.toml docs examples .gitlab-ci.yml
rtk proxy rg -n "CLAUDE_CODE_OAUTH_TOKEN|llm-claude-code|2\\.1\\.218|120000|claude-code" mr-milchick.toml docs examples .gitlab-ci.yml Cargo.toml
rtk git diff --check
```

Expected: the first search returns only intentional local-provider documentation; the second finds consistent config, feature, token, minimum-version, and timeout references; diff check is clean.

- [ ] **Step 7: Commit documentation and CI example**

```bash
rtk git add mr-milchick.toml docs examples .gitlab-ci.yml
rtk git commit -m "docs: add Claude Code inference setup"
```

---

## Final Verification

- [ ] **Step 1: Format and lint all feature shapes**

Run:

```bash
rtk cargo fmt --check
rtk cargo clippy --all-targets --no-default-features --features "gitlab slack-app llm-claude-code" --locked -- -D warnings
rtk cargo clippy --all-targets --no-default-features --features "gitlab slack-app llm-local llm-claude-code" --locked -- -D warnings
```

Expected: all commands pass without warnings.

- [ ] **Step 2: Run the default and provider feature test suites**

Run:

```bash
rtk cargo test --workspace --locked
rtk cargo test --workspace --no-default-features --features "gitlab slack-app llm-claude-code" --locked
rtk cargo test --workspace --no-default-features --features "gitlab slack-app llm-local" --locked
rtk cargo test --workspace --no-default-features --features "gitlab slack-app llm-local llm-claude-code" --locked
```

Expected: every suite passes; live GGUF smoke tests remain ignored; no Claude test contacts the network.

- [ ] **Step 3: Inspect runtime capabilities**

Run:

```bash
rtk cargo run --no-default-features --features "gitlab slack-app llm-local llm-claude-code" -- version
```

Expected output includes:

```text
- advisory inference: local llama.cpp
- advisory inference: Claude Code CLI
```

- [ ] **Step 4: Audit the final diff for secret and scope regressions**

Run:

```bash
rtk proxy rg -n "ANTHROPIC_API_KEY|CLAUDE_CODE_OAUTH_TOKEN" apps tests docs examples mr-milchick.toml
rtk proxy rg -n "allowedTools|--tools|--safe-mode|--no-session-persistence|--disable-slash-commands" apps/mr-milchick/src/core/inference docs examples
rtk git diff --check
rtk git status --short
```

Expected:

- `ANTHROPIC_API_KEY` appears only in documentation saying it is unsupported.
- `CLAUDE_CODE_OAUTH_TOKEN` appears in the secret loader, isolated subprocess setup, tests, and documentation, never in rendered output.
- All security flags appear in implementation and documentation.
- Diff check is clean and status contains only the intended feature files.

- [ ] **Step 5: Review commit history**

Run:

```bash
rtk git log --oneline -8
```

Expected: focused commits exist for configuration, inference refactor, process runner, Claude analysis, runtime wiring, neutral copy, and documentation.
