# Slack Pipeline Failure DM Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace shared-thread compact commit/CI messages with one metadata-deduplicated Slack DM to the review author for each failing head commit, and release the behavior as 5.2.2.

**Architecture:** `app.rs` emits the existing durable stage notification plus an optional failure-only user notification. The Slack sink resolves the review username through `user_map`, opens the direct conversation, checks managed message metadata in that DM, and posts once per review/head commit. Existing shared-thread delivery remains responsible only for the root and durable stage replies.

**Tech Stack:** Rust, Tokio, Reqwest, Serde, Slack Web API, inline Rust unit tests, WireMock-backed integration tests.

**Spec:** `docs/superpowers/specs/2026-09-01-slack-pipeline-failure-dm-design.md`

## Global Constraints

- Release version is exactly `5.2.2`, dated `2026-09-01` in `CHANGELOG`.
- Send a DM only when at least one parsed `PipelineStatusTemplateEntry` is `Failed`.
- Deduplicate sequential retries by review conversation key plus the full head commit ID.
- Send only to `ReviewSnapshot.author.username` after successful `[notifications.slack_app.user_map]` resolution.
- Never fall back to the shared channel or another recipient when prerequisites are missing.
- Include both the review URL and head-commit URL; describe the commit as the “Change under test,” not as proven root cause.
- Preserve shared Slack roots and durable `observe`, `refine`, and `explain` replies; remove compact commit/CI activity replies.
- Do not add config flags, templates, dependencies, persistent storage, success/recovery DMs, or atomic cross-process locking.
- Use TDD: add each behavioral test, observe the expected failure, then implement the minimum production change.

---

### Task 1: Implement, document, and package failure-only direct messages

**Files:**
- Modify: `apps/mr-milchick/src/core/model.rs`
- Modify: `apps/mr-milchick/src/app.rs`
- Modify: `apps/mr-milchick/src/runtime/runtime.rs`
- Modify: `apps/mr-milchick/src/connectors/notifications/slack_app.rs`
- Modify: `tests/slack_sink.rs`
- Modify: `tests/cli_integration.rs`
- Modify: `tests/support/mock_server.rs` or the existing Slack mock module that routes Slack API requests
- Modify: `README.md`
- Modify: `docs/ci-quickstart.md`
- Modify: `docs/config-reference.md`
- Modify: `docs/connectors-and-capabilities.md`
- Modify: `docs/local-testing.md`
- Modify: `docs/message-templates.md`
- Modify: `mr-milchick.toml`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `CHANGELOG`

**Interfaces:**
- Consumes: `ReviewSnapshot.author`, `ReviewSnapshot.review_ref`, `ReviewSnapshot.metadata.commits`, `PipelineStatusTemplateEntry`, `SlackAppConfig.user_map`, existing Slack history pagination, managed `mr_milchick` metadata, `NotificationAudience::User`, and managed Slack lifecycle policy handling in `RuntimeWiring::deliver_notifications`.
- Produces: a replacement for `NotificationDelivery::AppendCommitLog` that carries `review_url: String`, `review_label: String`, `head_commit: ReviewCommit`, and `failed_checks: Vec<String>`; a Slack `conversations.open` helper; one DM per review/head; package version 5.2.2.

- [ ] **Step 1: Add failing application-level notification tests**

Add focused `app.rs` tests around `build_notifications` (or a small extracted helper) with these exact assertions:

```rust
assert_eq!(messages.iter().filter(|m| matches!(m.delivery, NotificationDelivery::DirectPipelineFailure { .. })).count(), 1);
assert!(messages.iter().all(|m| !matches!(m.delivery, NotificationDelivery::AppendCommitLog { .. })));
assert!(matches!(failure.audience, NotificationAudience::User(ref username) if username == "arthur"));
```

The failing fixture must have one passed and one failed status and a snapshot with a review URL plus at least one commit. Add separate cases proving passed-only, unknown-only, empty, governance-blocked-without-failed-status, and intake-blocked `build_observe_notifications` inputs do not build a direct failure alert. Add prerequisite cases for missing review URL and missing head commit. The observe case must also prove only its durable stage notification remains.

- [ ] **Step 2: Run the application tests and verify RED**

Run:

```bash
rtk cargo test build_notifications
```

Expected: compilation or assertion failure because the direct failure delivery mode and behavior do not exist and `AppendCommitLog` is still emitted.

- [ ] **Step 3: Define the delivery data and build only eligible alerts**

Replace `AppendCommitLog` in `NotificationDelivery` with:

```rust
DirectPipelineFailure {
    review_url: String,
    review_label: String,
    head_commit: ReviewCommit,
    failed_checks: Vec<String>,
}
```

In `build_observe_notifications`, keep the durable stage message and remove the intake-blocked `AppendCommitLog` message without creating any DM.

In `build_notifications`, keep the durable stage message for every configured sink. Do not create a commit-log message. Only for `NotificationSinkKind::SlackApp`, collect entries whose state is `PipelineStatusState::Failed`; when the list is non-empty and both the review URL and last chronological commit exist, append one `NotificationMessage` with:

```rust
audience: NotificationAudience::User(snapshot.author.username.clone()),
severity: NotificationSeverity::Critical,
prefer_thread_reply: false,
conversation_key: Some(notification_conversation_key(snapshot)),
delivery: NotificationDelivery::DirectPipelineFailure { /* exact snapshot data */ },
```

Use `MR #<id>` for GitLab and `PR #<id>` for GitHub. Do not use the general blocked predicate as the failure signal.

In `RuntimeWiring::deliver_notifications`, replace `AppendCommitLog` in `managed_slack_lifecycle` with `DirectPipelineFailure` so an eligible failure alert is not suppressed by `notification_policy = "on-applied-action"`. Update the runtime test fixture and add a test proving this delivery is attempted even when the policy would skip a normal append.

- [ ] **Step 4: Run the application tests and verify GREEN**

Run:

```bash
rtk cargo test build_notifications
rtk cargo test deliver_notifications
```

Expected: the new application-level and runtime policy tests pass.

- [ ] **Step 5: Add failing Slack integration tests**

Replace commit-log integration coverage with tests that construct `DirectPipelineFailure` notifications and assert:

```rust
assert_eq!(open_payload["users"], "U01234567");
assert!(dm_text.contains("<https://gitlab.example.com/group/project/-/merge_requests/3995|MR #3995>"));
assert!(dm_text.contains("*Change under test:*"));
assert!(dm_text.contains("<https://gitlab.example.com/commit/22222222|`22222222` Second>"));
assert!(dm_text.contains("*Failed checks:* lint"));
```

The mock `conversations.open` response must return a `D...` channel ID. Assert `chat.postMessage.channel` is that DM channel, `thread_ts` is absent, and metadata contains event type `mr_milchick`, the review conversation key, and a message key derived from the full head commit ID.

Send the same notification twice with mock DM history returning the first post; assert only one DM post. Then send a notification with a different head commit and assert a second DM post. Add unmapped and invalid-mapped author cases that return `delivered = false`, issue no `conversations.open` or `chat.postMessage`, and never use the configured shared channel.

- [ ] **Step 6: Run the Slack tests and verify RED**

Run:

```bash
rtk cargo test --test slack_sink
```

Expected: compilation or assertion failure because direct-conversation opening, DM-history deduplication, and failure-message rendering do not exist.

- [ ] **Step 7: Implement direct conversation delivery and deduplication**

Add Serde request/response DTOs for `conversations.open`. Resolve `NotificationAudience::User(username)` through `SlackAppConfig.user_map` and the existing Slack-user-ID validation. Missing/invalid mappings return a non-delivered report with a sanitized detail and no fallback.

For `DirectPipelineFailure` before the shared-thread code path:

1. POST `conversations.open` for the mapped Slack user and require an `ok` response with a DM channel ID.
2. Load that channel’s history with `include_all_metadata=true` using the existing pagination/error helpers.
3. Form the message key `pipeline-failure:<full-head-commit-id>` and call the existing managed-message lookup with the review conversation key.
4. Return `already present` without posting when found.
5. Render the exact three-line message from the spec, escaping the review label, commit title, and failed-check labels with `escape_slack_text`.
6. POST one unthreaded message to the DM channel with existing `mr_milchick` metadata and no `commit_ids` payload.

Reject `DirectPipelineFailure` with any audience other than `User` as a connector configuration error. Delete obsolete commit selection/render helpers and their unit tests after the replacement tests are green.

- [ ] **Step 8: Run the Slack and application tests and verify GREEN**

Run:

```bash
rtk cargo test --test slack_sink
rtk cargo test build_notifications
rtk cargo test deliver_notifications
```

Expected: both commands exit zero; sequential duplicate sends produce one DM and no compact activity reply appears in the shared thread.

- [ ] **Step 9: Update operator documentation**

Replace every current claim that Slack app delivery “appends compact commit/CI activity” with the new behavior: durable shared-thread replies stay in place, parsed failures create one author DM per failing head commit, and passed/unknown states create no activity message. State that author delivery requires a valid `user_map` entry and never falls back to the shared channel.

Document required scopes as `chat:write`, `metadata.message:read`, the relevant shared-channel history scope, `im:write`, and `im:history`, including the need to reinstall the Slack app after scope changes. In `mr-milchick.toml`, annotate `user_map` as required for author failure DMs.

- [ ] **Step 10: Bump and document release 5.2.2**

Change the root package version in `Cargo.toml` and the matching root package entry in `Cargo.lock` from `5.2.1` to `5.2.2`. Add this top changelog section:

```markdown
## [5.2.2] — 2026-09-01

This patch release moves actionable CI failure activity out of shared Slack review threads.

### Changed

- Slack no longer appends compact commit and passing/failing CI activity messages to shared MR/PR threads; their root and durable stage replies remain unchanged.
- A parsed CI failure now sends the review author one direct message per failing head commit, with links to the review and the change under test. Passed, unknown, unavailable, and governance-only outcomes do not send a DM.
- Author DMs require a valid `[notifications.slack_app.user_map]` entry plus Slack `im:write` and `im:history` scopes; missing mappings never fall back to the shared channel.
```

Update the CLI integration version assertion to require output beginning with `mr-milchick 5.2.2 (`.

- [ ] **Step 11: Run formatting and full verification**

Run:

```bash
rtk cargo fmt -- --check
rtk cargo test
rtk cargo run -- version
```

Expected: formatting and all tests exit zero, and the version command begins with `mr-milchick 5.2.2 (`.

- [ ] **Step 12: Self-review the bounded diff and commit**

Run:

```bash
rtk git diff --check
rtk git status --short
rtk git diff -- Cargo.toml Cargo.lock CHANGELOG README.md mr-milchick.toml apps/mr-milchick/src/core/model.rs apps/mr-milchick/src/app.rs apps/mr-milchick/src/runtime/runtime.rs apps/mr-milchick/src/connectors/notifications/slack_app.rs tests/slack_sink.rs tests/cli_integration.rs docs/ci-quickstart.md docs/config-reference.md docs/connectors-and-capabilities.md docs/local-testing.md docs/message-templates.md
```

Confirm the diff contains no success/recovery DMs, configurable templates, new storage, shared-channel fallback, or unrelated refactor. Do not stage or modify pre-existing unrelated untracked files. Commit only the implementation, tests, release files, and this spec/plan:

```bash
rtk git add Cargo.toml Cargo.lock CHANGELOG README.md mr-milchick.toml apps/mr-milchick/src/core/model.rs apps/mr-milchick/src/app.rs apps/mr-milchick/src/runtime/runtime.rs apps/mr-milchick/src/connectors/notifications/slack_app.rs tests/slack_sink.rs tests/cli_integration.rs docs/ci-quickstart.md docs/config-reference.md docs/connectors-and-capabilities.md docs/local-testing.md docs/message-templates.md docs/superpowers/specs/2026-09-01-slack-pipeline-failure-dm-design.md docs/superpowers/plans/2026-09-01-slack-pipeline-failure-dm.md
rtk git commit -m "release: send pipeline failures by Slack DM"
```
