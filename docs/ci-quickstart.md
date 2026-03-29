# CI Quickstart

This guide shows the shortest path to running Mr Milchick in a GitLab merge request pipeline, then layering on Slack delivery. GitHub connector support and tag-based GitHub Releases now ship from the same repository, but the example below keeps the GitLab CI rollout path because it mirrors the existing pipeline model directly.

## What You Need

Mr Milchick currently supports two review connectors and two optional notification sinks:

- review connectors: GitLab, GitHub
- notification sinks: Slack workflow, Slack app

The binary expects platform review context at runtime. Real GitLab reads and writes require `GITLAB_TOKEN`; real GitHub reads and writes require `GITHUB_TOKEN`.

## Minimal Flavor File

The flavor file is optional, but it is the cleanest way to declare which compiled notification sinks should be active in a given environment.

```toml
[review_platform]
kind = "gitlab"

[[notifications]]
kind = "slack-workflow"
enabled = true

[[notifications]]
kind = "slack-app"
enabled = false
```

Save that as `mr-milchick.toml` in the repo root, or point `MR_MILCHICK_FLAVOR_PATH` at another file. If you omit the file entirely, compiled notification sinks are treated as enabled by default.

For GitHub builds, switch `kind = "gitlab"` to `kind = "github"`.

## Example GitLab Pipeline

This example builds a Linux artifact with the currently implemented capabilities, verifies the artifact with `version`, and starts with a safe `observe` rollout job.

```yaml
stages:
  - build
  - review

variables:
  MR_MILCHICK_REVIEWERS: >-
    [{"username":"milchick-duty","fallback":true},
     {"username":"principal-reviewer","mandatory":true},
     {"username":"alice","areas":["frontend","packages"]},
     {"username":"carol","areas":["backend"]},
     {"username":"grace","areas":["devops"]}]
  MR_MILCHICK_MAX_REVIEWERS: "2"
  MR_MILCHICK_SLACK_ENABLED: "true"

build:milchick:
  stage: build
  image: rust:1.87
  before_script:
    - rustup target add x86_64-unknown-linux-musl
    - apt-get update && apt-get install -y musl-tools pkg-config
  script:
    - cargo build --release --target x86_64-unknown-linux-musl --no-default-features --features "gitlab slack-app slack-workflow"
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

Once the planned output looks right, rename the job and switch the command to `./dist/mr-milchick refine`.

## GitHub Releases

This repository now includes [`.github/workflows/release.yml`](../.github/workflows/release.yml). On tag pushes it:

- verifies the tagged commit is on `master`
- runs `cargo test --workspace --locked`
- builds Linux musl artifacts for both `gitlab` and `github`
- publishes a GitHub Release with both connector-specific binaries and flavor examples

For day-to-day GitHub pull request execution, [`.github/workflows/review.yml`](../.github/workflows/review.yml) starts the connector in `observe` mode on `pull_request` and points `MR_MILCHICK_FLAVOR_PATH` at [`mr-milchick.github.toml`](../mr-milchick.github.toml). Keep that workflow on `observe` until the output matches your expectations, then switch it to `refine` when you are ready for live reviewer assignment and summary upserts.

## Required Variables

Store these in CI variables or secrets:

- `GITLAB_TOKEN`: required for live GitLab snapshot reads and mutations.
- `MR_MILCHICK_REVIEWERS`: reviewer pool JSON for area routing, fallback reviewers, and mandatory reviewers.

These GitLab CI variables are read from the pipeline environment:

- `CI_PROJECT_ID`
- `CI_MERGE_REQUEST_IID`
- `CI_PIPELINE_SOURCE`
- `CI_MERGE_REQUEST_SOURCE_BRANCH_NAME`
- `CI_MERGE_REQUEST_TARGET_BRANCH_NAME`
- `CI_MERGE_REQUEST_LABELS`

## Slack Workflow Setup

Slack workflow is the lower-permission option. Compile the binary with `slack-workflow`, enable that sink in `mr-milchick.toml`, and set:

```bash
MR_MILCHICK_SLACK_ENABLED=true
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/...
```

The webhook must be a Slack Workflow input webhook, not a generic incoming webhook. Mr Milchick sends one trigger payload with:

- `mr_milchick_talks_to`
- `mr_milchick_says`
- `mr_milchick_says_thread`

Your workflow is responsible for posting the compact parent message and the threaded follow-up.

## Slack App Setup

Slack app is the richer direct-posting option. Compile the binary with `slack-app`, enable that sink in `mr-milchick.toml`, and set:

```bash
MR_MILCHICK_SLACK_ENABLED=true
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-...
```

If you want GitLab usernames rewritten to real Slack mentions, also set:

```bash
MR_MILCHICK_SLACK_USER_MAP='{"principal-reviewer":"U01234567","alice":"U07654321"}'
```

The environment variable wins over `[slack_app.user_map]` in `mr-milchick.toml`.

## Rollout Path

Use this order to enable the tool safely:

1. Build the artifact and run `mr-milchick version` in CI logs.
2. Run `observe` until the planned findings and actions match your expectations.
3. If you want execution logs without external mutation, temporarily set `MR_MILCHICK_DRY_RUN=true` and run `refine`.
4. Remove dry-run and keep `refine` once the pipeline behavior is stable.

## When Notifications Are Sent

Slack delivery only happens during real `refine` execution. Notifications are skipped when:

- `MR_MILCHICK_DRY_RUN` is enabled
- the planned outcome still contains blocking findings
- the action plan contains `FailPipeline`
- no reviewer assignment action is needed

For the full variable reference, see [config-reference.md](config-reference.md). For the runtime capability model, see [connectors-and-capabilities.md](connectors-and-capabilities.md).
