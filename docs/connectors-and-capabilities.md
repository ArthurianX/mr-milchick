# Connectors And Capabilities

This page describes the runtime surface that is actually implemented today.

## Supported Today

Mr Milchick binaries are built from exactly one platform connector, zero or more notification sinks, and an optional advisory local-review backend.

Implemented now:

- platform connectors: GitLab, GitHub
- notification sinks: Slack app, Slack workflow
- advisory local review backend: llama.cpp via `llm-local`
- commands: `observe`, `refine`, `explain`, `version`

## Command Roles

- `observe`: verbose deterministic inspection only
- `refine`: fast governance execution plus optional notifications
- `explain`: slower advisory follow-up that may upsert the managed explain comment

Only `refine` can assign reviewers, change labels, fail the pipeline, or deliver notifications.

## Cargo Features

The app exposes these feature flags:

```toml
[features]
default = ["gitlab", "slack-app"]
gitlab = []
github = []
slack-app = []
slack-workflow = []
teams = []
discord = []
llm-local = ["dep:llama-cpp-2"]
```

Only `gitlab`, `github`, `slack-app`, `slack-workflow`, and `llm-local` correspond to implemented code paths today.

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

GitLab plus Slack workflow:

```bash
cargo build --release --no-default-features --features "gitlab slack-workflow"
```

GitLab plus Slack app:

```bash
cargo build --release --no-default-features --features "gitlab slack-app"
```

GitLab plus advisory local review:

```bash
cargo build --release --no-default-features --features "gitlab slack-app llm-local"
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
- advisory local review is optional and only used by `explain`
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
mr-milchick 4.0.0 (<git-sha> <build-date>)
Compiled capabilities:
- platform connector: gitlab
- advisory local review: not compiled
- notification sinks: slack-app
```

GitHub-only output shape:

```text
mr-milchick 4.0.0 (<git-sha> <build-date>)
Compiled capabilities:
- platform connector: github
- advisory local review: not compiled
- notification sinks: none
```

GitLab plus Slack plus local review output shape:

```text
mr-milchick 4.0.0 (<git-sha> <build-date>)
Compiled capabilities:
- platform connector: gitlab
- advisory local review: llama.cpp
- notification sinks: slack-app, slack-workflow
```

That is the fastest way to confirm the artifact you built matches the runtime environment and the docs you expect to use.

## What Is Not Implemented Yet

These names exist as reserved feature flags in the single crate, but they are not working runtime integrations today:

- Teams notification sink
- Discord notification sink

Keep future-facing roadmap language separate from the current capability docs so operators can trust the runtime surface without reading code first.
