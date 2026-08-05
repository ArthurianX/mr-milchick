# Connectors And Capabilities

This page describes the runtime surface that is actually implemented today.

## Supported Today

Mr Milchick binaries are built from exactly one platform connector, zero or more notification sinks, and an optional advisory local-review backend.

Implemented now:

- platform connectors: GitLab, GitHub
- notification sink: Slack app
- advisory review backends: llama.cpp via `llm-local`, or Claude Code via `llm-claude-code`
- commands: `observe`, `refine`, `explain`, `version`

## Command Roles

- `observe`: mutating intake gate for risk/draft labels, description checks, and Slack intake notifications
- `refine`: fast governance execution plus optional notifications
- `explain`: slower advisory follow-up that upserts the managed explain comment and may append a full advisory reply to the Slack MR thread

`observe` and `refine` can change labels, fail the pipeline, and deliver configured Slack notifications. Only `refine` assigns reviewers and owns the governance summary comment. `explain` appends its full advisory output as a new Slack thread reply after the platform explain comment completes. The Slack app keeps one root per review, updates the durable stage replies in place, and appends commit/CI activity beneath them.

## Cargo Features

The app exposes these feature flags:

```toml
[features]
default = ["gitlab", "slack-app"]
gitlab = []
github = []
slack-app = []
teams = []
discord = []
llm-local = ["dep:llama-cpp-2"]
llm-claude-code = []
```

Only `gitlab`, `github`, `slack-app`, `llm-local`, and `llm-claude-code` correspond to implemented code paths today.

## Common Build Shapes

Default build:

```bash
cargo build --release
```

GitHub only:

```bash
cargo build --release --no-default-features --features github
```

GitLab only:

```bash
cargo build --release --no-default-features --features gitlab
```

GitLab plus Slack app:

```bash
cargo build --release --no-default-features --features "gitlab slack-app"
```

GitLab plus advisory local review:

```bash
cargo build --release --no-default-features --features "gitlab slack-app llm-local"
```

GitLab plus Claude Code advisory review:

```bash
cargo build --release --no-default-features --features "gitlab slack-app llm-claude-code"
```

GitHub plus Slack app:

```bash
cargo build --release --no-default-features --features "github slack-app"
```

For CI release artifacts, this repo builds Linux x86_64 with musl:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

For local Linux-musl builds, prefer the repo helper so target installation and cross-toolchain checks happen in one place:

```bash
./scripts/build_linux_release.sh --no-default-features --features gitlab
```

The helper supports three local paths:

- `x86_64-linux-musl-gcc`
- `cross`
- `cargo-zigbuild` plus `zig`

When `llm-local` is included, the host also needs `cmake`. On macOS, the helper forwards the active Xcode SDK path to bindgen automatically for the local `llama.cpp` build.

## Capability Rules

The runtime enforces these invariants:

- exactly one platform connector must be compiled in
- notification sinks are optional fanout only
- advisory inference is optional and only used by `explain`
- runtime `[platform].kind` must match the compiled platform connector when it is set explicitly
- runtime notification sections can only be enabled for sinks that were compiled in

If those rules are violated, startup fails with a configuration error instead of silently degrading.

## Verifying The Artifact

Use the version command in CI logs:

```bash
./mr-milchick version
```

Default build output shape:

```text
mr-milchick 5.1.1 (<git-sha> <build-date>)
Compiled capabilities:
- platform connector: gitlab
- advisory inference: not compiled
- notification sinks: slack-app
```

GitHub-only output shape:

```text
mr-milchick 5.1.1 (<git-sha> <build-date>)
Compiled capabilities:
- platform connector: github
- advisory inference: not compiled
- notification sinks: none
```

GitLab plus Slack plus both advisory backends output shape:

```text
mr-milchick 5.1.1 (<git-sha> <build-date>)
Compiled capabilities:
- platform connector: gitlab
- advisory inference: local llama.cpp
- advisory inference: Claude Code CLI
- notification sinks: slack-app
```

That is the fastest way to confirm the artifact you built matches the runtime environment and the docs you expect to use.

## What Is Not Implemented Yet

These names exist as reserved feature flags in the single crate, but they are not working runtime integrations today:

- Teams notification sink
- Discord notification sink

Keep future-facing roadmap language separate from the current capability docs so operators can trust the runtime surface without reading code first.
