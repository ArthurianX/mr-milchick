# Claude Code Remote Inference Design

## Summary

Mr. Milchick will support Claude Code as an optional remote advisory-review backend alongside the existing local llama.cpp backend. Operators select the backend explicitly in `mr-milchick.toml`. Claude Code authenticates through the pipeline secret `CLAUDE_CODE_OAUTH_TOKEN`, runs only during `explain`, receives the same bounded review snapshot as local inference, and cannot use tools or inspect the checked-out repository.

The published binary will compile both `llm-local` and `llm-claude-code`. Existing local configurations remain compatible: when `backend` is omitted, it resolves to `local`.

## Goals

- Let an operator choose local llama.cpp or remote Claude Code inference in TOML.
- Preserve the existing advisory output contract and `explain`-only execution gate.
- Reuse the same snapshot prompt, output schema, recommendation limits, retry semantics, and rendering path for both backends.
- Authenticate Claude Code with `CLAUDE_CODE_OAUTH_TOKEN` without treating it as an Anthropic API key.
- Bound remote access, runtime, output size, and secret exposure.
- Provide a runnable GitLab CI example that installs a pinned Claude Code version.

## Non-goals

- Calling the Anthropic Messages API directly.
- Supporting `ANTHROPIC_API_KEY` in this feature.
- Allowing Claude Code tools, repository exploration, test execution, hooks, plugins, skills, MCP servers, or persistent sessions.
- Running inference from `observe` or `refine`.
- Automatically falling back between local and remote inference.
- Creating a generic provider/plugin framework.
- Guaranteeing identical natural-language output across different models. Parity means identical input scope, response contract, advisory semantics, and downstream behavior.

## Configuration

The schema gains an explicit backend and a Claude Code model:

```toml
[inference]
enabled = true
backend = "claude-code"
model = "fully-versioned-model-name"
timeout_ms = 120000
max_patch_bytes = 32768
trace = false
```

`backend` accepts `local` and `claude-code`. If omitted, it resolves to `local` for backward compatibility.

Shared fields:

- `enabled`
- `backend`
- `timeout_ms`
- `max_patch_bytes`
- `trace`

Local-only fields:

- `model_path`
- `context_tokens`

Claude Code-only fields:

- `model`

When enabled, `local` requires a non-empty `model_path` and rejects `model`. When enabled, `claude-code` requires a non-empty `model` and rejects `model_path` and explicitly configured `context_tokens`. Any non-empty model value supported by Claude Code is accepted. Documentation strongly recommends a full versioned model identifier rather than an alias.

The default timeout is backend-specific: 15 seconds for `local` and 120 seconds for `claude-code`. An explicit `timeout_ms` overrides either default.

`CLAUDE_CODE_OAUTH_TOKEN` is a secret environment input. It is loaded through the existing secret boundary, never interpolated into TOML, never logged, and never renamed to `ANTHROPIC_API_KEY`.

## Architecture

The existing `ReviewInferenceEngine` trait remains the provider boundary:

```text
ReviewSnapshot
  -> shared bounded prompt builder
  -> LocalLlamaReviewInferenceEngine
       or ClaudeCodeReviewInferenceEngine
  -> shared structured-output normalization
  -> ReviewInferenceOutcome
  -> existing explain renderer
```

Inference code is split by responsibility:

- `apps/mr-milchick/src/core/inference/mod.rs`: public types, engine trait, timeout orchestration, and provider exports.
- `apps/mr-milchick/src/core/inference/prompt.rs`: the backend-neutral snapshot prompt, strict retry prompt, JSON schema, patch truncation, and review cues.
- `apps/mr-milchick/src/core/inference/output.rs`: response DTOs, JSON extraction, validation, normalization, and recommendation limits.
- `apps/mr-milchick/src/core/inference/local.rs`: llama.cpp model loading, chat templating, generation, decoding, and local retry integration.
- `apps/mr-milchick/src/core/inference/claude_code.rs`: CLI discovery, version validation, isolated subprocess execution, Claude envelope parsing, and remote retry integration.

Resolved inference configuration uses a backend-specific enum so impossible combinations are not propagated into application wiring. `llm-local` and `llm-claude-code` are independent Cargo features and may be compiled separately or together.

The application selects exactly one engine from resolved configuration. Selecting a backend that was not compiled produces an unavailable advisory connector rather than selecting another backend.

## Claude Code Invocation

The remote engine invokes `claude` non-interactively. The review prompt is written to stdin so code and metadata do not appear in process arguments. The invocation uses:

- `--print`
- `--model <configured-model>`
- `--tools ""`
- `--safe-mode`
- `--disable-slash-commands`
- `--no-session-persistence`
- `--permission-mode dontAsk`
- `--output-format json`
- `--json-schema <shared-review-schema>`

The engine must not use `--bare` because Claude Code documents that bare mode does not read OAuth credentials.

The subprocess runs from a newly created empty temporary directory rather than the repository. Its environment is cleared and rebuilt from a fixed allowlist:

- `CLAUDE_CODE_OAUTH_TOKEN`
- `PATH`
- isolated `HOME` and Claude configuration/cache paths
- required locale variables
- certificate variables required for TLS
- explicitly supported HTTP/HTTPS/no-proxy variables

GitLab, GitHub, Slack, Cargo, and unrelated CI variables are not forwarded. Safe mode, the empty working directory, disabled slash commands, and an empty tool list prevent project instructions and repository content from expanding the snapshot-only boundary.

The engine caps captured stdout and stderr. Diagnostics include exit status and bounded sanitized messages but never include the OAuth token or dump the complete prompt. The child is configured to terminate when its future is dropped, and timeout handling explicitly kills and reaps it.

## CLI Compatibility

The supported minimum Claude Code version is `2.1.218`. Connector readiness checks:

1. A non-empty OAuth token is available.
2. `claude` resolves from `PATH`.
3. `claude --version` returns a parseable semantic version at least `2.1.218`.
4. A non-empty model was resolved from TOML.

The demo pins `@anthropic-ai/claude-code` to `2.1.218`. Newer compatible versions are accepted at runtime.

## Prompt and Output Parity

The existing local prompt becomes backend-neutral without changing its bounded snapshot contents. Both engines receive:

- merge-request title, author, branches, draft state, labels, and description;
- changed-file metadata and patch excerpts;
- deterministic review cues;
- the same global `max_patch_bytes` truncation behavior;
- the same recommendation categories and limits.

Both engines target:

```json
{
  "summary": "string or null",
  "recommendations": [
    {
      "category": "ReviewFocus|Risk|TestGap",
      "message": "string"
    }
  ]
}
```

Claude Code is additionally constrained with `--json-schema`. Its CLI JSON envelope is decoded before the shared output normalizer processes the structured review object.

Each engine gets at most two attempts within one overall deadline. The second attempt uses the existing strict prompt and occurs only when the first response is empty or malformed. Authentication failures, missing or incompatible CLI versions, unsupported models, rate limits, and other process failures are not retried. There is no cross-backend fallback.

## Failure Semantics

Invalid TOML, a missing required backend field, or contradictory backend fields remain startup configuration errors.

After configuration succeeds, operational failures are advisory and fail-open:

- Missing token, missing executable, or incompatible CLI: `Unavailable`.
- Overall deadline exceeded: `TimedOut`.
- CLI/API failure or invalid output after the strict retry: `Failed`.
- Valid structured output, including a valid empty review: `Ready`.

These statuses do not fail governance and do not trigger local fallback. `explain` continues through its existing rendering and platform-comment path. If no insights exist, the comment contains no recommendations block, while sanitized diagnostics record the inference status.

## Rendering

The user-facing heading changes from `Local review recommendations` to `AI review recommendations` for both providers. No provider name is injected into the advisory content. Existing templates continue to receive `llm_summary` and `recommendations_block`.

## Cargo Features and Release

Cargo gains:

```toml
llm-claude-code = []
```

It is independent of `llm-local`. Capability output reports local llama.cpp and Claude Code separately.

The standard release build enables:

```text
gitlab slack-app llm-local llm-claude-code
```

Compiling Claude Code support adds no linked model runtime; the `claude` executable remains an optional runtime dependency used only when that backend is enabled.

## GitLab CI Example

`examples/gitlab/claude-code-explain.gitlab-ci.yml` demonstrates the consumer setup. It uses the repository's normal root `mr-milchick.toml` and does not introduce a second config file.

The example:

- builds with `gitlab slack-app llm-claude-code`;
- runs `refine` before a separate `explain` job;
- uses a Node 22 Bookworm image for `explain`;
- defines `CLAUDE_CODE_VERSION: "2.1.218"`;
- installs `@anthropic-ai/claude-code@$CLAUDE_CODE_VERSION`;
- verifies the installed version;
- runs only for same-project merge requests and only when `CLAUDE_CODE_OAUTH_TOKEN` is present.

The example and CI documentation state that the token must be masked, must never be exposed to fork pipelines or untrusted pipeline authors, and carries the effective authority of the associated Claude subscription. Repositories accepting untrusted contributions must run remote review from immutable CI configuration outside contributor control.

## Documentation

The following documentation is updated:

- `mr-milchick.toml`: adjacent commented local and Claude Code examples, field exclusivity, required model, timeout, and secret name.
- `docs/config-reference.md`: schema, defaults, validation matrix, and secret boundary.
- `docs/ci-quickstart.md`: remote build feature, pinned installation, trusted-MR rules, and link to the complete example.
- `docs/connectors-and-capabilities.md`: the new compiled backend and runtime CLI dependency.
- `docs/architecture.md`: provider-neutral advisory inference architecture.
- `docs/local-llm.md`: clarify that it covers only the local provider and cross-link remote setup.
- `docs/README.md`: expose the remote review documentation path.

## Testing

Normal automated tests never contact Anthropic and never require a subscription token.

Resolver tests cover:

- omitted backend resolving to local;
- backend-specific timeout defaults;
- required fields when enabled;
- cross-backend field rejection;
- empty-model rejection;
- secret loading without secret interpolation.

Shared prompt and output tests cover:

- identical bounded snapshot input for either provider;
- global patch truncation;
- review cues;
- category mapping and recommendation limits;
- empty, valid, fenced, malformed, and extraneous output.

Claude Code engine tests use a fake executable injected through the engine constructor. They cover:

- minimum and newer version acceptance;
- old and malformed version rejection;
- exact argument construction;
- prompt delivery through stdin;
- configured model forwarding;
- empty working directory;
- environment allowlisting and absence of platform secrets;
- Claude JSON-envelope extraction;
- bounded stdout and stderr;
- one strict retry for empty or malformed structured output;
- no retry for process failures;
- timeout, child termination, and reaping;
- sanitized status and error mapping.

Application and integration tests cover:

- local, Claude Code, disabled, and uncompiled backend wiring;
- independent and combined Cargo feature compilation;
- neutral recommendation heading;
- acceptance of `CLAUDE_CODE_OAUTH_TOKEN` as a supported secret;
- absence of the token from logs and rendered output.

Verification runs the default test suite, Claude-only feature tests, local-only feature tests, and the combined feature set. The documented demo pipeline is the opt-in live verification path.

## Rollout

1. Publish a binary containing both inference features.
2. Keep existing configurations unchanged; they continue to resolve to local.
3. Add `backend = "claude-code"` and a full versioned `model` in the consuming project's root `mr-milchick.toml`.
4. Add the masked `CLAUDE_CODE_OAUTH_TOKEN` variable only to trusted same-project pipelines.
5. Install pinned Claude Code in the separate `explain` job.
6. Validate with `mr-milchick version`, then run `refine` followed by `explain`.
7. Upgrade the pinned CLI and configured model deliberately after compatibility testing.
