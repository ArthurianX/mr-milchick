# Local Testing

Local testing now follows the same boundary as CI:

- review context from env
- mandatory runtime config from TOML
- secrets from env

`mr-milchick.toml` is the default config path, but a valid TOML config is not optional in v5 because repository areas are required. Use `MR_MILCHICK_CONFIG_PATH` when testing against a scratch config.

## Minimal Local Config File

```bash
cat >/tmp/milchick-local.toml <<'TOML'
[platform]
kind = "gitlab"

[[areas.definitions]]
key = "frontend"
paths = ["apps/frontend/**"]
risk = "medium"

[[areas.definitions]]
key = "backend"
paths = ["services/**"]
risk = "medium"

[[areas.definitions]]
key = "ci"
paths = [".gitlab-ci.yml", ".github/**", "scripts/**"]
risk = "high"

[observe.description]
required = true
template_paths = [".gitlab/merge_request_templates/Default.md"]
ignore_branch_issue_key = true

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
TOML
```

`areas.definitions` is required in v5. Reviewer `areas` must reference those keys exactly.

Export the config path before running local commands:

```bash
export MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml
```

You can also export the review context once instead of repeating it on every command:

```bash
export CI_PROJECT_ID=412
export CI_MERGE_REQUEST_IID=3995
export CI_PIPELINE_SOURCE=merge_request_event
export CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example
export CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop
export CI_MERGE_REQUEST_LABELS=""
```

## Live GitLab OBSERVE Intake

Use a real but low-risk test MR, because live `observe` now mutates review state:

- removes the other configured risk labels
- applies exactly one `risk::*` label
- applies the draft label when the MR is draft
- sends a Slack app intake message when Slack app is enabled
- exits nonzero if intake requirements are not met

Set review context to the real GitLab project and MR:

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- observe
```

For GitLab, use a token that can read the MR and update labels. A project access token or personal access token with API access is the simplest local setup.

To intentionally test a blocked intake result, use a test MR with an empty/template-only description:

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/ABC-123-empty-description \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- observe || true
```

The `|| true` keeps your shell script going after the expected exit `1`.

## Dry-Run Refine

Edit the mandatory TOML config:

```toml
[execution]
dry_run = true
```

Then run with the same review env:

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- refine
```

That is useful for previewing governance execution, but because `dry_run` does not post the managed summary comment to the review platform, a later live `cargo run -- explain` run will skip.

## Live Refine And Explain

If you want to test the full governance-plus-advisory flow against a live review, first run `observe` successfully, then build with `llm-local`, enable `[inference]`, turn `dry_run` off, run `refine` once, then run `explain`:

```toml
[execution]
dry_run = false
```

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- refine

CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- explain
```

`explain` rereads Milchick's managed governance summary comment and only runs the advisory pass when that latest `refine` reported applied governance effect or a blocking outcome.

## Real Slack App Thread Testing

Slack config belongs in the mandatory TOML file:

```toml
[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
base_url = "https://slack.com/api"

[notifications.slack_app.user_map]
"alice" = "U01234567"
"bob" = "U07654321"
```

The Slack app bot must be installed in the target workspace and invited to the target channel. It needs permission to post messages and read channel history so Milchick can find the existing MR root thread and reply under it. In Slack OAuth terms, that usually means `chat:write` plus the relevant history scope for the channel type you use.

Secrets stay in env. First run `observe`; it should create the root MR message and a thread reply:

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml \
GITLAB_TOKEN=your-gitlab-token \
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token \
cargo run -- observe
```

Then run `refine`; it should reuse the same Slack root thread and tag the assigned reviewers in the thread reply:

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml \
GITLAB_TOKEN=your-gitlab-token \
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token \
cargo run -- refine
```

What to check in Slack:

- one channel-level root message for the MR, containing `MR #3995`
- OBSERVE intake details appear as a thread reply
- REFINE details appear as a later thread reply
- reviewer mentions appear only in the REFINE reply

For Slack workflow testing, keep in mind that workflow delivery does not support the v5 one-root-thread contract:

```toml
[notifications.slack_workflow]
enabled = true
channel = "C0ALY38CW3X"
```

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml \
GITLAB_TOKEN=your-gitlab-token \
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/... \
cargo run -- refine
```

`explain` never sends Slack notifications.

## Config Path

Use `MR_MILCHICK_CONFIG_PATH` whenever you want a local test config that differs from the checked-in `mr-milchick.toml`. If neither the default file nor the override path contains valid v5 config, startup fails before any review work begins.

```bash
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-local.toml \
GITLAB_TOKEN=your-gitlab-token \
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token \
cargo run -- observe
```
