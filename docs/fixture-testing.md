# Fixture Testing

Fixtures still let you run Milchick without a live review platform. The difference is that notification previews and delivery now come from `mr-milchick.toml`.

## Basic Commands

```bash
cargo run -- observe --fixture fixtures/first-notification.toml
cargo run -- explain --fixture fixtures/first-notification.toml
cargo run -- refine --fixture fixtures/first-notification.toml
```

In fixture mode, `explain` does not need a previously posted platform comment. Milchick synthesizes the governance gate from the fixture outcome itself, so `explain` runs when the fixture would have applied governance actions or remained blocking and skips otherwise.

## Preview Notifications

Enable a sink in config to preview it during fixture runs:

```toml
[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
```

Then run:

```bash
cargo run -- observe --fixture fixtures/first-notification.toml
```

## Preview OBSERVE Intake Messages

For v5 OBSERVE, the Slack app sink is the useful preview target because it owns the one-root-thread MR message shape. Use a fixture-only config like this when you want to inspect the rendered root and thread replies without touching a live review platform:

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
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token \
cargo run -- refine --fixture fixtures/first-notification.toml --send-notifications
```

Slack workflow example:

```bash
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/... \
cargo run -- refine --fixture fixtures/update-notification.toml --send-notifications
```

`explain` never sends notifications, even in fixture mode.

## Alternate Config

If you want fixture-specific notification settings or templates, point Milchick at another config file:

```bash
MR_MILCHICK_CONFIG_PATH=tests/fixture-config.toml \
cargo run -- observe --fixture fixtures/first-notification.toml
```
