# Connectors And Capabilities

This page describes the runtime surface that is actually implemented today.

## Supported Today

Mr Milchick binaries are built from exactly one review connector and zero or more notification sinks.

Implemented now:

- review connectors: GitLab, GitHub
- notification sinks: Slack app, Slack workflow
- commands: `observe`, `refine`, `explain`, `version`

## Cargo Features

The app exposes these feature flags:

```toml
[features]
default = ["gitlab", "slack-app", "slack-workflow"]
gitlab = []
github = []
slack-app = []
slack-workflow = []
teams = []
discord = []
```

Only `gitlab`, `github`, `slack-app`, and `slack-workflow` correspond to implemented code paths today.

## Common Build Shapes

Default build:

```bash
cargo build --release
```

GitLab only:

```bash
cargo build --release --no-default-features --features gitlab
```

GitLab plus Slack workflow:

```bash
cargo build --release --no-default-features --features "gitlab slack-workflow"
```

GitLab plus Slack app:

```bash
cargo build --release --no-default-features --features "gitlab slack-app"
```

GitLab plus both Slack sinks:

```bash
cargo build --release --no-default-features --features "gitlab slack-app slack-workflow"
```

GitHub plus both Slack sinks:

```bash
cargo build --release --no-default-features --features "github slack-app slack-workflow"
```

For CI release artifacts, this repo builds Linux x86_64 with musl:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

## Capability Rules

The runtime enforces these invariants:

- exactly one review connector must be enabled
- notification sinks are optional fanout only
- the flavor file cannot request a sink that was not compiled in
- the flavor file review platform must match the compiled review connector

If those rules are violated, startup fails with a configuration error instead of silently degrading.

## Flavor Alignment

The flavor file is a runtime declaration of intended compiled capabilities:

```toml
[review_platform]
kind = "gitlab"

[[notifications]]
kind = "slack-workflow"
enabled = true
```

If the binary was compiled without `slack-workflow`, the application exits with a configuration error. The same applies if the flavor file names a review platform other than the compiled connector.

## Verifying The Artifact

Use the version command in CI logs:

```bash
./mr-milchick version
```

Current output shape:

```text
mr-milchick 1.4.0 (<git-sha> <build-date>)
Compiled capabilities:
- review platform: gitlab
- notification sinks: slack-app, slack-workflow
```

GitHub build shape:

```text
mr-milchick 1.4.0 (<git-sha> <build-date>)
Compiled capabilities:
- review platform: github
- notification sinks: slack-app, slack-workflow
```

That is the fastest way to confirm the artifact you built matches the runtime environment and the docs you expect to use.

## What Is Not Implemented Yet

These names exist as reserved feature flags in the single crate, but they are not working runtime integrations today:

- Teams notification sink
- Discord notification sink

Keep future-facing roadmap language separate from the current capability docs so operators can trust the runtime surface without reading code first.
