# GitHub adoption implementation handoff

## Status

Local implementation on `codex/github-adoption` is complete and locally verified. One independent full-branch review identified material defects; the local model corrected them and the coordinator verified the regressions. Nothing has been pushed or published by this implementation run.

## Implementation method

The user-selected Qwen model configured in OpenCode wrote the product changes. OpenCode first reproduced the failures, then direct requests to the same local model replaced a slow exploratory tool loop. The coordinator supplied focused review feedback, applied only validated model edits, formatted code, ran checks, and arranged one independent full-branch review. Global OpenCode/provider configuration was not changed.

## Delivered surface

- GitLab and GitHub feature-specific CI and repaired platform-aware fixtures.
- GitHub managed-comment ownership, standard Actions-token/PAT identity handling, null/deleted-author tolerance, label encoding, and rerun tests.
- Composite Action, source-build wrapper, shell regression harness, trusted base-SHA fork workflows, and a minimal consumer configuration.
- Release feature repair, pre-publication asset verification, SHA-256 manifest, and GitHub quickstart.

## Verification

- Default GitLab suite: 190 passed.
- GitHub + Slack suite: 204 passed.
- GitHub + Slack + Claude Code capability suite: 204 passed.
- Action shell harness passed, including source-directory build, consumer-directory execution, token isolation, spaced paths, invalid commands, and exit status.
- `actionlint` passed for CI, review, release, and consumer-example workflows; Bash syntax and whitespace checks passed.
- Native macOS release builds passed for GitHub-only, GitHub/Slack, and GitLab/Slack. Linux-musl builds are not established by these checks.
- Spoofed-comment and null-author regressions failed against the old implementation, then passed with the fix. Stricter label-path and build-directory tests likewise failed before their fixes.
- Parallel fixture collision was reproduced: the old clock-only helper generated 5,758 unique paths out of 16,000; the atomic-counter/PID helper passed the regression and full suite.

## Acceptance still required

- Execute the workflow on real same-repository and fork PRs in a disposable consumer repository, checking labels, reviewer requests, summary reruns, drafts, policy failure, and trusted base configuration.
- Run Linux x86_64-musl release jobs and verify downloaded binaries/checksums. Native macOS builds do not prove Linux artifacts.
- Select an immutable action commit after integration; the documented replacement pin is intentionally not a fabricated release.
- Choose integration/publication explicitly. No version bump, release publication, or external acceptance writes were performed.

## Rulings and limitations

- **Same model, narrower execution:** direct local-model patches replaced the OpenCode agent loop to reduce repeated exploration. This gives the coordinator more responsibility for context selection and verification; local generation is not evidence of correctness.
- **One platform per binary:** retain the existing mutually exclusive connector features. Testing uses separate Cargo invocations; `--all-features` is not a valid gate.
- **Actions/PAT authentication:** standard Actions `GITHUB_TOKEN` and PATs are the specified path. Custom GitHub App tokens have not been accepted as supported; adopters requiring them need a separate identity design/test.
- **External acceptance deferred:** actual GitHub/fork behavior, Linux-musl execution, and published-download verification remain outstanding. Do not describe this as proven production onboarding before those checks.
- **Existing policy scope preserved:** broad branch-policy/reviewer-routing behavior is not expanded. GitHub approval counts remain absent; branch protection/rulesets must require the job check for enforcement.
- **Stronger guide requirements:** explicit required-check setup, observe-then-refine ordering, reviewer eligibility, and pinned dependencies are treated as functional onboarding requirements rather than cosmetic polish.
- **Build source boundary:** Cargo runs in the pinned Action source directory, while the CLI runs in the trusted consuming workspace, to avoid consumer Cargo configuration/toolchain overrides interfering with the Action build.

## Independent review disposition

- Fixed malformed `/labels//...` removal URL; tightened mock matching and asserted exact real encoded paths.
- Added both connector feature sets to the release test gate.
- Added the shell harness to CI.
- Added Action source-directory build isolation and its regression.
- Corrected pinned-toolchain guidance, observe-then-refine order, required-check instructions, and author-exclusion/eligibility guidance.

## Deferred minor

The independent reviewer noted comment-only cleanup in `connectors/github/mod.rs`; it is deferred. No behavior depends on that cleanup.

## Token accounting

Local inference moved generation to the configured LAN endpoint. Several corrective passes and independent review were necessary. Completed-response usage is a lower bound because exploratory OpenCode turns and interrupted responses add usage. There is no comparable Codex-only run, so net token savings are not established.
