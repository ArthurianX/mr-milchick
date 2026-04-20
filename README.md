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

Mr Milchick is a Rust CLI for GitLab merge request pipelines and GitHub pull request workflows. It runs as a single CI job, reads the active review context, evaluates review policy, plans reviewer and comment actions, and can sync the result back to the review platform with optional Slack notifications. It is a binary (that cares), not a bot and not a long-running service.

## Purpose

The tool exists to keep review governance where the decision already happens: inside CI. It turns reviewer routing, CODEOWNERS coverage, and blocking policy into deterministic pipeline behavior that stays visible in code review and easy to audit later.

## How It Works

<div align="center">
  <img src="assets/flow.gif" alt="Mr Milchick Flow" >
</div>

`observe` is the verbose deterministic inspection path: it previews the governance summary, action plan, routing details, and fixture notifications without mutating anything or invoking inference. `refine` is the fast governance path: it assigns reviewers, syncs the deterministic governance summary comment, optionally delivers Slack notifications, and fails the pipeline when blocking policy remains unresolved. `explain` is a slower advisory follow-up: it rereads the existing Milchick governance summary comment, skips itself when the last `refine` reported no governance effect and no blocking outcome, and otherwise upserts a separate advisory explain comment. `version` prints build metadata and the compiled capabilities in the artifact you are actually running.

Today the implemented surface is intentionally focused: GitLab and GitHub are the supported platform connectors, and Slack app plus Slack workflow are optional notification sinks.

One notification detail is intentionally sink-specific: Slack app update notifications try to reuse the existing thread for the same MR, while Slack workflow notifications do not do same-thread lookup and keep their workflow-driven delivery shape.

Optional local review suggestions can also be enabled when the binary is built with the `llm-local` feature and pointed at a local GGUF model. In `4.x`, that local `llama.cpp`-backed advisory pass runs only during `explain`, which keeps the normal `refine` governance flow fast and deterministic while still allowing a slower follow-up comment with structured review hints.

Internally, the repository now ships as a single crate with layered modules:

- `apps/mr-milchick/src/core`: pure policy, routing, CODEOWNERS, rendering, and tone logic
- `apps/mr-milchick/src/runtime`: execution traits, capability wiring, and reporting
- `apps/mr-milchick/src/connectors`: GitLab, GitHub, and optional Slack integrations
- `apps/mr-milchick/src/app.rs`: CLI orchestration and command flow

## Quickstart

The example below builds the binary in GitLab CI with the GitLab platform connector plus Slack app support, prints the compiled capabilities, and runs it for merge request pipelines. Start with `observe` while rolling out, then switch the review job to `refine` when you want live reviewer assignment and governance summary sync. If you later want advisory LLM commentary, add a slower follow-up `explain` job after a real `refine` run. GitHub release automation now lives in [`.github/workflows/release.yml`](.github/workflows/release.yml), and [`.github/workflows/review.yml`](.github/workflows/review.yml) uses [`mr-milchick.github.toml`](mr-milchick.github.toml) for GitHub pull request runs.

Runtime settings now live in `mr-milchick.toml`. A minimal GitLab-oriented example looks like this:

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
areas = ["frontend", "packages"]

[[reviewers.definitions]]
username = "carol"
areas = ["backend"]
```

`mr-milchick.toml` also supports Milchick-specific env interpolation before TOML parsing with `${VAR}` and `${VAR:-default}`. That is useful in CI when the file should stay authoritative but values like a local model toggle, model filename, or workspace path still come from the job environment.

```yaml
stages:
  - build
  - review

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

To make that pipeline work, store `GITLAB_TOKEN` as a CI secret. This build shape includes Slack app support, so you can enable it in `mr-milchick.toml` whenever you are ready and then provide `MR_MILCHICK_SLACK_BOT_TOKEN` as the secret input for the sink. Channel selection, Slack base URL overrides for tests, and optional user mapping now live in `mr-milchick.toml`. With the Slack app sink, follow-up update notifications try to land in the original MR thread for the same `MR #...`; Slack workflow delivery does not currently do that thread reuse. If you prefer Slack workflow delivery instead, switch the feature set and notification config intentionally. A deeper setup guide, including `mr-milchick.toml`, rollout steps, the split `refine`/`explain` model, and both Slack variants, lives in [docs/ci-quickstart.md](docs/ci-quickstart.md).

Some internal teams also use an optional convention where earlier CI jobs write compact JSON files under `*/milchick-status/*.json`, and Milchick later folds those prior-job outcomes into Slack notifications. That path is intentionally optional and documented in the configuration and template guides rather than treated as a required workflow.

For a local Linux-musl artifact, use the checked-in helper instead of calling the target directly:

```bash
./scripts/build_linux_release.sh --no-default-features --features "gitlab slack-app"
```

The helper installs the Rust target when needed, then uses the first available build path on the host:

- `x86_64-linux-musl-gcc`
- `cross`
- `cargo-zigbuild` plus `zig`

If none of those are available, it stops with a clearer toolchain error instead of the opaque `can't find crate for core` failure.
On macOS hosts it also forwards the active Xcode SDK path to bindgen, which avoids `llama-cpp-sys` header discovery failures when building `--features llm-local` for `x86_64-unknown-linux-musl`.
If you include `llm-local`, the native `llama.cpp` build also requires `cmake` on the host.

You can always fetch the latest binary, but inside sensitive infrastructures it's much better to build it directly there and use it locally.

## Local LLM Review

Mr Milchick can attach advisory local review suggestions from a GGUF model when compiled with `--features llm-local`. That inference path is now explain-only: `observe` and `refine` stay inference-free, while `explain` can add a separate advisory comment after a prior `refine` run has produced a governance summary worth following up on. The current local inference path uses each model's built-in chat template when available, falls back safely when a template is missing, and supports repeatable smoke tests plus model benchmarking.

The main runtime knobs now live under `[inference]` in `mr-milchick.toml`:

- `enabled`
- `model_path`
- `timeout_ms`
- `max_patch_bytes`
- `context_tokens`
- `trace`

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
