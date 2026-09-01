# Slack Latest Explain Review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep exactly one current `explain` review reply in each Slack MR thread, update it after every completed explain run, and label it with a viewer-local latest-review timestamp.

**Architecture:** Leave the `explain` gate, inference execution, managed GitLab/GitHub explain comment, and Slack best-effort semantics unchanged. Replace the Slack notification's append-only delivery with the existing durable `UpsertThreadReply` mechanism using slot `explain`, and append a Slack `<!date^...>` footer to the Slack-only rendered body before delivery.

**Tech Stack:** Rust 2024, Tokio, Slack Web API message metadata and `chat.update`, Cargo unit/integration tests, Markdown documentation.

**Spec:** Approved bounded design in the 2026-09-01 conversation; no separate design document was required.

## Global Constraints

- Work directly in the current checkout and preserve unrelated untracked `.tokensave/` and `docs/superpowers/specs/2026-08-07-ms-cobel-design.md` content.
- Run every shell command through `rtk` as required by `AGENTS.md`.
- Preserve the current `explain` scheduling and governance-metadata gate; do not add daily throttling, discussion APIs, commit tracking, or new storage.
- Preserve the GitLab/GitHub managed explain-comment behavior.
- Preserve Slack dry-run behavior and best-effort failure reporting; Slack delivery failure must not fail `explain`.
- Use the existing review `conversation_key`, `MR #<id>` thread key, and `NotificationDelivery::UpsertThreadReply { slot: "explain" }`.
- Add this Slack-only footer at the bottom of every completed review: `_The Board has reviewed the refinements. Latest review: <!date^<unix-seconds>^{date_long_pretty} at {time}|latest review time>._`
- The Slack date token must use the current Unix timestamp so Slack renders the date and time in each reader's locale.
- Do not attempt to modify or adopt historical append-only explain replies. For an existing MR, the first run after upgrade creates the durable `explain` reply; later runs update it.
- Release the change as version `5.2.1` dated `2026-09-01` and document it in the root `CHANGELOG`.
- Do not push, publish, tag, or create a release.

---

### Task 1: Upsert the Current Slack Explain Review and Package 5.2.1

**Files:**
- Modify: `apps/mr-milchick/src/app.rs`
- Modify: `tests/cli_integration.rs`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `CHANGELOG`
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/connectors-and-capabilities.md`
- Modify: `docs/config-reference.md`
- Modify: `docs/local-testing.md`
- Modify: `docs/fixture-testing.md`

**Interfaces:**
- Consumes: existing `NotificationDelivery::UpsertThreadReply { slot: String }`, Slack managed-message metadata, `render_slack_app_explain`, and the existing explain execution flow.
- Produces: `fn append_latest_review_footer(body: &str, reviewed_at_epoch_seconds: u64) -> String`, plus `build_explain_notifications` output that uses durable slot `explain` and contains the Slack-native latest-review footer.

- [ ] **Step 1: Complete the failing application-level tests**

The checkout already contains a red test named `slack_explain_notification_updates_the_latest_review_thread_slot` in `apps/mr-milchick/src/app.rs`. Keep its assertions that the delivery is `UpsertThreadReply` with slot `explain`, the body includes `The Board has reviewed the refinements. Latest review: <!date^`, and the body ends with `^{date_long_pretty} at {time}|latest review time>._`.

Add a second deterministic unit test beside it for the footer helper:

```rust
#[test]
fn latest_review_footer_uses_a_slack_viewer_local_timestamp() {
    assert_eq!(
        append_latest_review_footer("Advisory review", 1_788_220_800),
        "Advisory review\n\n_The Board has reviewed the refinements. Latest review: <!date^1788220800^{date_long_pretty} at {time}|latest review time>._"
    );
}
```

This test catches a missing footer, a non-Slack timestamp, the wrong timestamp value, or placement anywhere other than the bottom of the body.

- [ ] **Step 2: Extend the CLI integration test to prove create-then-update behavior**

Change `explain_mode_fixture_sends_slack_advisory_reply` in `tests/cli_integration.rs` so it invokes the same fixture explain command twice against the same `MockGitLabServer` and configuration.

After both successful runs, assert these literal boundary effects:

```rust
assert_eq!(server.request_count("POST", "/slack/api/chat.postMessage"), 2);
assert_eq!(server.request_count("POST", "/slack/api/chat.update"), 1);
```

The two posts are the existing MR root plus the first durable explain reply; the second explain run must update rather than append. Parse the first reply payload and the update payload, then assert both contain:

```rust
assert_eq!(
    payload["metadata"]["event_payload"]["message_key"],
    serde_json::json!("slot:explain")
);
let text = payload["text"].as_str().expect("Slack review text should be a string");
assert!(text.contains("Mr. Milchick Advisory Explain"));
assert!(text.contains("The Board has reviewed the refinements. Latest review: <!date^"));
assert!(text.ends_with("^{date_long_pretty} at {time}|latest review time>._"));
```

Keep the existing thread timestamp assertion for the first reply. Do not change the generic append-only sink test: `NotificationDelivery::Append` remains a supported delivery mode even though explain will no longer select it.

- [ ] **Step 3: Run the focused tests and confirm the RED state**

Run:

```bash
rtk cargo test slack_explain_notification_updates_the_latest_review_thread_slot --locked
rtk cargo test latest_review_footer_uses_a_slack_viewer_local_timestamp --locked
rtk cargo test explain_mode_fixture_sends_slack_advisory_reply --locked
```

Expected before production implementation: the existing application test fails because explain still selects `Append`; the new helper test fails to compile until the helper exists; the integration test fails because the second explain creates another Slack reply instead of calling `chat.update`.

- [ ] **Step 4: Implement the minimal Slack-only rendering and delivery change**

In `apps/mr-milchick/src/app.rs`, expand the time import:

```rust
use std::time::{Duration, SystemTime, UNIX_EPOCH};
```

Add this focused helper near `build_explain_notifications`:

```rust
fn append_latest_review_footer(body: &str, reviewed_at_epoch_seconds: u64) -> String {
    format!(
        "{}\n\n_The Board has reviewed the refinements. Latest review: <!date^{}^{{date_long_pretty}} at {{time}}|latest review time>._",
        body.trim_end(),
        reviewed_at_epoch_seconds,
    )
}
```

At the start of `build_explain_notifications`, calculate the review timestamp once so every configured Slack message from the same run uses the same value:

```rust
let reviewed_at_epoch_seconds = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs();
```

Replace the notification body with:

```rust
body: append_latest_review_footer(
    &render_slack_app_explain(template_catalog, context),
    reviewed_at_epoch_seconds,
),
```

Replace `NotificationDelivery::Append` with:

```rust
NotificationDelivery::UpsertThreadReply {
    slot: "explain".to_string(),
}
```

Do not change `run_explain_mode`, the Slack connector, the template schema, or the platform explain rendering.

- [ ] **Step 5: Run the focused tests and confirm GREEN**

Run:

```bash
rtk cargo test slack_explain_notification_updates_the_latest_review_thread_slot --locked
rtk cargo test latest_review_footer_uses_a_slack_viewer_local_timestamp --locked
rtk cargo test explain_mode_fixture_sends_slack_advisory_reply --locked
```

Expected: all focused tests pass. The integration test observes exactly two `chat.postMessage` calls and one `chat.update` call across two explain runs.

- [ ] **Step 6: Update operator documentation without rewriting historical design records**

Update current documentation to say that Slack keeps one durable explain reply per MR thread and updates it with the latest completed review. Mention that its footer carries a Slack viewer-local date/time and that historical pre-5.2.1 append-only replies are left untouched.

Make these exact semantic replacements:

- `README.md`: replace the overview statement that explain “appends the full advisory review” with wording that it “creates or updates one current advisory review reply”.
- `docs/architecture.md`: describe the platform explain-comment upsert followed by the Slack durable-reply upsert.
- `docs/connectors-and-capabilities.md`: replace both append/new-reply claims with one-current-reply behavior.
- `docs/config-reference.md`: update `notification_policy`, `templates.slack_app.explain_thread`, and Notes wording. Remove the stale claim that `explain` does not notify Slack.
- `docs/local-testing.md`: say repeated explain runs update the existing managed reply.
- `docs/fixture-testing.md`: replace “append-only Slack advisory reply” with “durable Slack advisory reply”.

Do not edit `docs/superpowers/specs/2026-08-05-slack-explain-thread-replies-design.md` or `docs/superpowers/plans/2026-08-05-slack-explain-thread-replies.md`; they are historical records of the 5.1.1 design.

- [ ] **Step 7: Add the 5.2.1 changelog and bump package metadata**

Insert this release at the top of `CHANGELOG`, before 5.2.0:

```markdown
## [5.2.1] — 2026-09-01

This patch release keeps advisory Slack threads current without adding a new review reply after every explain run.

### Changed

- `explain` now creates one managed advisory reply per Slack MR thread and updates it in place after later completed reviews, while historical append-only replies remain untouched.
- The current Slack advisory reply ends with a viewer-local latest-review date and time so readers can tell when its contents were refreshed.
```

Change the root package version from `5.2.0` to `5.2.1` in `Cargo.toml` and the `mr-milchick` package entry in `Cargo.lock`. Rename `version_command_reports_5_2_0` to `version_command_reports_5_2_1` in `tests/cli_integration.rs` and change its expected prefix to `mr-milchick 5.2.1 (`.

- [ ] **Step 8: Format and run the full verification matrix**

Run:

```bash
rtk cargo fmt
rtk cargo fmt --check
rtk cargo test --workspace --locked
rtk cargo check --locked --no-default-features --features "gitlab slack-app llm-local llm-claude-code"
rtk cargo check --locked --no-default-features --features "github slack-app llm-local llm-claude-code"
rtk cargo run --locked -- version
rtk git diff --check
rtk git status --short
```

Expected: formatting passes; the complete default test suite passes; both platform feature combinations compile with Slack and both inference backends; version output begins `mr-milchick 5.2.1 (`; `git diff --check` reports no whitespace errors; only the planned files plus the pre-existing unrelated untracked paths appear in status.

- [ ] **Step 9: Review and commit the bounded release change**

Review:

```bash
rtk git diff -- apps/mr-milchick/src/app.rs tests/cli_integration.rs Cargo.toml Cargo.lock CHANGELOG README.md docs/architecture.md docs/connectors-and-capabilities.md docs/config-reference.md docs/local-testing.md docs/fixture-testing.md
```

Confirm the diff contains no daily throttling, discussion handling, new persistent state, Slack connector changes, or edits to historical 5.1.1 design/plan documents.

Commit only the planned files:

```bash
rtk git add apps/mr-milchick/src/app.rs tests/cli_integration.rs Cargo.toml Cargo.lock CHANGELOG README.md docs/architecture.md docs/connectors-and-capabilities.md docs/config-reference.md docs/local-testing.md docs/fixture-testing.md docs/superpowers/plans/2026-09-01-slack-latest-explain-review.md
rtk git commit -m "fix: keep Slack explain review current"
```

Do not add `.tokensave/` or `docs/superpowers/specs/2026-08-07-ms-cobel-design.md`.
