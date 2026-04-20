# Local Testing

Local testing now follows the same boundary as CI:

- review context from env
- runtime config from `mr-milchick.toml`
- secrets from env

## Minimal Local Config

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
areas = ["frontend"]

[[reviewers.definitions]]
username = "carol"
areas = ["backend"]
```

## Minimal Observe Run

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- observe
```

## Dry-Run Refine

```toml
[execution]
dry_run = true
```

Then run:

```bash
cargo run -- refine
```

That is useful for previewing governance execution, but because `dry_run` does not post the managed summary comment to the review platform, a later live `cargo run -- explain` run will skip.

## Live Refine And Explain

If you want to test the full governance-plus-advisory flow against a live review, build with `llm-local`, enable `[inference]`, turn `dry_run` off, run `refine` once, then run `explain`:

```toml
[execution]
dry_run = false
```

```bash
GITLAB_TOKEN=your-gitlab-token cargo run -- refine
GITLAB_TOKEN=your-gitlab-token cargo run -- explain
```

`explain` rereads Milchick's managed governance summary comment and only runs the advisory pass when that latest `refine` reported applied governance effect or a blocking outcome.

## Local Slack Testing

Slack config belongs in TOML:

```toml
[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
base_url = "https://slack.com/api"
```

Secrets stay in env:

```bash
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token cargo run -- refine
```

For Slack workflow testing:

```toml
[notifications.slack_workflow]
enabled = true
channel = "C0ALY38CW3X"
```

```bash
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/... cargo run -- refine
```

`explain` never sends Slack notifications.

## Alternate Config Path

Use `MR_MILCHICK_CONFIG_PATH` when you want a local test config that differs from the checked-in `mr-milchick.toml`.
