# CI Quickstart

This is the shortest path to running Mr Milchick safely in CI with the new config boundary.

## What You Need

- one compiled platform connector: GitLab or GitHub
- optional notification sink: Slack app
- a `mr-milchick.toml` file committed with non-secret runtime config
- secrets supplied through CI variables

## Minimal Config

```toml
[platform]
kind = "gitlab"

[reviewers]
max_reviewers = 2

[[reviewers.definitions]]
username = "milchick-duty"
fallback = true

[[reviewers.definitions]]
username = "principal-reviewer"
mandatory = true

[[reviewers.definitions]]
username = "alice"
areas = ["frontend", "packages"]

[[reviewers.definitions]]
username = "carol"
areas = ["backend"]

[inference]
enabled = ${MR_MILCHICK_LLM_ENABLED:-false}
model_path = "${CI_PROJECT_DIR}/models/microcoder-1.5b-Q5_K_M.gguf"

[notifications.pipeline_status]
enabled = true
search_root = "${CI_PROJECT_DIR}"
```

Put that in `mr-milchick.toml` at the repo root. If you need another path, set `MR_MILCHICK_CONFIG_PATH`.
That `[inference]` section only affects `explain`; `observe` and `refine` never invoke inference. To use Claude Code instead of local GGUF inference, set `backend = "claude-code"`, provide a `model`, build with `llm-claude-code`, and supply the masked `CLAUDE_CODE_OAUTH_TOKEN` only to trusted same-project merge requests.

## Example GitLab Pipeline

```yaml
stages:
  - build
  - review

build:milchick:
  stage: build
  image: rust:1.87
  before_script:
    - rustup target add x86_64-unknown-linux-musl
    - apt-get update && apt-get install -y musl-tools pkg-config
  script:
    - cargo build --release --target x86_64-unknown-linux-musl --no-default-features --features "gitlab slack-app"
    - mkdir -p dist
    - cp target/x86_64-unknown-linux-musl/release/mr-milchick dist/
  artifacts:
    paths:
      - dist/mr-milchick

milchick:observe:
  stage: review
  image: debian:bookworm-slim
  needs: ["build:milchick"]
  script:
    - ./dist/mr-milchick version
    - ./dist/mr-milchick observe
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
```

`observe` is now the intake gate. It may apply labels, send the Slack intake thread reply, and fail early when intake requirements are not met.

## Recommended Job Shape

The intended rollout now looks like this:

1. Run `observe` first to label risk, handle drafts, validate the description, and open/update the Slack MR thread.
2. Run `refine` after the other CI jobs have produced their status signals.
3. If you want advisory LLM commentary, add a separate slower `explain` job after a real `refine` run.

That split keeps the main governance job quick while leaving richer advisory analysis to a later follow-up stage or pipeline.

If you want `explain` to produce advisory output, build the binary with `llm-local`, enable `[inference]`, and add a follow-up job such as:

```yaml
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
  image: debian:bookworm-slim
  needs: ["build:milchick", "milchick:refine"]
  script:
    - ./dist/mr-milchick explain
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
```

`explain` reloads Milchick's managed governance summary comment from the review platform and skips itself when the latest `refine` run reported no governance effect and no blocking outcome.

For remote advisory review, set `backend = "claude-code"` and a non-empty `model` in the root config, build with `llm-claude-code`, and provide masked `CLAUDE_CODE_OAUTH_TOKEN` only to trusted same-project merge requests. See [the pinned GitLab example](../examples/gitlab/claude-code-explain.gitlab-ci.yml) and [Claude Code advisory review](claude-code-llm.md).

## Required Secrets

- `GITLAB_TOKEN` for GitLab live reads and writes
- `GITHUB_TOKEN` for GitHub live reads and writes
- `MR_MILCHICK_SLACK_BOT_TOKEN` for Slack app delivery

## Review Context Env

GitLab CI still provides:

- `CI_PROJECT_ID`
- `CI_MERGE_REQUEST_IID`
- `CI_PIPELINE_SOURCE`
- `CI_MERGE_REQUEST_SOURCE_BRANCH_NAME`
- `CI_MERGE_REQUEST_TARGET_BRANCH_NAME`
- `CI_MERGE_REQUEST_LABELS`

GitHub Actions still provides the corresponding `GITHUB_*` review context.

## Slack App Setup

```toml
[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"

[notifications.slack_app.user_map]
"principal-reviewer" = "U01234567"
"alice" = "U07654321"
```

Set:

- `MR_MILCHICK_SLACK_BOT_TOKEN`

The installed Slack app needs `chat:write`, `metadata.message:read`, `im:write`, `im:history`, and the relevant shared-channel history scope (`channels:history` for public channels or `groups:history` for private channels). Reinstall the Slack app after changing scopes. A valid `[notifications.slack_app.user_map]` entry is required for author failure DMs; missing mappings never fall back to the shared channel.

## Interpolation Notes

- TOML supports `${VAR}` for required env vars.
- TOML supports `${VAR:-default}` for optional env vars with a fallback.
- Keep string substitutions quoted, for example `model_path = "${CI_PROJECT_DIR}/models/review.gguf"`.
- Scalar substitutions can stay unquoted, for example `enabled = ${MR_MILCHICK_LLM_ENABLED:-false}`.
- This syntax is specific to Mr Milchick and is resolved before TOML parsing, so IDE TOML validators may still flag it.

## Optional Prior-Job Status Input

Some internal CI setups have earlier jobs write JSON files under `milchick-status/` directories and then let Milchick include those results in Slack notifications. A parsed failure sends one author DM per failing head commit; passed and unknown results do not create activity messages.

If you use that pattern, enable:

```toml
[platform.gitlab]

[[platform.gitlab.label_rules]]
name = "ready-to-merge-after-green-statuses"
add = ["ready-to-merge"]
remove = ["tests-are-failing"]

[platform.gitlab.label_rules.when]
all = [{ pipeline_state = "passed" }]

[notifications.pipeline_status]
enabled = true
fail_pipeline_on_failed = true
search_root = "${CI_PROJECT_DIR}"
```

Milchick will recursively scan for `*/milchick-status/*.json` before rendering Slack notifications. Label rules can also use those parsed results as `pipeline_state` when `MR_MILCHICK_PIPELINE_STATE` is not set. If `fail_pipeline_on_failed = true`, Milchick will fail the current pipeline when any parsed status entry is explicitly failed, which is useful when earlier fan-in jobs are allowed to fail and Milchick is the final enforcement point. If no status files are found, Milchick warns and skips the failure gate.

## Safe Rollout

1. Run `mr-milchick version` in CI to confirm the built capabilities.
2. Start with `observe`.
3. If you want execution-shaped output without external mutation, set `[execution] dry_run = true` and run `refine`.
4. Turn `dry_run` back off once the behavior is stable.
5. Add a separate `explain` job only after a real `refine` run exists on the review platform.

With `dry_run = true`, `refine` previews the governance summary but does not post it to the platform, so a later live `explain` run has nothing to reread and will skip.

For the full schema, see [config-reference.md](config-reference.md).
