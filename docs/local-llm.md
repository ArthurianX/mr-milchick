# Local LLM Review

Mr Milchick can run an optional local review pass against a GGUF model and add advisory review suggestions to the normal rule-based output. This path is meant for teams that want local AI review hints without sending diffs to a hosted model provider.

The local model path is advisory only. Reviewer routing, CODEOWNERS, blocking policy, GitLab or GitHub mutations, and Slack delivery still come from the normal Milchick execution flow.

## How It Is Implemented

Local review is compiled behind the `llm-local` feature and runs through `apps/mr-milchick/src/core/inference/mod.rs`.

At runtime, Milchick:

- builds a structured review snapshot from the active merge request or pull request context
- turns the snapshot into a bounded prompt with title, metadata, changed files, and diff excerpts
- loads a local GGUF through `llama.cpp` via the `llama-cpp-2` crate
- prefers the model's built-in `tokenizer.chat_template` when available
- falls back to a manual prompt wrapper if template application fails
- parses the response as strict JSON into `summary` plus categorized recommendations

The current local inference path also includes a few protective behaviors:

- built-in chat templates are preferred over hand-written wrappers
- Qwen-family models are asked to run with `enable_thinking=false` when the template API supports it
- small Qwen-family checkpoints use repeatable non-greedy sampling tuned near Qwen's coding defaults
- larger Qwen-family checkpoints fall back to greedy decoding because they benchmarked better that way in this repository
- invalid JSON or empty structured replies trigger a stricter retry prompt before the result is marked failed

## Build And Runtime Requirements

Local LLM builds need a native toolchain in addition to Rust because `llama-cpp-sys-2` runs `bindgen` and builds `llama.cpp` from source. On Debian-based CI runners that usually means `clang`, `libclang-dev`, `llvm-dev`, `cmake`, `pkg-config`, and any target toolchain packages such as `musl-tools`.

Build the binary with `llm-local` enabled:

```bash
cargo build --features llm-local
```

You can check the compiled surface with:

```bash
cargo run --features llm-local -- version
```

For real execution, you still need the normal platform connector features you use in CI. A typical local build might look like:

```bash
cargo build --release --no-default-features --features "gitlab slack-app llm-local"
```

## Environment Variables

### Runtime Local LLM Variables

These variables affect the optional advisory model pass:

| Variable | Required | Purpose |
| --- | --- | --- |
| `MR_MILCHICK_LLM_ENABLED` | No | Enables or disables local review suggestions explicitly. If omitted, Milchick can infer availability from the compiled feature set and config. |
| `MR_MILCHICK_LLM_MODEL_PATH` | Yes for local review | Absolute or relative path to the GGUF file Milchick should load. |
| `MR_MILCHICK_LLM_TIMEOUT_MS` | No | Timeout for the full advisory inference pass in milliseconds. |
| `MR_MILCHICK_LLM_MAX_PATCH_BYTES` | No | Caps how many diff bytes are included in the local prompt across all changed files. |
| `MR_MILCHICK_LLM_CONTEXT_TOKENS` | No | Caps the llama.cpp context window Milchick requests for local review. Raise this for large MRs when the prompt no longer fits in the default window. |

### Smoke-Test And Benchmark Variables

These variables are used by the local smoke-test workflow:

| Variable | Required | Purpose |
| --- | --- | --- |
| `MR_MILCHICK_LLM_MODEL_PATH` | Yes | Points the smoke test at a local GGUF file. |
| `MR_MILCHICK_LLM_SMOKE_TIMEOUT_MS` | No | Overrides the smoke test timeout. |
| `MR_MILCHICK_LLM_SMOKE_MAX_PATCH_BYTES` | No | Overrides the smoke test patch budget. |
| `MR_MILCHICK_LLM_SMOKE_CONTEXT_TOKENS` | No | Overrides the smoke test context window. |

## Usage

### Run With A Local GGUF

```bash
MR_MILCHICK_LLM_ENABLED=true \
MR_MILCHICK_LLM_MODEL_PATH=test-models/fieldmouse/QWEN-QWEN3.5-2B-Q4_K_M.gguf \
MR_MILCHICK_LLM_TIMEOUT_MS=120000 \
MR_MILCHICK_LLM_MAX_PATCH_BYTES=4096 \
MR_MILCHICK_LLM_CONTEXT_TOKENS=8192 \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
GITLAB_TOKEN=your-gitlab-token \
cargo run --features "gitlab llm-local" -- observe
```

### Typical Settings

These are good starting points:

- `MR_MILCHICK_LLM_TIMEOUT_MS=120000`
- `MR_MILCHICK_LLM_MAX_PATCH_BYTES=4096`
- `MR_MILCHICK_LLM_CONTEXT_TOKENS=8192`
- keep one primary model path in CI and use a second model only for manual fallback or benchmarks

Raise `MR_MILCHICK_LLM_MAX_PATCH_BYTES` when your review cases rely on multi-file context. Raise `MR_MILCHICK_LLM_CONTEXT_TOKENS` when the prompt still overflows after patch budgeting. Lower either knob if you want stricter latency or memory bounds.

## CI Shape

The simplest CI pattern is:

1. Build the Milchick binary with `llm-local`.
2. Fetch or clone a separate model repository into the pipeline workspace.
3. Point `MR_MILCHICK_LLM_MODEL_PATH` at the chosen GGUF.
4. Run `mr-milchick refine` or `observe` normally.

### Recommended Model Repo Pattern

Large GGUF files are often too big for ordinary GitLab job artifacts. In practice, a separate model repository cloned during CI is usually safer than trying to pass GGUF files through cross-project artifacts.

A good pattern is:

- keep the winning GGUF files in a separate private repo
- use Git LFS if your GitLab instance supports it
- pin the model repo to a tag such as `models-v1`
- publish a `SHA256SUMS` file alongside the models
- verify the model hash before running Milchick

Example consumer job shape:

```yaml
run_mr_milchick:
  stage: review
  image: alpine:3.23.3
  variables:
    MR_MILCHICK_LLM_ENABLED: "true"
    MR_MILCHICK_LLM_MODEL_PATH: "$CI_PROJECT_DIR/models/QWEN-QWEN3.5-2B-Q4_K_M.gguf"
  before_script:
    - apk add --no-cache git git-lfs
    - git lfs install
    - git clone --depth 1 --branch models-v1 https://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.example.com/group/mr-milchick-models.git models
    - sha256sum -c models/SHA256SUMS
  script:
    - ./dist/mr-milchick refine
```

## Smoke Testing

The repository includes an ignored integration smoke suite in `tests/llm_local_smoke.rs`.

Compile-only check:

```bash
cargo test --features llm-local --test llm_local_smoke --no-run
```

Run the backend load probe:

```bash
MR_MILCHICK_LLM_MODEL_PATH=test-models/fieldmouse/QWEN-QWEN3.5-2B-Q4_K_M.gguf \
cargo test --features llm-local --test llm_local_smoke backend_load_and_context_probe -- --ignored --nocapture
```

Run the full ignored smoke suite against one model:

```bash
MR_MILCHICK_LLM_MODEL_PATH=test-models/fieldmouse/QWEN-QWEN3.5-2B-Q4_K_M.gguf \
MR_MILCHICK_LLM_SMOKE_TIMEOUT_MS=120000 \
MR_MILCHICK_LLM_SMOKE_MAX_PATCH_BYTES=4096 \
cargo test --features llm-local --test llm_local_smoke -- --ignored --test-threads=1 --nocapture
```

The current smoke suite checks:

- GGUF load and context creation
- a harder JavaScript backend review case with removed auth, removed validation, and deleted tests
- a harder TypeScript frontend review case with raw HTML rendering, removed sanitization, and deleted tests

## Benchmarking

Use the repeatable benchmark script when you want to compare multiple GGUF files under `test-models`.

```bash
./scripts/benchmark_llm_smoke.sh
```

Useful variants:

```bash
./scripts/benchmark_llm_smoke.sh --models-dir test-models --output-dir /tmp/milchick-llm-bench
./scripts/benchmark_llm_smoke.sh --smoke-timeout-ms 240000 --patch-budget 4096
```

The benchmark runs each ignored smoke case separately with `--test-threads=1` and writes:

- `summary.md` with a ranked scoreboard
- `summary.csv` for spreadsheet or plotting work
- per-model logs under `logs/`

The current benchmark scoring looks at:

- load success
- reaction smoke pass rate
- total wall-clock runtime
- quality penalties such as prompt parroting, empty output, too-few recommendations, duplicate recommendations, generic wording, structured-but-failed cases, and protocol failures

## Current Tuning Notes

The current implementation reflects repository-local benchmark results rather than a universal rule for all GGUFs:

- built-in chat templates are preferred by default
- smaller Qwen-family models use repeatable stochastic decoding
- larger Qwen-family models use greedy decoding

If your model mix changes, rerun the benchmark rather than assuming the same decoding policy will stay optimal.
