# AGENTS.md

## Project Overview

Mr. Milchick is a Rust CLI binary (`mr-milchick`) that enforces merge request governance inside GitLab CI pipelines. It is **not** a service or bot — it runs as a single invocation per pipeline, reads CI environment variables, evaluates policy rules, and optionally mutates GitLab (assign reviewers, post comments, fail pipeline).

Three subcommands: `observe` (dry-run evaluation), `refine` (execute actions), `explain` (deep reasoning output with snapshot details).

## Architecture & Data Flow

Mr. Milchick now publishes as a single crate from the repository root. The architecture still uses explicit internal layers under `apps/mr-milchick/src/`.

The pipeline through `apps/mr-milchick/src/app.rs` follows a strict linear flow:

```
CI env vars (context/raw.rs → context/builder.rs → context/model.rs)
  → Rule engine (core/rules/engine.rs evaluates all rules, currently branch_policy.rs)
  → GitLab snapshot fetch (connectors/gitlab/client.rs → connectors/gitlab/api.rs domain models)
  → Domain analysis (core/domain/snapshot_analysis.rs → path_classifier.rs → area_summary.rs)
  → Reviewer routing (core/domain/reviewer_routing.rs + core/domain/codeowners/)
  → Action planning (core/actions/planner.rs enriches RuleOutcome with reviewer assignments)
  → Comment rendering (core/comment/render.rs produces markdown with MR_MILCHICK_MARKER)
  → Execution (runtime/executor.rs traits + connectors implementations)
```

**Key design rule**: `core/rules/` is pure logic with no side effects. All mutations flow through `runtime/` and `connectors/` only.

## Build & Test

```bash
cargo build            # compile
cargo test             # all unit tests (inline in each module)
cargo run -- observe   # requires CI env vars (see below)
```

Tests are co-located with source code (`#[cfg(test)] mod tests` blocks), not in a separate `/tests` directory. When adding a new module, add tests in the same file.
Integration tests also live at the repository root in `/tests` for the packaged crate boundary.

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

- **Newtype wrappers for stringly-typed data**: `ProjectId(String)`, `MergeRequestIid(String)`, `BranchName(String)`, `Label(String)` in `apps/mr-milchick/src/context/model.rs`. Always wrap raw strings in domain types.
- **Runtime config from environment variables**: `apps/mr-milchick/src/config/loader.rs` builds `RuntimeConfig` from CI-provided environment variables. Reviewer routing comes from `MR_MILCHICK_REVIEWERS` JSON and `MR_MILCHICK_MAX_REVIEWERS`; CODEOWNERS behavior comes from `MR_MILCHICK_CODEOWNERS_ENABLED` and `MR_MILCHICK_CODEOWNERS_PATH`; optional Slack notifications come from `MR_MILCHICK_SLACK_BOT_TOKEN` for the Slack app sink, `MR_MILCHICK_SLACK_WEBHOOK_URL` for the Slack Workflow webhook sink, `MR_MILCHICK_SLACK_CHANNEL`, `MR_MILCHICK_SLACK_ENABLED`, and optionally `MR_MILCHICK_SLACK_BASE_URL` for testing.
- **GitLab DTO separation**: `apps/mr-milchick/src/connectors/gitlab/dto.rs` holds serde-deserialized API responses; `apps/mr-milchick/src/connectors/gitlab/api.rs` holds domain models. The client in `apps/mr-milchick/src/connectors/gitlab/client.rs` maps DTOs → domain types.
- **Error handling**: `anyhow::Result` for application errors; `thiserror` for `AppError` enum in `apps/mr-milchick/src/error.rs`. Use `anyhow::bail!` / `.context()` for enriched error messages.
- **Async via tokio**: `#[tokio::main]` in `main.rs`. The `ActionExecutor` trait uses `#[async_trait]`. Only the GitLab client layer and execution are async.

## Adding a New Rule

1. Create `apps/mr-milchick/src/core/rules/your_rule.rs` with a function `pub fn evaluate_your_rule(ctx: &CiContext) -> RuleOutcome`
2. Use `RuleFinding::info()`, `::warning()`, or `::blocking()` for findings
3. Push `Action::FailPipeline { reason }` to `outcome.action_plan` if the rule should block
4. Register it in `apps/mr-milchick/src/core/rules/engine.rs` by adding to the `outcomes` array
5. Add `pub mod your_rule;` to `apps/mr-milchick/src/core/rules/mod.rs`

## Adding a New Code Area

1. Add variant to `CodeArea` enum in `apps/mr-milchick/src/core/domain/code_area.rs` (with `as_str()`)
2. Add path matching rules in `apps/mr-milchick/src/core/domain/path_classifier.rs` — order matters (first match wins)
3. Ensure the new area is recognized by `CodeArea::from_config_key()` in `apps/mr-milchick/src/core/domain/code_area.rs` if it needs a config key alias
4. Keep `ReviewerRoutingConfig::from_config()` in `apps/mr-milchick/src/core/domain/reviewer_routing.rs` compatible with the new area
5. Update env-based examples and docs that demonstrate `MR_MILCHICK_REVIEWERS` payloads

## Tone System

Tone is deterministic, not random. Messages are selected by hashing MR identity (`project_id:mr_iid:category`). The tone registry (`apps/mr-milchick/src/core/tone/registry.rs`) maps `ToneCategory` → static message arrays. When adding messages, append to existing arrays — insertion order affects determinism for existing MRs.

## GitLab Executor Idempotency

The GitLab connector checks existing MR notes before posting — it skips duplicate comments and updates existing ones matched by `MR_MILCHICK_MARKER` (`<!-- mr-milchick:summary -->`). This marker is defined in `apps/mr-milchick/src/connectors/gitlab/mod.rs` and rendered from `apps/mr-milchick/src/core/comment/render.rs`.

## Slack Notifications

Slack review notifications are optional and only fire during real `refine` execution when reviewers were actually assigned and the pipeline is not being failed. Mr. Milchick supports both a Slack app sink and a Slack Workflow webhook sink. The workflow webhook variant is designed for lower-permission setups where a Slack app would require admin approval; Milchick sends one workflow trigger payload with `mr_milchick_talks_to`, `mr_milchick_says`, and `mr_milchick_says_thread`, and the workflow itself is responsible for posting the light parent message and threaded follow-up.

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
| `MR_MILCHICK_SLACK_BOT_TOKEN` | No | Slack bot OAuth token used by the Slack app sink |
| `MR_MILCHICK_SLACK_WEBHOOK_URL` | No | Slack Workflow webhook URL used by the Slack workflow sink |
| `MR_MILCHICK_SLACK_CHANNEL` | No | Slack channel ID used by Slack sinks and passed to the workflow payload when using the webhook sink |
| `MR_MILCHICK_SLACK_ENABLED` | No | `true` by default; set to `false` to disable Slack notifications even when webhook/channel are present |
| `MR_MILCHICK_SLACK_BASE_URL` | No | Overrides the Slack API base URL; intended for tests and local mocks |
