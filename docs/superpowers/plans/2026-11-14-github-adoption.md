# GitHub-first Adoption Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. The user selected OpenCode's configured local model as implementer; do not substitute Codex subagents. Steps use checkbox syntax for tracking.

**Goal:** Deliver a locally verified GitHub Action and onboarding path around Milchick's existing CLI.

**Architecture:** Keep the connector/runtime/core boundaries. A composite action builds the pinned action source and executes the CLI against trusted consumer files; a trusted base workflow handles fork PR metadata through the API.

**Tech Stack:** Rust 2024, Cargo, Bash, GitHub Actions composite action, existing HTTP mock server.

**Spec:** `docs/superpowers/specs/2026-11-14-github-adoption-design.md`

## Global Constraints

- Rust edition 2024 and declared MSRV 1.85 remain unchanged.
- Compile exactly one platform connector per binary; preserve GitLab behavior.
- Core policy remains pure; network effects stay in connectors/runtime.
- No new hosted service, mandatory third-party action dependency, or mandatory inference backend.
- Never execute PR-head code, scripts, action definitions, or configuration under a write-capable token.
- Do not publish releases, push commits, open PRs, modify secrets, or change remote settings during delegated implementation.
- Do not read or modify existing unrelated untracked files, `.tokensave/`, credentials, or user-global OpenCode configuration.
- Prefix shell commands with `rtk`; keep reports concise and never print secrets.

## Review Focus

- Fork PR changes config or action code: privileged processing still uses trusted base files (Task 3).
- Human comment includes the summary marker: cannot redirect managed-comment updates (Task 2).
- Rerun after risk changes: replace managed risk label and preserve unrelated labels (Task 2).
- Paths with spaces or invalid action commands: correct quoting and fail before invocation (Task 3).
- Copy-pasted config/default installation: no GitLab defaults or undefined area references (Tasks 1 and 4).

## Task 1: Establish passing platform-specific suites

**Files:** `apps/mr-milchick/src/config/resolver.rs`, `tests/cli_integration.rs`, `tests/support/mock_server.rs`, `.github/workflows/test.yml`.

**Interfaces:** Preserve `resolve_config`, `github_base_env`, and existing CLI commands. Produce platform-specific fixtures and a CI feature matrix.

- [ ] Reproduce `rtk cargo test --locked` and `rtk cargo test --locked --no-default-features --features 'github slack-app'`. Record failures, not full logs.
- [ ] Run each failing test alone. In the two config tests inspect hardcoded GitLab kinds/token assertions; in five Slack CLI tests inspect environment composition and config base URLs; inspect missing GitHub label routes in the shared mock. Determine fixture versus production causes before edits.
- [ ] Add GitHub counterparts to platform-specific config assertions: GitHub config resolves GitHub token and base URL, GitLab config remains tested only in an appropriate build. Common secret tests must exercise the active platform rather than silently becoming GitLab-only.
- [ ] Correct fixture environment/config composition and mock routes at their existing seams. Verify real request method, path, and JSON body. Do not weaken assertions or permanently skip failing tests.
- [ ] Add a separate CI matrix for `gitlab slack-app` and `github slack-app` on push and pull_request with read-only permissions. Preserve current develop/master push coverage.
- [ ] Run both full suites, then `rtk cargo test --locked --no-default-features --features 'github slack-app llm-claude-code'`. All must pass.
- [ ] Commit only Task 1 files on the isolated implementation branch.

## Task 2: Lock down GitHub mutation behavior and comment ownership

**Files:** `tests/github_connector.rs`, `tests/support/mock_server.rs`, `apps/mr-milchick/src/connectors/github/{client,dto,api,mod}.rs` only where evidence requires changes; `tests/cli_integration.rs`.

**Interfaces:** Retain `PlatformConnector::load_snapshot` and `apply_review_actions`. If identity filtering is required, carry authenticated comment-author identity through GitHub DTO/domain mapping and keep it internal to that connector.

- [ ] Add failing tests for observe risk transition (low to high, unrelated label retained), label removal, unchanged summary (one POST), changed summary (PATCH existing ID), dry-run (zero mutation requests), and blocking policy (nonzero process status).
- [ ] Add mock authenticated-user and comment-author data. Test that a human-authored marker comment is ignored and a comment by the token identity is updated. Inspect existing token/auth flow to select the smallest explicit identity seam; do not hardcode `github-actions[bot]` because PAT tokens may be used.
- [ ] Run `rtk cargo test --locked --no-default-features --features 'github slack-app' --test github_connector` plus the named CLI regressions; observe expected failures before implementation.
- [ ] Implement only demonstrated missing behavior. Keep pagination tests and existing summary/explain marker separation passing.
- [ ] Run both full platform suites. Commit Task 2 files.

## Task 3: Add trusted GitHub Action execution

**Files:** create `action.yml`, `scripts/run_github_action.sh`, `tests/github_action.sh`, `examples/github/milchick.yml`; modify `.github/workflows/review.yml`.

**Interfaces:** Inputs `command` (default `refine`), `config-path` (default `mr-milchick.github.toml`), required `token`; no outputs. Wrapper environment `INPUT_COMMAND`, `INPUT_CONFIG_PATH`, `INPUT_TOKEN`, `GITHUB_ACTION_PATH`, and the normal consuming workspace.

- [ ] Write shell regression tests using a temporary fake cargo/binary harness. Assert only observe/refine/explain accepted, invalid command fails before build, Cargo manifest points inside action source, config paths with spaces survive intact, token is passed only as environment, caller working directory is preserved, and binary exit 7 becomes wrapper exit 7. Do not print the test token.
- [ ] Run `rtk proxy bash tests/github_action.sh` and confirm red before creating the wrapper.
- [ ] Implement the composite action and quoted Bash wrapper per spec. Build with `--locked --release --no-default-features --features github` from action source. Avoid inline shell interpolation of user inputs. Do not change global Rust installations in the wrapper.
- [ ] Create the consumer workflow and dogfood workflow with pull_request_target event types from spec, base SHA checkout, persist-credentials false, explicit permissions, per-PR concurrency, and separate observe/refine steps. Consumer action/dependency refs must be verified immutable SHAs; if the implementation is not published yet, use a clearly documented replacement value rather than a fabricated valid-looking SHA.
- [ ] Review every privileged shell/checkout/action path against a fork modifying action.yml, Cargo.toml, config, CODEOWNERS, or a template. No PR-head value may select executed code or trusted local policy.
- [ ] Run shell tests, `bash -n`, and actionlint if installed. Otherwise report absent tooling and use an available YAML parser for syntax; do not equate syntax parsing with actionlint validation.
- [ ] Commit Task 3 files.

## Task 4: Repair releases and the consumer quickstart

**Files:** `.github/workflows/release.yml`, `README.md`, `docs/ci-quickstart.md`, `docs/README.md`; create `docs/github-quickstart.md` and `examples/github/mr-milchick.toml`.

**Interfaces:** Retain existing binary artifact names; publish a `SHA256SUMS` asset covering binaries and example configs. No version change or publication now.

- [ ] Reproduce removed-feature failure with `rtk cargo check --locked --no-default-features --features 'github slack-app slack-workflow'`.
- [ ] Remove obsolete feature flags from both release flavors; generate SHA256SUMS only after gathering all artifacts; upload it alongside existing assets. Retain tag ancestry check and prevent recursive release-event behavior if inspection demonstrates it.
- [ ] Add valid minimal GitHub config with defined areas and inference/Slack disabled. Supply reviewer configuration examples using clearly replaceable eligible collaborator logins, and explain author exclusion/native branch protection limitations.
- [ ] Write GitHub quickstart covering immutable action pin, Linux/Rust prerequisites, config path, observe/refine, dry run, fork trust boundaries, required checks, and no approval-count enforcement claim. Repair missing/undefined areas in existing README/CI examples and link the new guide from docs hub.
- [ ] Add a CLI config smoke case in existing integration tests that loads the actual minimal example with a mocked GitHub API and dry_run=true. Assert valid resolution and zero mutations.
- [ ] Run `rtk cargo build --locked --release --no-default-features --features 'github slack-app'` and corresponding GitLab build. On a host without musl tools, do not claim musl verification; record the exact unverified release target.
- [ ] Run full platform suites, shell harness, and `rtk git diff --check`. Commit Task 4 files.

## Task 5: Handoff for independent review and live acceptance

**Files:** create `docs/github-implementation-handoff.md`.

**Interfaces:** Short Markdown report: changed files, exact verification commands/results, known limits, remaining live acceptance. Never include credentials or full logs.

- [ ] Review the complete diff against the spec and ensure no changes to unrelated user files, global config, remote state, or secrets.
- [ ] Record same-repository/fork smoke tests and release candidate download/checksum verification as pending unless actually performed under separate authorization.
- [ ] Produce the short handoff and stop. Codex reviews the diff and reruns checks before reporting local completion; fixing review findings remains assigned to OpenCode's configured local model.

## Execution handoff

After user review, create an isolated implementation worktree/branch with `codex/` prefix and copy only these approved spec/plan documents into it. Launch OpenCode explicitly with `--model local-deepseek/qwen3.8-flash-next-gyro-gyro-m`, attach these two files, and execute bounded tasks sequentially. Capture its JSON stream outside tracked files, retain session ID for follow-ups, and surface only concise results to Codex. Do not install/upgrade OpenCode or alter provider configuration.

Codex budget strategy: no duplicated implementation, no per-task Codex subagents, compact progress summaries, one independent diff review and targeted verification at the end. Savings are expected in Codex usage, not guaranteed in total tokens or wall time.
