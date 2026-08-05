# Slack Explain Thread Replies Design

## Summary

When `mr-milchick explain` completes an advisory review, it will append the
full explain output to the existing Slack review thread. The Slack reply is a
new message on every completed explain run; Milchick will not identify, edit,
or replace earlier explain replies.

The change will ship with the existing Slack HTTP error-reporting hardening as
patch release 5.1.1.

## Goals

- Make the completed advisory review visible in the same Slack thread used by
  `observe` and `refine`.
- Include the full explain output: governance findings, planned actions, AI
  review recommendations, and closing text.
- Render Slack-native formatting rather than sending GitLab or GitHub Markdown
  unchanged.
- Preserve append-only behavior for explain replies.
- Preserve the notification runtime's dry-run and best-effort failure
  semantics.
- Release both the explain-thread feature and the existing Slack HTTP error
  diagnostics as version 5.1.1 with changelog coverage.

## Non-goals

- Updating, deduplicating, or deleting earlier Slack explain replies.
- Changing the managed GitLab or GitHub explain comment behavior.
- Posting a Slack reply when `explain` is skipped by the governance metadata
  gate.
- Making Slack delivery failure fail the advisory-review command.
- Adding another notification sink or a new Slack API.

## Architecture and Data Flow

`run_explain_mode` will continue to load the governance gate, run inference,
build one explain template context, render the platform comment, and execute
`ReviewAction::UpsertExplain`. After platform execution returns, it will render
the same context with Slack formatting and build one `NotificationMessage` for
each configured Slack sink.

The notification will use the existing review conversation key and `MR #<id>`
thread key, set `prefer_thread_reply` for Slack, and use
`NotificationDelivery::Append`. `RuntimeWiring::deliver_notifications` will
therefore resolve or create the normal review thread and call
`chat.postMessage` with its root timestamp. Because `Append` carries no managed
message metadata and does not load existing replies, every completed explain
run creates a new thread reply.

The platform action remains first. Slack notification delivery occurs only
after the platform connector returns a review report, and both platform and
notification results are printed in the existing combined execution report.

## Templates and Configuration

The default Slack explain template will render the full advisory output with
Slack-native emphasis and list formatting. `TemplateCatalog` and
`SlackAppTemplates` will gain an `explain_thread` field, exposed as the
optional `[templates.slack_app].explain_thread` TOML override. It will accept
the same explain-context placeholders as the platform explain templates and
render their values using `RenderStyle::SlackApp`.

The platform explain templates remain independent and unchanged. A custom
Slack explain template affects only the appended thread reply.

## Delivery and Failure Semantics

Live runs deliver the Slack reply when the Slack app sink is enabled and
configured. Dry-run and fixture behavior continues to follow the existing
execution strategy, including the fixture opt-in for real notification
delivery.

Slack connector errors are converted into an undelivered notification report.
They do not fail `explain` or undo the platform comment. The existing
notification policy remains in force; an explain reply is associated with the
completed explain action rather than with reviewer, label, or pipeline
mutations.

If the governance metadata gate skips `explain`, inference, platform delivery,
and Slack delivery are all skipped.

## Testing

Test-first coverage will prove that:

- the default Slack explain renderer includes findings, actions, and AI review
  recommendations in Slack formatting;
- the optional Slack template override is validated and applied;
- explain notification construction uses the existing conversation/thread
  identity and `NotificationDelivery::Append`;
- a completed explain execution includes Slack notification delivery reports;
- append delivery posts a new reply and never uses the managed update path;
- dry-run and Slack delivery failure behavior remain non-mutating and
  non-fatal; and
- existing observe/refine managed-message behavior remains unchanged.

The focused tests will be followed by formatting, the complete locked test
suite with the relevant Slack and inference features, and a release build or
check appropriate to the repository's feature matrix.

## Documentation and Release

Architecture, connector-capability, configuration-reference, and example TOML
documentation will state that `explain` appends its full advisory review to the
Slack review thread and document `templates.slack_app.explain_thread`.

`Cargo.toml` and the root package entry in `Cargo.lock` will move from 5.1.0 to
5.1.1. The 5.1.1 changelog section will include:

- an Added entry for append-only Slack explain replies;
- a Changed or Docs entry for the configurable Slack explain template and
  refreshed command semantics; and
- a Fixed entry for the existing bounded Slack HTTP/rate-limit diagnostics,
  including `Retry-After` guidance.

