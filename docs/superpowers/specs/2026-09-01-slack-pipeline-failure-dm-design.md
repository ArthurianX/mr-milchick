# Slack Pipeline Failure DM Design

## Goal

Remove compact commit/CI activity messages from shared Slack review threads. When parsed CI status data contains a failure, notify only the review author by direct message, with links to the review and the current head commit.

Release the behavior as mr-milchick 5.2.2 and document the changed Slack permissions and delivery semantics.

## Scope

This change applies to the Slack app sink used by `observe` and `refine`. Existing review-thread roots and durable `observe`, `refine`, and `explain` replies remain unchanged. `observe` stops appending its intake-blocked commit line but does not send a DM because it has no parsed upstream status input. `refine` owns the new parsed-failure DM. Platform comments, reviewer assignment, labels, governance failure behavior, templates, and notification policy semantics remain unchanged.

The old `AppendCommitLog` delivery mode and its compact passing/failing commit lines are removed. No passing, unknown, unavailable, or governance-only outcome creates a replacement activity message.

## Failure signal

A direct message is eligible only when at least one `PipelineStatusTemplateEntry` has `PipelineStatusState::Failed`. `RuleOutcome::has_fail_pipeline()`, blocking findings, and `MR_MILCHICK_PIPELINE_STATE` are not sufficient because they can represent governance failures rather than a parsed upstream CI failure.

The failed-check list contains the labels of failed parsed status entries, in input order. Status details remain available in the durable stage reply and are not repeated in the DM.

## Recipient and prerequisites

The recipient is `ReviewSnapshot.author.username`, resolved through `[notifications.slack_app.user_map]`. The mapped value must be a valid Slack user ID.

If the author is unmapped, the mapped ID is invalid, the review URL is absent, or the snapshot has no head commit, the failure alert is skipped with a sanitized delivery detail or warning. It must never fall back to the shared Slack channel or another recipient.

The Slack connector opens or reuses the one-to-one conversation using `conversations.open`, then posts with `chat.postMessage`. Installations therefore require `im:write` in addition to the existing `chat:write`, `metadata.message:read`, and shared-channel history scope. Reading managed metadata from the DM for deduplication requires `im:history`.

## Message shape

The DM is concise and actionable:

```text
:red_circle: CI failed for <review-url|MR #3995>.
*Change under test:* <commit-url|`deadbeef` Commit title>
*Failed checks:* lint, unit-tests
```

GitHub reviews use `PR #...`; GitLab reviews use `MR #...`. Review and commit URLs come from the neutral snapshot. The wording says “change under test,” not “change that caused the failure,” because pipeline failure does not prove root cause. Dynamic Slack text is escaped.

## Delivery and idempotency

Add a direct, send-once delivery mode carrying the review link/label, head commit, and failed-check labels. It is valid only with `NotificationAudience::User`.

After resolving the Slack user and direct-message channel, the sink reads the DM history with full metadata. A managed message is identified by the existing review `conversation_key` plus a message key derived from the full head commit ID. If that pair already exists, delivery returns `delivered = false` with `already present`. Otherwise the sink posts one unthreaded DM with `mr_milchick` metadata.

Sequential retries for the same review and head commit therefore do not send another DM. A new failing head commit sends a new DM. A passed retry does not delete or update the earlier failure DM. As with the existing stateless Slack managed-message flow, simultaneous processes can race between history lookup and post; no new persistent store or lock is introduced.

Slack API failures remain best-effort notification failures: they are reported through the existing connector error path and do not replace the governance outcome.

## Code boundaries

- `app.rs` decides whether a failure DM notification exists and removes creation of the old commit-log notification.
- `core/model.rs` describes the new delivery data without performing Slack API work.
- `runtime/runtime.rs` treats the new failure alert as managed Slack lifecycle delivery so `notification_policy = "on-applied-action"` does not suppress an eligible alert.
- `slack_app.rs` resolves the mapped Slack user, opens the DM, performs metadata-backed deduplication, escapes/render text, and posts.
- The Slack mock and integration tests prove API shape, recipient selection, message links, failure-only behavior, and deduplication.
- Operator docs explain the behavior and new scopes. Package metadata and the changelog publish 5.2.2.

## Verification

Tests must prove:

1. Passed, unknown, missing, intake-blocked, and governance-only results do not build a direct failure alert.
2. A failed parsed status builds exactly one Slack alert addressed to the review author and no `AppendCommitLog` notification.
3. Missing URL, missing head commit, and missing/invalid user mapping skip safely without channel fallback.
4. Slack opens the mapped user’s DM, posts both links and failed-check labels, and stores managed metadata.
5. A second send for the same review/head does not post; a different failing head does.
6. Shared Slack review threads contain only their root and durable stage replies, not compact commit/CI activity lines.
7. `version` reports 5.2.2, formatting succeeds, and the full default test suite passes.

## Out of scope

- Proving which commit or file caused a CI failure.
- Sending success or recovery DMs.
- Notifying reviewers, groups, channels, or commit authors other than the review creator.
- Configurable DM templates, per-user opt-outs, throttling windows, or a persistent notification database.
- Atomic deduplication across simultaneous pipelines.
