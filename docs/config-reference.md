# Configuration Reference

This document lists the configuration inputs Mr. Milchick actually uses today.

## Config Sources

Mr. Milchick uses two configuration sources:

- environment variables for runtime behavior
- [`mr-milchick.toml`](mr-milchick.toml) for the optional flavor manifest

These sources do not currently overlap much.

Environment variables control reviewer routing, CODEOWNERS behavior, Slack credentials, dry-run mode, CI context, and GitLab access.

The TOML file currently controls:

- `review_platform.kind`
- `[[notifications]]`
- `[slack_app.user_map]`

`MR_MILCHICK_FLAVOR_PATH` can be used to point the app at a different flavor TOML file.

## Flavor TOML

Minimal example:

```toml
[review_platform]
kind = "gitlab"

[[notifications]]
kind = "slack-workflow"
enabled = true

[[notifications]]
kind = "slack-app"
enabled = false

[slack_app.user_map]
engineer.lady1 = "U01234567"
engineer.guy1 = "U07654321"
```

Notes:

- the review platform in the TOML must match the review connector compiled into the binary
- listed notification sinks must also be compiled into the binary
- when no `[[notifications]]` entries are present, compiled notification sinks are treated as enabled by default

## Runtime Environment Variables

### CI Context

Required for merge request pipeline execution:

```bash
CI_PROJECT_ID=412
CI_MERGE_REQUEST_IID=3995
CI_PIPELINE_SOURCE=merge_request_event
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop
CI_MERGE_REQUEST_LABELS="0. run-tests,3. Ready to be merged"
```

### Reviewer Routing

```bash
MR_MILCHICK_REVIEWERS='[
  {"username":"milchick-duty","fallback":true},
  {"username":"principal-reviewer","mandatory":true},
  {"username":"alice","areas":["frontend","packages"]},
  {"username":"carol","areas":["backend"]},
  {"username":"grace","areas":["devops"]}
]'
MR_MILCHICK_MAX_REVIEWERS=2
```

Notes:

- `MR_MILCHICK_MAX_REVIEWERS` limits the area-routed portion of reviewer selection
- mandatory reviewers do not consume that budget
- matched CODEOWNERS sections override area-based routing, so the cap does not apply in that path

See [`docs/reviewer-routing.md`](docs/reviewer-routing.md) for the exact behavior.

### CODEOWNERS

```bash
MR_MILCHICK_CODEOWNERS_ENABLED=true
MR_MILCHICK_CODEOWNERS_PATH=.github/CODEOWNERS
```

Notes:

- `MR_MILCHICK_CODEOWNERS_ENABLED` defaults to `true`
- if no path is provided, Milchick auto-discovers common CODEOWNERS locations

### Dry Run

```bash
MR_MILCHICK_DRY_RUN=true
```

This forces `refine` into non-mutating mode.

### Slack Non-Secrets

```bash
MR_MILCHICK_SLACK_ENABLED=true
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X
MR_MILCHICK_SLACK_BASE_URL=https://slack.com/api
MR_MILCHICK_SLACK_USER_MAP='{"engineer.lady1":"U01234567","engineer.guy1":"U07654321"}'
```

`MR_MILCHICK_SLACK_BASE_URL` is mainly useful for tests and local mocks.

`MR_MILCHICK_SLACK_USER_MAP` is a JSON object keyed by GitLab username. Values should be Slack user IDs like `U01234567`. When present, the Slack app sink rewrites `@gitlab.username` mentions into Slack `<@U01234567>` mentions so users get pinged in Slack. Empty values are ignored, and the environment variable takes precedence over `[slack_app.user_map]` in TOML.

### Secret Environment Variables

Keep these in CI variables or local shell env, not in the repo:

```bash
GITLAB_TOKEN=your-gitlab-token
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/...
```

Optional endpoint override:

```bash
GITLAB_BASE_URL=https://gitlab.com/api/v4
```

### Flavor Override

```bash
MR_MILCHICK_FLAVOR_PATH=mr-milchick.toml
```

This selects which flavor TOML file is loaded at runtime.
