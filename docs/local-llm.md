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

- `enabled = true` turns on advisory review suggestions during normal CLI runs.
- `model_path` is required when inference is enabled.
- `trace = true` prints the detailed inference result in `observe`, `explain`, and `refine`.
- If the binary is built without `llm-local`, Milchick reports that inference was configured but not compiled in.

## Secrets And Context

Inference still runs inside the normal application flow, so live review runs also need the usual platform token and CI context.

## Smoke Tests

The ignored smoke tests in [`tests/llm_local_smoke.rs`](/Users/arthurianxxx/Documents/mr-milchick/tests/llm_local_smoke.rs) still use dedicated smoke-test env inputs because they exercise the local inference engine in isolation rather than the full app config loader.

Smoke-test env vars:

- `MR_MILCHICK_LLM_SMOKE_MODEL_PATH`
- `MR_MILCHICK_LLM_SMOKE_TIMEOUT_MS`
- `MR_MILCHICK_LLM_SMOKE_MAX_PATCH_BYTES`
- `MR_MILCHICK_LLM_SMOKE_CONTEXT_TOKENS`

## Practical Guidance

- Raise `max_patch_bytes` when review quality drops because the prompt loses multi-file context.
- Raise `context_tokens` when prompts still overflow after patch budgeting.
- Use `trace = true` temporarily when tuning prompts or verifying that a model is actually contributing useful output.

For the full runtime schema, see [config-reference.md](config-reference.md).
