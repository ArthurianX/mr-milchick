# AGENTS.md

## Project Overview

Mr. Milchick is a Rust CLI binary (`mr-milchick`) that enforces merge request governance inside GitLab CI pipelines. It is **not** a service or bot — it runs as a single invocation per pipeline, reads CI environment variables, evaluates policy rules, and optionally mutates GitLab (assign reviewers, post comments, fail pipeline).

Three subcommands: `observe` (dry-run evaluation), `refine` (execute actions), `explain` (deep reasoning output with snapshot details).

## Architecture & Data Flow

The pipeline through `src/app.rs` follows a strict linear flow:

```
CI env vars (context/raw.rs → context/builder.rs → context/model.rs)
  → Rule engine (rules/engine.rs evaluates all rules, currently branch_policy.rs)
  → GitLab snapshot fetch (gitlab/client.rs → gitlab/api.rs domain models)
  → Domain analysis (domain/snapshot_analysis.rs → path_classifier.rs → area_summary.rs)
  → Reviewer routing (domain/reviewer_routing.rs + domain/codeowners/)
  → Action planning (actions/planner.rs enriches RuleOutcome with reviewer assignments)
  → Comment rendering (comment/render.rs produces markdown with MR_MILCHICK_MARKER)
  → Execution (actions/executor.rs trait, DryRunExecutor or executor/gitlab.rs)
```

**Key design rule**: `rules/` is pure logic with no side effects. All mutations flow through `actions/` only.

## Build & Test

```bash
cargo build            # compile
cargo test             # all unit tests (inline in each module)
cargo run -- observe   # requires CI env vars (see below)
```

Tests are co-located with source code (`#[cfg(test)] mod tests` blocks), not in a separate `/tests` directory. When adding a new module, add tests in the same file.

## Local Smoke Testing

The tool reads GitLab CI env vars. For local runs, set them manually. See `docs/local-testing.md` for complete examples. Minimal invocation:

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
cargo run -- observe
```

Real GitLab API calls require `GITLAB_TOKEN` and optionally `GITLAB_BASE_URL` (defaults to `https://gitlab.com/api/v4`).

## Key Conventions

- **Newtype wrappers for stringly-typed data**: `ProjectId(String)`, `MergeRequestIid(String)`, `BranchName(String)`, `Label(String)` in `context/model.rs`. Always wrap raw strings in domain types.
- **Runtime config from environment variables**: `config/loader.rs` builds `RuntimeConfig` from CI-provided environment variables. Reviewer routing comes from `MR_MILCHICK_REVIEWERS` JSON and `MR_MILCHICK_MAX_REVIEWERS`; CODEOWNERS behavior comes from `MR_MILCHICK_CODEOWNERS_ENABLED` and `MR_MILCHICK_CODEOWNERS_PATH`.
- **GitLab DTO separation**: `gitlab/dto.rs` holds serde-deserialized API responses; `gitlab/api.rs` holds domain models. The client in `gitlab/client.rs` maps DTOs → domain types.
- **Error handling**: `anyhow::Result` for application errors; `thiserror` for `AppError` enum in `error.rs`. Use `anyhow::bail!` / `.context()` for enriched error messages.
- **Async via tokio**: `#[tokio::main]` in `main.rs`. The `ActionExecutor` trait uses `#[async_trait]`. Only the GitLab client layer and execution are async.

## Adding a New Rule

1. Create `src/rules/your_rule.rs` with a function `pub fn evaluate_your_rule(ctx: &CiContext) -> RuleOutcome`
2. Use `RuleFinding::info()`, `::warning()`, or `::blocking()` for findings
3. Push `Action::FailPipeline { reason }` to `outcome.action_plan` if the rule should block
4. Register it in `src/rules/engine.rs` by adding to the `outcomes` array
5. Add `pub mod your_rule;` to `src/rules/mod.rs`

## Adding a New Code Area

1. Add variant to `CodeArea` enum in `domain/code_area.rs` (with `as_str()`)
2. Add path matching rules in `domain/path_classifier.rs` — order matters (first match wins)
3. Ensure the new area is recognized by `CodeArea::from_config_key()` in `domain/code_area.rs` if it needs a config key alias
4. Keep `ReviewerRoutingConfig::from_config()` in `domain/reviewer_routing.rs` compatible with the new area
5. Update env-based examples and docs that demonstrate `MR_MILCHICK_REVIEWERS` payloads

## Tone System

Tone is deterministic, not random. Messages are selected by hashing MR identity (`project_id:mr_iid:category`). The tone registry (`tone/registry.rs`) maps `ToneCategory` → static message arrays. When adding messages, append to existing arrays — insertion order affects determinism for existing MRs.

## GitLab Executor Idempotency

`executor/gitlab.rs` checks existing MR notes before posting — it skips duplicate comments and updates existing ones matched by `MR_MILCHICK_MARKER` (`<!-- mr-milchick:summary -->`). This marker is defined in `comment/render.rs`.

## Environment Variables

| Variable | Required | Purpose |
|---|---|---|
| `CI_PROJECT_ID` | Yes | GitLab project ID |
| `CI_MERGE_REQUEST_IID` | For MR pipelines | MR identifier |
| `CI_PIPELINE_SOURCE` | Yes | Must be `merge_request_event` for MR mode |
| `CI_MERGE_REQUEST_SOURCE_BRANCH_NAME` | For MR pipelines | Source branch |
| `CI_MERGE_REQUEST_TARGET_BRANCH_NAME` | For MR pipelines | Target branch |
| `CI_MERGE_REQUEST_LABELS` | No | Comma-separated labels |
| `GITLAB_TOKEN` | For real execution | GitLab API token |
| `GITLAB_BASE_URL` | No | Defaults to `https://gitlab.com/api/v4` |
| `MR_MILCHICK_REVIEWERS` | No | JSON array of reviewer capability objects used for routing |
| `MR_MILCHICK_MAX_REVIEWERS` | No | Max number of area-routed reviewers to add; defaults to `2` |
| `MR_MILCHICK_DRY_RUN` | No | `true`/`1`/`yes` to force `refine` into dry-run execution |
| `MR_MILCHICK_CODEOWNERS_ENABLED` | No | `true` by default; set to `false` to disable CODEOWNERS routing |
| `MR_MILCHICK_CODEOWNERS_PATH` | No | Overrides CODEOWNERS auto-discovery path |
