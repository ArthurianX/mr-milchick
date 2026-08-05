# Slack Explain Thread Replies Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make each completed `mr-milchick explain` append a full Slack-native advisory review to the MR thread, and release it with the existing Slack HTTP diagnostic hardening as 5.1.1.

**Architecture:** Add a Slack-specific explain template that reuses the existing `SummaryTemplateContext`, then construct one append-only Slack `NotificationMessage` after the platform explain action. Reuse the notification runtime and Slack sink unchanged for delivery, so thread lookup, dry-run behavior, best-effort failure reporting, and no-update semantics stay centralized.

**Tech Stack:** Rust 2024, Tokio, Clap, serde/TOML, reqwest Slack Web API, Cargo integration tests.

## Global Constraints

- Leave the existing `slack_app.rs` HTTP/rate-limit diagnostic change intact and include it in the 5.1.1 release.
- Post the Slack reply only after the explain platform action returns a report.
- Use `NotificationDelivery::Append`; do not attach managed-message metadata or call `chat.update` for explain replies.
- Keep Slack notification errors best-effort: report them without failing `explain`.
- Keep dry-run non-mutating; fixture delivery requires `--send-notifications`.
- Use `rtk` before every shell command.
- Preserve the existing GitLab/GitHub managed explain comments and all observe/refine Slack lifecycle behavior.

---

## File Structure

- `apps/mr-milchick/src/config/schema.rs`: expose the optional TOML `templates.slack_app.explain_thread` field.
- `apps/mr-milchick/src/core/message_templates.rs`: define, validate, resolve, and render the Slack-specific explain template.
- `apps/mr-milchick/src/cli.rs`: allow fixture explain runs to opt into real notification delivery.
- `apps/mr-milchick/src/app.rs`: construct and deliver append-only Slack explain notifications after the platform action.
- `tests/slack_sink.rs`: prove repeated append replies never use the managed update endpoint.
- `tests/cli_integration.rs`: exercise `explain --fixture --send-notifications` against the Slack mock.
- `mr-milchick.toml`, `docs/config-reference.md`, `docs/architecture.md`, `docs/connectors-and-capabilities.md`: document the new Slack explain reply and template override.
- `Cargo.toml`, `Cargo.lock`, `CHANGELOG`: publish version 5.1.1 and document both release changes.

### Task 1: Add a Slack Explain Template and Its Configuration Boundary

**Files:**

- Modify: `apps/mr-milchick/src/config/schema.rs:313-337`
- Modify: `apps/mr-milchick/src/core/message_templates.rs:1-75,235-245,350-485,650-675,1620-1820`

**Interfaces:**

- Consumes: `SummaryTemplateContext`, `RenderStyle::SlackApp`, and the existing review-comment placeholder set.
- Produces: `render_slack_app_explain(catalog: &TemplateCatalog, context: &SummaryTemplateContext) -> String` and `templates.slack_app.explain_thread`.

- [ ] **Step 1: Write the failing renderer and override tests**

Add these tests in `core/message_templates.rs` next to `renders_default_gitlab_explain_template` and the existing partial-override tests:

```rust
#[test]
fn renders_default_slack_explain_template() {
    let rendered = render_slack_app_explain(
        &TemplateCatalog::default(),
        &build_explain_template_context(
            &RuleOutcome::new(),
            &sample_snapshot(),
            &sample_inference_outcome(),
            &ToneSelector::default(),
            &sample_context(),
        ),
    );

    assert!(rendered.starts_with("*Mr. Milchick Advisory Explain*"));
    assert!(rendered.contains("AI review recommendations"));
    assert!(rendered.contains("Check that first-time reviewer notifications"));
    assert!(!rendered.contains("## Mr. Milchick Advisory Explain"));
}

#[test]
fn applies_slack_explain_thread_template_override() {
    let catalog = resolve_template_catalog(&TemplatesConfig {
        slack_app: SlackAppTemplates {
            explain_thread: Some("*{{summary_title}}* {{recommendations_count}}".to_string()),
            ..SlackAppTemplates::default()
        },
        ..TemplatesConfig::default()
    });

    let rendered = render_slack_app_explain(
        &catalog,
        &build_explain_template_context(
            &RuleOutcome::new(),
            &sample_snapshot(),
            &sample_inference_outcome(),
            &ToneSelector::default(),
            &sample_context(),
        ),
    );

    assert_eq!(rendered, "*Mr. Milchick Advisory Explain* 2");
}
```

- [ ] **Step 2: Run the focused tests to verify the intended failure**

Run:

```bash
rtk cargo test renders_default_slack_explain_template --locked
rtk cargo test applies_slack_explain_thread_template_override --locked
```

Expected: compilation fails because `render_slack_app_explain` and `SlackAppTemplates::explain_thread` do not exist.

- [ ] **Step 3: Implement the smallest reusable Slack explain rendering path**

In `message_templates.rs`, add this Slack-native default next to the other Slack template constants:

```rust
const SLACK_APP_EXPLAIN_THREAD_TEMPLATE: &str = r#"*{{summary_title}}*
Merge request: {{mr_link}}

{{summary_intro}}

{{findings_block}}

{{actions_block}}

{{recommendations_block}}

_{{closing_tone_message}}_"#;
```

Add `explain_thread: String` to `SlackAppTemplateCatalog`, initialize it from that constant in `TemplateCatalog::default`, and add `explain_thread: Option<String>` to `schema::SlackAppTemplates`.

Extend `TemplateField` with `SlackAppExplainThread`. Its config path must be `templates.slack_app.explain_thread`, and its allowed placeholders must be `GITLAB_SUMMARY_PLACEHOLDERS`, because it is rendered from `SummaryTemplateContext` rather than `NotificationTemplateContext`. Resolve the optional override alongside the other Slack overrides:

```rust
apply_template_override(
    &mut catalog.slack_app.explain_thread,
    templates.slack_app.explain_thread.as_deref(),
    TemplateField::SlackAppExplainThread,
);
```

Add the renderer alongside `render_review_explain`:

```rust
pub fn render_slack_app_explain(
    catalog: &TemplateCatalog,
    context: &SummaryTemplateContext,
) -> String {
    render_template(
        &catalog.slack_app.explain_thread,
        &context.variables(RenderStyle::SlackApp, true),
    )
}
```

- [ ] **Step 4: Run the focused tests to verify they pass**

Run:

```bash
rtk cargo test core::message_templates --locked
```

Expected: all template tests pass, including the default Slack formatting and the override test.

- [ ] **Step 5: Commit the template boundary**

```bash
rtk git add apps/mr-milchick/src/config/schema.rs apps/mr-milchick/src/core/message_templates.rs
rtk git commit -m "feat: add Slack explain template"
```

### Task 2: Deliver Explain Output as an Append-Only Slack Thread Reply

**Files:**

- Modify: `apps/mr-milchick/src/cli.rs:60-105`
- Modify: `apps/mr-milchick/src/app.rs:43-52,373-381,642-708,1160-1340,1797-2220`
- Modify: `apps/mr-milchick/src/runtime/runtime.rs:1-205`
- Modify: `tests/slack_sink.rs:1-360`
- Modify: `tests/cli_integration.rs:878-1140`

**Interfaces:**

- Consumes: `render_slack_app_explain`, configured `NotificationSinkKind`s, the existing `notification_conversation_key`, and `NotificationDelivery::Append`.
- Produces: `build_explain_notifications(...) -> Vec<NotificationMessage>` and `explain --fixture ... --send-notifications` delivery coverage.

- [ ] **Step 1: Write failing notification-construction and append-delivery tests**

In `app.rs` tests, add a focused test that builds the explain context and asserts the one configured Slack notification is append-only:

```rust
#[test]
fn slack_explain_notification_appends_to_the_review_thread() {
    let snapshot = sample_snapshot(Vec::new());
    let notifications = build_explain_notifications(
        &snapshot,
        &TemplateCatalog::default(),
        &build_explain_template_context(
            &RuleOutcome::new(),
            &snapshot,
            &ReviewInferenceOutcome::ready(Default::default()),
            &ToneSelector::default(),
            &sample_context(),
        ),
        &[NotificationSinkKind::SlackApp],
    );

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].thread_key.as_deref(), Some("MR #456"));
    assert_eq!(notifications[0].conversation_key.as_deref(), Some("gitlab:123:456"));
    assert!(notifications[0].prefer_thread_reply);
    assert!(matches!(notifications[0].delivery, NotificationDelivery::Append));
    assert!(notifications[0].body.contains("AI review recommendations"));
}
```

In `tests/slack_sink.rs`, add a Tokio test that sends the same `Append` notification twice after a root exists. Assert that the mock records two reply `chat.postMessage` payloads with the root `thread_ts`, no `chat.update` request, and no `metadata` on either reply.

In `tests/cli_integration.rs`, add a `#[cfg(feature = "slack-app")]` test modeled on `refine_mode_fixture_sends_slack_notifications_without_gitlab_env`. It must run:

```rust
run_cli(
    "explain",
    &[
        "--fixture",
        "fixtures/draft-explain.toml",
        "--send-notifications",
    ],
    &[
        ("MR_MILCHICK_CONFIG_PATH", config_path.as_str()),
        ("MR_MILCHICK_SLACK_BOT_TOKEN", "xoxb-test"),
        ("RUST_LOG", "off"),
    ],
)
```

Assert success, a delivered Slack notification in stdout, exactly two `chat.postMessage` requests (new root then reply), one `conversations.history` request, and a reply containing `AI review recommendations` with a non-null `thread_ts`.

In `runtime/runtime.rs`, add a `#[cfg(test)]` module with a minimal `NotificationSink` that returns `ConnectorError::Request("Slack unavailable".to_string())` and a no-op `PlatformConnector` used only to construct `RuntimeWiring`. Add this test:

```rust
#[tokio::test]
async fn reports_notification_sink_failure_without_failing_delivery() {
    let reports = wiring_with_failing_slack_sink()
        .deliver_notifications(
            ExecutionStrategy::Real,
            NotificationPolicy::Always,
            &[sample_slack_append_notification()],
            &crate::core::model::ReviewActionReport::default(),
        )
        .await
        .expect("notification failures should be reported, not returned");

    assert_eq!(reports.len(), 1);
    assert!(!reports[0].delivered);
    assert!(reports[0]
        .detail
        .as_deref()
        .is_some_and(|detail| detail.contains("Slack unavailable")));
}
```

- [ ] **Step 2: Run the new tests to verify the intended failures**

Run:

```bash
rtk cargo test slack_explain_notification_appends_to_the_review_thread --locked
rtk cargo test --test slack_sink append_notifications_never_update_managed_thread_messages --locked
rtk cargo test --test cli_integration explain_mode_fixture_sends_slack_advisory_reply --locked
rtk cargo test runtime::tests::reports_notification_sink_failure_without_failing_delivery --locked
```

Expected: the application unit test does not compile because `build_explain_notifications` is absent; the Slack and CLI tests fail because explain has no notification construction or fixture send flag. The runtime regression test should already pass, documenting the non-fatal delivery contract that the new explain notification relies on.

- [ ] **Step 3: Add fixture notification opt-in to `explain`**

Extend `ExplainArgs` to match `RefineArgs`:

```rust
#[derive(Debug, Args, Clone, Default)]
pub struct ExplainArgs {
    #[command(flatten)]
    pub fixture: FixtureArgs,

    /// Actually deliver notifications when running from a fixture
    #[arg(long)]
    pub send_notifications: bool,
}
```

Change `Command::send_notifications()` to return `args.send_notifications` for both `Refine` and `Explain`. Keep the existing `run_mode` validation, which already rejects the flag without `--fixture` and maps fixture opt-in to `ExecutionStrategy::Real`.

- [ ] **Step 4: Build and deliver the Slack explain notification after the platform action**

Import `render_slack_app_explain` in `app.rs`. Add this helper near the existing notification builders:

```rust
fn build_explain_notifications(
    snapshot: &crate::core::model::ReviewSnapshot,
    template_catalog: &TemplateCatalog,
    context: &crate::core::message_templates::SummaryTemplateContext,
    sink_kinds: &[NotificationSinkKind],
) -> Vec<NotificationMessage> {
    let thread_key = Some(format!("MR #{}", snapshot.review_ref.review_id));
    let conversation_key = Some(notification_conversation_key(snapshot));

    sink_kinds
        .iter()
        .filter(|&&sink| matches!(sink, NotificationSinkKind::SlackApp))
        .map(|sink| NotificationMessage {
            sink: *sink,
            subject: format!(
                "Mr. Milchick advisory explain for MR #{}",
                snapshot.review_ref.review_id
            ),
            body: render_slack_app_explain(template_catalog, context),
            audience: NotificationAudience::Default,
            severity: NotificationSeverity::Info,
            thread_key: thread_key.clone(),
            prefer_thread_reply: true,
            conversation_key: conversation_key.clone(),
            delivery: NotificationDelivery::Append,
        })
        .collect()
}
```

Pass `&preview_sink_kinds` from `run_mode` into `run_explain_mode`, matching refine's configured-sink flow. In `run_explain_mode`, bind `explain_context` before rendering the platform Markdown, create the notification vector from it, execute `UpsertExplain`, then call `deliver_notifications` with the same strategy and notification policy:

```rust
let notification_reports = prepared
    .wiring
    .deliver_notifications(
        strategy,
        app_config.config.execution.notification_policy,
        &notifications,
        &review_report,
    )
    .await?;

print_execution_report(&crate::runtime::ExecutionReport {
    review_report,
    notification_reports,
});
```

For fixture runs, call `print_notification_previews(&notifications)` before delivery, as refine does. Do not change `RuntimeWiring::deliver_notifications`: its existing error-to-report conversion already implements the required best-effort Slack behavior.

- [ ] **Step 5: Run the focused behavior tests to verify they pass**

Run:

```bash
rtk cargo test app::tests::slack_explain_notification_appends_to_the_review_thread --locked
rtk cargo test --test slack_sink append_notifications_never_update_managed_thread_messages --locked
rtk cargo test --test cli_integration explain_mode_fixture_sends_slack_advisory_reply --locked
rtk cargo test --test cli_integration send_notifications_requires_fixture --locked
rtk cargo test runtime::tests::reports_notification_sink_failure_without_failing_delivery --locked
```

Expected: the explain fixture creates a root and one Slack advisory reply; repeated append replies make no `chat.update` calls; the existing invalid non-fixture flag behavior stays intact.

- [ ] **Step 6: Commit explain Slack delivery**

```bash
rtk git add apps/mr-milchick/src/cli.rs apps/mr-milchick/src/app.rs apps/mr-milchick/src/runtime/runtime.rs tests/slack_sink.rs tests/cli_integration.rs
rtk git commit -m "feat: send explain reviews to Slack threads"
```

### Task 3: Document and Package the 5.1.1 Release

**Files:**

- Modify: `mr-milchick.toml:330-380`
- Modify: `docs/config-reference.md:390-455`
- Modify: `docs/architecture.md:25-92`
- Modify: `docs/connectors-and-capabilities.md:16-25`
- Modify: `Cargo.toml:1-12`
- Modify: `Cargo.lock:812-825`
- Modify: `CHANGELOG:1-12`
- Include: `apps/mr-milchick/src/connectors/notifications/slack_app.rs` (the existing HTTP/rate-limit diagnostic hardening and its test)

**Interfaces:**

- Consumes: the finished public template and explain-notification behavior, plus the already-present Slack HTTP error diagnostic change.
- Produces: aligned operator documentation, package metadata `5.1.1`, and changelog entries for both changes.

- [ ] **Step 1: Write release assertions before editing release metadata**

Add a CLI integration test named `version_command_reports_5_1_1` that runs `run_cli("version", &[], &[("RUST_LOG", "off")])` and asserts successful output starts with `mr-milchick 5.1.1 (`. This is the regression guard for the package version exposed to operators.

- [ ] **Step 2: Run the release assertion to verify it fails**

Run:

```bash
rtk cargo test --test cli_integration version_command_reports_5_1_1 --locked
```

Expected: the assertion fails because the binary reports `mr-milchick 5.1.0`.

- [ ] **Step 3: Update documentation and release metadata**

Update the commented Slack configuration example in `mr-milchick.toml` with:

```toml
# explain_thread = """
# *{{summary_title}}*
# Merge request: {{mr_link}}
#
# {{summary_intro}}
# {{findings_block}}
# {{actions_block}}
# {{recommendations_block}}
# _{{closing_tone_message}}_
# """
```

Add `templates.slack_app.explain_thread` to the configuration reference and explicitly say it uses review-comment placeholders, renders only the append-only `explain` reply, and does not change platform explain comments.

Change architecture and capability descriptions so `explain` performs the existing platform-comment upsert followed by an optional append-only Slack thread reply. Replace the stale statement that explain never sends notifications.

Change package version strings from `5.1.0` to `5.1.1` in `Cargo.toml` and the root `mr-milchick` package entry in `Cargo.lock`.

Insert this 2026-08-05 release section immediately below the changelog introduction:

```markdown
## [5.1.1] — 2026-08-05

This patch release completes Slack visibility for advisory reviews and improves diagnostics for Slack API failures.

### Added

- `explain` now appends the full Slack-native advisory review to the existing MR thread after its managed platform explain comment completes. Each completed explain run creates a new thread reply rather than updating a prior advisory reply.
- Optional `[templates.slack_app].explain_thread` overrides the Slack-only advisory reply while retaining the GitLab and GitHub explain templates.

### Fixed

- Slack history failures now retain the HTTP status, a bounded response body, and `Retry-After` guidance for rate-limited requests instead of discarding the actionable API detail.
```

- [ ] **Step 4: Run release, formatting, and complete-suite verification**

Run:

```bash
rtk cargo fmt --check
rtk cargo test --workspace --locked
rtk cargo test --workspace --no-default-features --features "gitlab slack-app llm-claude-code" --locked
rtk cargo check --no-default-features --features "gitlab slack-app llm-local llm-claude-code" --locked
rtk cargo run --locked -- version
```

Expected: formatter, default test suite, Claude Code feature suite, combined inference feature check, and version command all exit zero; the last command reports `mr-milchick 5.1.1`.

- [ ] **Step 5: Inspect the final release diff**

Run:

```bash
rtk git diff --check
rtk git diff -- Cargo.toml Cargo.lock CHANGELOG apps/mr-milchick/src/connectors/notifications/slack_app.rs
rtk git status --short
```

Expected: no whitespace errors; the release diff includes both the append-only explain-thread feature and the existing Slack HTTP diagnostic hardening; no unrelated files are staged.

- [ ] **Step 6: Commit the release package and diagnostics**

```bash
rtk git add mr-milchick.toml docs/config-reference.md docs/architecture.md docs/connectors-and-capabilities.md Cargo.toml Cargo.lock CHANGELOG apps/mr-milchick/src/connectors/notifications/slack_app.rs tests/cli_integration.rs
rtk git commit -m "release: prepare v5.1.1"
```
