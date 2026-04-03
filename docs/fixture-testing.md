# Fixture Testing

Fixtures still let you run Milchick without a live review platform. The difference is that notification previews and delivery now come from `mr-milchick.toml`.

## Basic Commands

```bash
cargo run -- observe --fixture fixtures/first-notification.toml
cargo run -- explain --fixture fixtures/first-notification.toml
cargo run -- refine --fixture fixtures/first-notification.toml
```

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

## Alternate Config

If you want fixture-specific notification settings or templates, point Milchick at another config file:

```bash
MR_MILCHICK_CONFIG_PATH=tests/fixture-config.toml \
cargo run -- observe --fixture fixtures/first-notification.toml
```
