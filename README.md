<div align="center">


# Mr Milchick

<img src="assets/mil.png" height="200" alt="Mr Milchick Logo" >

**A pleasantly unsettling steward for merge requests.**


[![Crates.io](https://img.shields.io/crates/v/mr-milchick.svg)](https://crates.io/crates/mr-milchick)
[![Documentation](https://docs.rs/mr-milchick/badge.svg)](https://docs.rs/mr-milchick)
[![License](https://img.shields.io/crates/l/mr-milchick.svg)](LICENSE)
![Crates.io Downloads (recent)](https://img.shields.io/crates/dr/mr-milchick?label=Crates%20Downloads)
    
<br />

[Documentation](docs/README.md) | [Examples](docs/ci-quickstart.md) | [Contributing](CONTRIBUTING.md)

<br />
<strong>Like this project?</strong> <a href="https://github.com/ArthurianX/mr-milchick">Star me on GitHub</a>

</div>

---

## Overview

Mr Milchick is a Rust CLI for GitLab merge request pipelines and GitHub pull request workflows. It runs as a single CI job, reads the active review context, evaluates review policy, plans reviewer and summary actions, and can sync the result back to the review platform with optional Slack notifications. It is a binary (that cares), not a bot and not a long-running service.

## Purpose

The tool exists to keep review governance where the decision already happens: inside CI. It turns reviewer routing, CODEOWNERS coverage, and blocking policy into deterministic pipeline behavior that stays visible in code review and easy to audit later.

## How It Works

<div align="center">
  <img src="assets/flow.gif" alt="Mr Milchick Flow" >
</div>

`observe` runs the planning flow without mutating anything. `refine` executes the same plan for real, including reviewer assignment, summary comment sync, optional Slack delivery, and pipeline failure when blocking policy remains unresolved. `explain` adds deeper routing and CODEOWNERS detail for debugging, while `version` prints build metadata and the compiled capabilities in the artifact you are actually running.

Today the implemented surface is intentionally focused: GitLab and GitHub are the supported platform connectors, and Slack app plus Slack workflow are optional notification sinks.

Optional local review suggestions can also be enabled when the binary is built with the `llm-local` feature and pointed at a local GGUF model. In that mode, Milchick runs a local `llama.cpp`-backed advisory pass alongside the normal review flow and adds structured review hints without introducing a hosted model dependency.

Internally, the repository now ships as a single crate with layered modules:

- `apps/mr-milchick/src/core`: pure policy, routing, CODEOWNERS, rendering, and tone logic
- `apps/mr-milchick/src/runtime`: execution traits, capability wiring, and reporting
- `apps/mr-milchick/src/connectors`: GitLab, GitHub, and optional Slack integrations
- `apps/mr-milchick/src/app.rs`: CLI orchestration and command flow

## Quickstart

The example below builds the binary in GitLab CI with the GitLab platform connector plus Slack app support, prints the compiled capabilities, and runs it for merge request pipelines. Start with `observe` while rolling out, then switch the review job to `refine` when you want live reviewer assignment. GitHub release automation now lives in [`.github/workflows/release.yml`](.github/workflows/release.yml), and [`.github/workflows/review.yml`](.github/workflows/review.yml) uses [`mr-milchick.github.toml`](mr-milchick.github.toml) for GitHub pull request runs.

```yaml
stages:
  - build
  - review

variables:
  MR_MILCHICK_REVIEWERS: >-
    [{"username":"milchick-duty","fallback":true},
     {"username":"principal-reviewer","mandatory":true},
     {"username":"alice","areas":["frontend","packages"]},
     {"username":"carol","areas":["backend"]}]

build:milchick:
  stage: build
  image: rust:1.87
  before_script:
    - rustup target add x86_64-unknown-linux-musl
    - apt-get update && apt-get install -y musl-tools pkg-config
  script:
    - cargo build --release --target x86_64-unknown-linux-musl --no-default-features --features "gitlab slack-app"
    - mkdir -p dist
    - cp target/x86_64-unknown-linux-musl/release/mr-milchick dist/
  artifacts:
    paths:
      - dist/mr-milchick

milchick:review:
  stage: review
  image: debian:bookworm-slim
  needs: ["build:milchick"]
  script:
    - ./dist/mr-milchick version
    - ./dist/mr-milchick observe
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
```

To make that pipeline work, store `GITLAB_TOKEN` as a CI secret. This build shape includes Slack app support, so you can enable it in `mr-milchick.toml` whenever you are ready and then provide `MR_MILCHICK_SLACK_BOT_TOKEN`, `MR_MILCHICK_SLACK_CHANNEL`, and optionally `MR_MILCHICK_SLACK_USER_MAP`. If you prefer Slack workflow delivery instead, switch the feature set and notification config intentionally. A deeper setup guide, including `mr-milchick.toml`, rollout steps, and both Slack variants, lives in [docs/ci-quickstart.md](docs/ci-quickstart.md).

For a local Linux-musl artifact, use the checked-in helper instead of calling the target directly:

```bash
./scripts/build_linux_release.sh --no-default-features --features "gitlab slack-app"
```

The helper installs the Rust target when needed, then uses the first available build path on the host:

- `x86_64-linux-musl-gcc`
- `cross`
- `cargo-zigbuild` plus `zig`

If none of those are available, it stops with a clearer toolchain error instead of the opaque `can't find crate for core` failure.

You can always fetch the latest binary, but inside sensitive infrastructures it's much better to build it directly there and use it locally.

## Local LLM Review

Mr Milchick can attach advisory local review suggestions from a GGUF model when compiled with `--features llm-local`. The current local inference path uses each model's built-in chat template when available, falls back safely when a template is missing, and supports repeatable smoke tests plus model benchmarking.

The main runtime knobs are:

- `MR_MILCHICK_LLM_ENABLED`
- `MR_MILCHICK_LLM_MODEL_PATH`
- `MR_MILCHICK_LLM_TIMEOUT_MS`
- `MR_MILCHICK_LLM_MAX_PATCH_BYTES`
- `MR_MILCHICK_LLM_CONTEXT_TOKENS`

The full setup, CI shape, model repo pattern, smoke testing, and benchmark workflow live in [docs/local-llm.md](docs/local-llm.md).

## Publishing

Mr Milchick now publishes as a single crates.io package. The root of the repository is the publishable crate, so the release flow is just:

```bash
cargo publish --dry-run
cargo publish
```

There is no internal multi-crate publish ordering anymore.

## Docs

The main documentation hub is [docs/README.md](docs/README.md). From there you can jump straight to:

- [CI quickstart](docs/ci-quickstart.md)
- [Configuration reference](docs/config-reference.md)
- [Reviewer routing and CODEOWNERS](docs/reviewer-routing.md)
- [Connectors and compiled capabilities](docs/connectors-and-capabilities.md)
- [Architecture](docs/architecture.md)
- [Tone and messages](docs/tone-and-messages.md)
- [Local LLM review](docs/local-llm.md)
- [Local testing](docs/local-testing.md)

## Direction

The project is moving toward stronger connector boundaries, clearer capability reporting, and deeper governance behavior without adding service infrastructure. Future connector names may already exist as reserved Cargo features, but the supported runtime surface should always be read from the current docs and the `version` command output.

## Contributing

Contributions should preserve determinism, clear architectural boundaries, and calm operational output. See [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow and coding expectations.

## Security

See [SECURITY.md](SECURITY.md) for reporting guidance and operational expectations.

## License

Released under the MIT license. See [LICENSE](LICENSE).
