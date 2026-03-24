# Build Pipeline Examples

This document shows a few practical ways to build and run Mr. Milchick in CI with the connector-based workspace layout.

## Example 1: Default Build

Build the default binary with GitLab, Slack app, and Slack workflow support:

```bash
cargo build -p mr-milchick --release
```

Use this when one artifact is acceptable for all environments.

## Example 2: Minimal GitLab-Only Build

Build a binary with no notification sinks:

```bash
cargo build -p mr-milchick --release --no-default-features --features gitlab
```

Use this when review mutations are required but outbound notifications are not allowed.

## Example 3: GitLab Plus Slack Workflow Build

Build a lower-permission notification variant:

```bash
cargo build -p mr-milchick --release --no-default-features --features "gitlab slack-workflow"
```

Recommended runtime environment:

```bash
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/...
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X
MR_MILCHICK_SLACK_ENABLED=true
```

Use this when Slack app installation requires admin approval and a workflow input webhook is easier to obtain.

## Example 4: GitLab Plus Slack App Build

Build the richer Slack integration variant:

```bash
cargo build -p mr-milchick --release --no-default-features --features "gitlab slack-app"
```

Recommended runtime environment:

```bash
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-...
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X
MR_MILCHICK_SLACK_ENABLED=true
```

Use this when the workspace allows app-based posting and threaded replies from Milchick directly.

## GitLab CI Example: Build Artifact

```yaml
stages:
  - build
  - review

build:milchick:
  stage: build
  image: rust:1.87
  script:
    - cargo build -p mr-milchick --release --no-default-features --features "gitlab slack-workflow"
    - mkdir -p dist
    - cp target/release/mr-milchick dist/
  artifacts:
    paths:
      - dist/mr-milchick
```

## GitLab CI Example: Observe Job

```yaml
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

## GitLab CI Example: Refine Job With Slack Workflow

```yaml
milchick:refine:
  stage: review
  image: debian:bookworm-slim
  needs: ["build:milchick"]
  variables:
    MR_MILCHICK_SLACK_ENABLED: "true"
  script:
    - ./dist/mr-milchick version
    - ./dist/mr-milchick refine
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
  secrets:
    GITLAB_TOKEN:
      vault: production/milchick/gitlab-token
    MR_MILCHICK_SLACK_WEBHOOK_URL:
      vault: production/milchick/slack-workflow-url
```

## GitLab CI Example: Parallel Flavor Builds

If different teams need different compiled capabilities, build separate artifacts:

```yaml
build:milchick:slack-app:
  stage: build
  image: rust:1.87
  script:
    - cargo build -p mr-milchick --release --no-default-features --features "gitlab slack-app"
    - mkdir -p dist/slack-app
    - cp target/release/mr-milchick dist/slack-app/mr-milchick
  artifacts:
    paths:
      - dist/slack-app/mr-milchick

build:milchick:slack-workflow:
  stage: build
  image: rust:1.87
  script:
    - cargo build -p mr-milchick --release --no-default-features --features "gitlab slack-workflow"
    - mkdir -p dist/slack-workflow
    - cp target/release/mr-milchick dist/slack-workflow/mr-milchick
  artifacts:
    paths:
      - dist/slack-workflow/mr-milchick
```

## Operational Notes

- Keep CI metadata and secrets in environment variables; the flavor file is additive, not a replacement.
- Use `mr-milchick version` in pipeline logs to confirm the artifact you built is the one you are actually running.
- Match the flavor file notification list to the binary features you compiled.
- Prefer the Slack workflow sink in lower-permission workspaces; prefer the Slack app sink when richer Slack API access is allowed.
