# Connector Compilation Guidelines

Mr. Milchick uses compile-time connector selection.

A given binary must contain:

- exactly 1 review connector
- zero or more notification sinks

Today that means:

- review connector: `gitlab`
- notification sinks: `slack-app`, `slack-workflow`

`github`, `teams`, and `discord` remain reserved feature names for future connectors and sinks.

## Core Rule

Review reads and writes always go through the same compiled review connector.

Notification sinks are optional fanout only. They never change planning logic.

## Feature Model

The app crate exposes connector features through [`apps/mr-milchick/Cargo.toml`](apps/mr-milchick/Cargo.toml).

Default build:

```bash
cargo build -p mr-milchick
```

Current default feature set:

- `gitlab`
- `slack-app`
- `slack-workflow`

Minimal GitLab-only build:

```bash
cargo build -p mr-milchick --no-default-features --features gitlab
```

GitLab plus Slack app:

```bash
cargo build -p mr-milchick --no-default-features --features "gitlab slack-app"
```

GitLab plus Slack workflow:

```bash
cargo build -p mr-milchick --no-default-features --features "gitlab slack-workflow"
```

GitLab plus both Slack sinks:

```bash
cargo build -p mr-milchick --no-default-features --features "gitlab slack-app slack-workflow"
```

## Invariants

- exactly one review connector feature must be enabled
- zero notification sinks is valid
- the flavor file must not request a sink that is not compiled in
- the flavor file review platform must match the compiled review connector

The app validates these invariants during startup in [`apps/mr-milchick/src/app.rs`](apps/mr-milchick/src/app.rs).

## Flavor Alignment

The flavor file declares intended compiled capabilities.

Example:

```toml
[review_platform]
kind = "gitlab"

[[notifications]]
kind = "slack-workflow"
enabled = true
```

If the binary was compiled without `slack-workflow`, startup fails with a configuration error instead of silently degrading.

## Recommended Build Strategy

Use one of these approaches:

1. Build a single general-purpose binary with all currently supported sinks enabled.
2. Build separate binaries for different deployment environments, such as:
   - GitLab-only
   - GitLab plus Slack app
   - GitLab plus Slack workflow

Separate binaries are usually easier to reason about in regulated environments because the compiled capability set is obvious from the build job.

## Target Environment Matters

Build the binary for the environment where it will actually run.

This matters just as much as connector selection. A binary compiled for a local macOS workstation is not an appropriate release artifact if the real runtime environment is a Linux x86_64 GitLab runner.

For release artifacts, prefer an explicit target triple rather than relying on the builder host defaults.

Example:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build -p mr-milchick --release --target x86_64-unknown-linux-musl
```

Why `x86_64-unknown-linux-musl` is a good release target:

- it matches common Linux x86_64 deployment environments
- it produces a more portable static binary than a host-default glibc build in many CI setups
- it makes the intended runtime platform explicit in the build job

This is the same approach used in the release pipeline in [`/.gitlab-ci.yml`](/Users/arthur.kovacs/Work/mr-milchick/.gitlab-ci.yml).

## Verification

Use the version command to confirm what a binary contains:

```bash
cargo run -p mr-milchick -- version
```

Mr. Milchick prints the compiled review platform and notification sinks at startup, which makes feature mismatches easier to spot in CI logs.
