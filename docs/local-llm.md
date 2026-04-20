# Local LLM Review

Local advisory review is now configured from `mr-milchick.toml`.

## Runtime Config

```toml
[inference]
enabled = true
model_path = "models/review.gguf"
timeout_ms = 15000
max_patch_bytes = 32768
context_tokens = 4096
trace = false
```

Notes:

- `enabled = true` turns on advisory review suggestions for `explain`.
- `model_path` is required when inference is enabled.
- `trace = true` prints the detailed inference result during `explain`.
- `observe` and `refine` never invoke inference.
- If the binary is built without `llm-local`, Milchick reports that inference was configured but not compiled in.

## Secrets And Context

Live advisory runs still need the usual platform token and CI review context because `explain` reloads the review snapshot and Milchick's managed governance summary comment from the review platform.

`explain` only runs advisory analysis when the latest governance summary metadata says the prior `refine` run either applied governance actions or remained blocked. If the metadata is missing, malformed, or reports no governance effect, `explain` skips by design.

## Smoke Tests

The ignored smoke tests in [`tests/llm_local_smoke.rs`](/Users/arthur.kovacs/Work/mr-milchick/tests/llm_local_smoke.rs) still use dedicated smoke-test env inputs because they exercise the local inference engine in isolation rather than the full app config loader.

Smoke-test env vars:

- `MR_MILCHICK_LLM_SMOKE_MODEL_PATH`
- `MR_MILCHICK_LLM_SMOKE_TIMEOUT_MS`
- `MR_MILCHICK_LLM_SMOKE_MAX_PATCH_BYTES`
- `MR_MILCHICK_LLM_SMOKE_CONTEXT_TOKENS`

## Practical Guidance

- Building with `--features llm-local` requires `cmake` because `llama-cpp-sys` compiles vendored `llama.cpp` during the build.
- On macOS hosts, prefer [`scripts/build_linux_release.sh`](/Users/arthur.kovacs/Work/mr-milchick/scripts/build_linux_release.sh) for Linux-musl artifacts so the active Xcode SDK path is forwarded to bindgen automatically.
- If `explain` skips because the last `refine` had no governance effect and no blocking outcome, that is expected behavior rather than an inference failure.
- Raise `max_patch_bytes` when review quality drops because the prompt loses multi-file context.
- Raise `context_tokens` when prompts still overflow after patch budgeting.
- Use `trace = true` temporarily when tuning prompts or verifying that a model is actually contributing useful output.

For the full runtime schema, see [config-reference.md](config-reference.md).
