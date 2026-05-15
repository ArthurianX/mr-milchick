# Fixture Testing

Fixtures still let you run Milchick without a live review platform, but v5 still requires a real Milchick TOML config. Fixtures provide review data; TOML provides repository areas, observe rules, notification sinks, and templates.

Use the checked-in `mr-milchick.toml`, or point `MR_MILCHICK_CONFIG_PATH` at a fixture-specific config. The examples below use an explicit fixture config so the commands are self-contained.

## Required Fixture Config

```bash
cat >/tmp/milchick-fixture.toml <<'TOML'
[platform]
kind = "gitlab"

[[areas.definitions]]
key = "frontend"
paths = ["apps/frontend/**"]
risk = "medium"

[[areas.definitions]]
key = "docs"
paths = ["docs/**", "README.md"]
risk = "low"

[observe.description]
required = false

[reviewers]
max_reviewers = 2

[[reviewers.definitions]]
username = "principal-reviewer"
mandatory = true

[[reviewers.definitions]]
username = "bob"
areas = ["frontend"]
TOML
```

## Basic Commands

```bash
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-fixture.toml \
cargo run -- observe --fixture fixtures/first-notification.toml

MR_MILCHICK_CONFIG_PATH=/tmp/milchick-fixture.toml \
cargo run -- explain --fixture fixtures/first-notification.toml

MR_MILCHICK_CONFIG_PATH=/tmp/milchick-fixture.toml \
cargo run -- refine --fixture fixtures/first-notification.toml
```

In fixture mode, `explain` does not need a previously posted platform comment. Milchick synthesizes the governance gate from the fixture outcome itself, so `explain` runs when the fixture would have applied governance actions or remained blocking and skips otherwise.

## Preview Notifications

Enable a sink in the fixture config to preview it during fixture runs. This is a full TOML config, not just a Slack snippet:

```bash
cat >/tmp/milchick-fixture-slack.toml <<'TOML'
[platform]
kind = "gitlab"

[[areas.definitions]]
key = "frontend"
paths = ["apps/frontend/**"]
risk = "medium"

[observe.description]
required = false

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
TOML
```

Then run:

```bash
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-fixture-slack.toml \
cargo run -- observe --fixture fixtures/first-notification.toml
```

## Preview OBSERVE Intake Messages

For v5 OBSERVE, the Slack app sink is the useful preview target because it owns the one-root-thread MR message shape. Use this complete fixture-only config when you want to inspect the rendered root and thread replies without touching a live review platform:

```bash
cat >/tmp/milchick-observe-fixture.toml <<'TOML'
[platform]
kind = "gitlab"

[[areas.definitions]]
key = "frontend"
paths = ["apps/frontend/**"]
risk = "medium"

[[areas.definitions]]
key = "docs"
paths = ["docs/**", "README.md"]
risk = "low"

[observe.description]
required = true
template_paths = ["/tmp/milchick-mr-template.md"]
ignore_branch_issue_key = true

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"

[templates.slack_app]
thread_root = "ROOT {{thread_key}} {{mr_title}}"
observe_thread = "OBSERVE {{observe_status}} risk={{risk_level}} label={{risk_label}}\n{{risk_reasons_block}}\n{{next_step}}"
observe_blocked_thread = "BLOCKED {{observe_status}} description={{description_status}} risk={{risk_level}}\n{{blocking_reasons_block}}\n{{unmatched_paths_block}}\n{{next_step}}"
observe_draft_thread = "DRAFT {{observe_status}} risk={{risk_level}} label={{risk_label}}\n{{next_step}}"
refine_thread = "REFINE {{mr_ref}} {{reviewers_line}}\n{{actions_block}}"
TOML

cat >/tmp/milchick-mr-template.md <<'MD'
Describe the change
MD
```

Then run each OBSERVE message shape:

```bash
# Passed intake: meaningful description, mapped frontend path.
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-observe-fixture.toml \
cargo run -- observe --fixture fixtures/first-notification.toml

# Blocked intake: no meaningful description.
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-observe-fixture.toml \
cargo run -- observe --fixture fixtures/blocking-refine.toml

# Draft intake: adds the draft label, previews the draft thread, exits 0.
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-observe-fixture.toml \
cargo run -- observe --fixture fixtures/draft-observe.toml

# High-risk blocked intake: changed path is not covered by configured areas.
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-observe-fixture.toml \
cargo run -- observe --fixture fixtures/high-risk-observe.toml

# Template-only blocked intake: description matches the configured template.
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-observe-fixture.toml \
cargo run -- observe --fixture fixtures/template-only-observe.toml
```

Fixture OBSERVE uses dry-run execution, so these commands print the planned label mutations and notification previews instead of changing a live MR. Blocked examples still exit nonzero; append `|| true` if you are running all previews from one shell script.

## Send Fixture Notifications

Fixture delivery still requires `--send-notifications`.

Slack app example:

```bash
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-fixture-slack.toml \
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token \
cargo run -- refine --fixture fixtures/first-notification.toml --send-notifications
```

Slack workflow example:

```bash
cat >/tmp/milchick-fixture-workflow.toml <<'TOML'
[platform]
kind = "gitlab"

[[areas.definitions]]
key = "frontend"
paths = ["apps/frontend/**"]
risk = "medium"

[observe.description]
required = false

[notifications.slack_workflow]
enabled = true
channel = "C0ALY38CW3X"
TOML

MR_MILCHICK_CONFIG_PATH=/tmp/milchick-fixture-workflow.toml \
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/... \
cargo run -- refine --fixture fixtures/update-notification.toml --send-notifications
```

`explain` never sends notifications, even in fixture mode.

## Config Path

`MR_MILCHICK_CONFIG_PATH` is how you select the mandatory TOML config for a fixture run:

```bash
MR_MILCHICK_CONFIG_PATH=/tmp/milchick-fixture.toml \
cargo run -- observe --fixture fixtures/first-notification.toml
```
