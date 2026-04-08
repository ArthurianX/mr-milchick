# CI Quickstart

This is the shortest path to running Mr Milchick safely in CI with the new config boundary.

## What You Need

- one compiled platform connector: GitLab or GitHub
- optional notification sinks: Slack app and/or Slack workflow
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

Once the output looks right, switch `observe` to `refine`.

## Required Secrets

- `GITLAB_TOKEN` for GitLab live reads and writes
- `GITHUB_TOKEN` for GitHub live reads and writes
- `MR_MILCHICK_SLACK_BOT_TOKEN` for Slack app delivery
- `MR_MILCHICK_SLACK_WEBHOOK_URL` for Slack workflow delivery

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

## Slack Workflow Setup

```toml
[notifications.slack_workflow]
enabled = true
channel = "C0ALY38CW3X"
```

Set:

- `MR_MILCHICK_SLACK_WEBHOOK_URL`

The webhook must be a Slack Workflow input webhook. Milchick sends:

- `mr_milchick_talks_to`
- `mr_milchick_says`
- `mr_milchick_says_thread`

## Interpolation Notes

- TOML supports `${VAR}` for required env vars.
- TOML supports `${VAR:-default}` for optional env vars with a fallback.
- Keep string substitutions quoted, for example `model_path = "${CI_PROJECT_DIR}/models/review.gguf"`.
- Scalar substitutions can stay unquoted, for example `enabled = ${MR_MILCHICK_LLM_ENABLED:-false}`.
- This syntax is specific to Mr Milchick and is resolved before TOML parsing, so IDE TOML validators may still flag it.

## Optional Prior-Job Status Input

Some internal CI setups have earlier jobs write compact JSON files under `milchick-status/` directories and then let Milchick include those results in Slack notifications.

If you use that pattern, enable:

```toml
[platform.gitlab]
all_pipelines_pass_label = "ready-to-merge"

[notifications.pipeline_status]
enabled = true
fail_pipeline_on_failed = true
search_root = "${CI_PROJECT_DIR}"
```

Milchick will recursively scan for `*/milchick-status/*.json` before rendering Slack notifications. If `all_pipelines_pass_label` is configured, Milchick also uses those parsed results to add the GitLab label only when every status entry passed. If `fail_pipeline_on_failed = true`, Milchick will fail the current pipeline when any parsed status entry is explicitly failed, which is useful when earlier fan-in jobs are allowed to fail and Milchick is the final enforcement point. If no status files are found, Milchick warns and skips both the label action and the failure gate.

## Safe Rollout

1. Run `mr-milchick version` in CI to confirm the built capabilities.
2. Start with `observe`.
3. If you want execution-shaped output without external mutation, set `[execution] dry_run = true` and run `refine`.
4. Turn `dry_run` back off once the behavior is stable.

For the full schema, see [config-reference.md](config-reference.md).
