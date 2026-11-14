# GitHub-first adoption design

Status: approved by the user on 2026-11-14. Implementation method: the model configured in OpenCode; Codex authors the spec/plan and reviews results.

## Intent

Help GitHub maintainers route reviews and enforce deterministic review policy in CI with a short, reproducible setup. The user selected GitHub users as the first audience. Preserve Milchick's single-invocation Rust CLI architecture and existing GitLab support.

## Approaches

1. **GitHub Action wrapping the CLI — proposed choice.** Simplifies consuming-repository setup without adding a service. Requires maintaining an action and trusted fork workflow.
2. **CLI-only workflow recipe.** Less wrapper code, but more installation work for every adopter.
3. **Hosted GitHub App.** Easier centralized installation but adds hosting, credentials, and a different operating model.

The user requested a local-model implementation after the Actions-first recommendation. The precise design below remains subject to review.

## First milestone

A consuming repository can pin Milchick by commit, configure its own areas/reviewers, and run `observe` followed by `refine`. GitHub gets risk/draft labels, reviewer requests, and one maintained summary comment. Nonzero policy results remain visible as failed jobs. No inference or Slack is required for this baseline.

## Constraints

- Rust edition 2024 and declared MSRV 1.85 remain unchanged.
- Compile exactly one platform connector per binary; preserve GitLab behavior.
- Core policy remains pure; network effects stay in connectors/runtime.
- No new hosted service, mandatory third-party action dependency, or mandatory inference backend.
- Never execute PR-head code, scripts, action definitions, or configuration under a write-capable token.
- Use trusted base-commit configuration, CODEOWNERS, templates, and tooling for privileged fork processing. Fetch PR metadata and changed files through GitHub APIs.
- Do not publish releases, push commits, open PRs, modify secrets, or change remote settings during delegated implementation.
- Do not read or modify existing unrelated untracked files, `.tokensave/`, credentials, or user-global OpenCode configuration.

## Delivery design

### Verification and connector behavior

Make the default GitLab suite and the GitHub-only suite pass without permanent skips. Diagnose fixture failures before production edits. Tests must cover actual outbound requests, label replacement, preserving unrelated labels, duplicate summary prevention, dry-run nonmutation, pagination, and policy failures. A repeated reviewer request may be harmless; avoid claiming stronger idempotency than verified.

CI runs separate `gitlab slack-app` and `github slack-app` feature builds on pushes and pull requests. Keep optional inference outside the minimal adoption path. Verify `llm-claude-code` compatibility separately. Do not use `--all-features` because platform features are mutually exclusive.

### Action interface

Add root `action.yml` as a composite action and `scripts/run_github_action.sh` as its focused implementation. Inputs: `command` (default `refine`; allow only `observe`, `refine`, `explain`), `config-path` (default `mr-milchick.github.toml`), and required `token`. Outputs are unnecessary for this milestone; preserve CLI exit status.

Build the GitHub binary from the pinned action source, not the consuming repository: `cargo build --locked --release --no-default-features --features github --manifest-path "$GITHUB_ACTION_PATH/Cargo.toml"`. Execute from the consuming repository's trusted workspace so local config/CODEOWNERS discovery works. Supply `GITHUB_TOKEN` and `MR_MILCHICK_CONFIG_PATH` only to the invocation. Validate inputs and quote paths; do not interpolate inputs directly into shell code. Document Linux runner/Rust-toolchain prerequisites and source-build latency honestly. Optimized release downloading/caching is deferred.

### Fork and credential boundary

Supply a GitHub-specific workflow example using `pull_request_target` and trusted base SHA checkout with `persist-credentials: false`. Pin external action dependencies to verified commit SHAs. The Milchick action reference must also be immutable; document how to select a released commit rather than inventing a SHA. Same-repository dogfood workflow may use the trusted checked-out local action.

Use least permissions needed by exercised endpoints, initially `contents: read`, `pull-requests: write`, and `issues: write` for label/comment operations; narrow only with verified evidence. Set concurrency by PR number with cancellation of stale runs. Handle opened, synchronize, reopened, edited, ready_for_review, and converted_to_draft events. Avoid label-trigger loops. Run observe/refine as separate steps; a blocking observe prevents refine. Contributor-controlled config changes take effect only after merge into the trusted base.

Existing marker lookup trusts comment content alone. Audit whether a contributor can spoof the summary marker and cause a failed update or advisory misread. If reproducible, extend the GitHub comment identity boundary to accept only comments authored by the authenticated token identity; test spoofed comments and legitimate updates. Do not treat matching text as authentication.

### Release and onboarding

Remove obsolete `slack-workflow` feature references. Test both release flavors, retain master ancestry validation, and add SHA-256 checksum assets. Release artifacts are Linux x86_64 musl; do not advertise other targets. No version bump or publication is part of implementation.

Add `docs/github-quickstart.md` and a small valid config in `examples/github/`. Explain permissions, fork trust model, dry run, reviewer eligibility, native CODEOWNERS/branch-protection responsibilities, and how to make the workflow required. Approval counts are currently absent from the GitHub snapshot: do not claim GitHub approval-count enforcement. Repair invalid README/CI quickstart area examples. Keep GitLab documentation usable.

## Acceptance and rollout

1. Default and GitHub feature test suites pass, including CLI integration and connector tests, without skips added to hide failures.
2. Both release feature combinations compile; action input/exit behavior is checked and YAML is validated.
3. The documented minimal configuration resolves in a GitHub binary.
4. A disposable GitHub consumer is used for live same-repository and fork PR tests: risk/draft labels, reviewer requests, one updated summary, reruns, dry-run, blocking policy, and trusted base configuration.
5. Build/download a release candidate and verify checksums and `version` output before describing installation as proven.

Items 4–5 requiring new external writes/publication remain an explicit later acceptance step. Implementation may be complete locally while live acceptance remains outstanding; report those states separately.

## Marketing and non-goals

Positioning hypothesis: **Deterministic reviewer routing and review policy in GitHub Actions.** First produce a usable quickstart and a public demo PR; then recruit a few pilot maintainers and measure setup friction. No promotional posting, hosted App, billing, automatic GitLab-to-GitHub mirroring, generalized CI ingestion, new inference functionality, or broad rebranding in this milestone. Naming/artwork rights remain a separate decision before a campaign.

## Delegation and review

Use OpenCode's configured `local-deepseek/qwen3.8-flash-next-gyro-gyro-m` model. Work in an isolated branch/worktree after approval, one plan task at a time. The implementer records exact checks and remaining failures in a short handoff file. Codex reviews the final diff and reruns meaningful checks; it does not silently replace the requested local implementer. No claim of quantified token savings without comparable usage evidence.
