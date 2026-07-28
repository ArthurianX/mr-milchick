# Process Draft Merge Requests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Process draft merge requests through the same intake and governance workflow as ready merge requests while keeping `status::draft` accurate.

**Architecture:** Remove the four draft-specific execution gates and the reviewer-routing early return. The observe planner continues to expose draft status but applies the same blockers to every MR and converges the configured draft label by adding it for drafts and removing it for ready MRs. Existing connectors, templates, and notification machinery remain unchanged.

**Tech Stack:** Rust, Tokio, inline `#[cfg(test)]` unit tests, integration tests using the in-process mock GitLab/Slack server.

## Global Constraints

- Do not add webhooks, scheduled reconciliation, or CI trigger behavior.
- Preserve draft-aware template fields and label-rule predicates.
- Use `ReviewAction::{AddLabels, RemoveLabels}` for draft-label convergence.
- Test each behavior before production changes and observe the intended failure.
- Keep the rule engine and observe planner pure; connector mutations remain in the runtime layer.

---

### Task 1: Make observe enforce intake and converge the draft label

**Files:**
- Modify: `apps/mr-milchick/src/core/observe.rs:118-176`
- Test: `apps/mr-milchick/src/core/observe.rs:582-596`

**Interfaces:**
- Consumes: `plan_observe(ctx, snapshot, areas, config) -> ObservePlan` and `ObserveConfig::draft_label`.
- Produces: an `ObservePlan` whose `blocking_reasons` do not vary by draft state and whose actions express the current draft-label state.

- [ ] **Step 1: Write failing observe tests**

Replace the draft-only test with two assertions: a draft with a missing description blocks intake, and a ready snapshot with `status::draft` plans its removal.

```rust
#[test]
fn draft_plan_blocks_missing_description_and_applies_draft_label() {
    let mut snapshot = snapshot(None, vec!["apps/roulette/src/lib.rs"]);
    snapshot.is_draft = true;

    let plan = plan_observe(&ctx(), &snapshot, &areas(), &observe_config());

    assert_eq!(plan.status, ObserveStatus::Blocked);
    assert!(plan.should_fail());
    assert!(plan.actions.iter().any(|action| matches!(
        action,
        ReviewAction::AddLabels { labels } if labels == &vec!["status::draft".to_string()]
    )));
}

#[test]
fn ready_plan_removes_draft_label() {
    let mut snapshot = snapshot(Some("real description"), vec!["apps/roulette/src/lib.rs"]);
    snapshot.labels = vec!["status::draft".to_string()];

    let plan = plan_observe(&ctx(), &snapshot, &areas(), &observe_config());

    assert!(plan.actions.iter().any(|action| matches!(
        action,
        ReviewAction::RemoveLabels { labels } if labels == &vec!["status::draft".to_string()]
    )));
}
```

- [ ] **Step 2: Run the tests to verify the intended failures**

Run: `rtk cargo test draft_plan_blocks_missing_description_and_applies_draft_label`

Then run: `rtk cargo test ready_plan_removes_draft_label`

Expected: the draft test reports `Draft` rather than `Blocked`; the ready-label test has no matching removal action.

- [ ] **Step 3: Implement the minimal observe change**

Always add invalid-description and unmatched-path reasons. Keep `ObserveStatus::Draft` only if it still represents a distinct non-blocking status; otherwise derive `Passed`/`Blocked` from reasons. Add the configured label for drafts and remove it for ready MRs:

```rust
for reason in &description.reasons {
    if !matches!(description.status, DescriptionStatus::Meaningful | DescriptionStatus::NotRequired) {
        blocking_reasons.push(reason.clone());
    }
}
if !risk.unmatched_paths.is_empty() {
    blocking_reasons.push(format!(
        "Unmatched changed paths require area configuration: {}.",
        risk.unmatched_paths.join(", ")
    ));
}

if snapshot.is_draft {
    actions.push(ReviewAction::AddLabels { labels: vec![config.draft_label.clone()] });
} else {
    actions.push(ReviewAction::RemoveLabels { labels: vec![config.draft_label.clone()] });
}
```

- [ ] **Step 4: Run the focused observe tests to verify they pass**

Run: `rtk cargo test core::observe::tests`

Expected: all observe tests pass.

- [ ] **Step 5: Commit the observe behavior**

```bash
rtk git add apps/mr-milchick/src/core/observe.rs
rtk git commit -m "fix: enforce intake for draft merge requests"
```

### Task 2: Route reviewers for drafts

**Files:**
- Modify: `apps/mr-milchick/src/core/actions/planner.rs:75-87`
- Test: `apps/mr-milchick/src/core/actions/planner.rs:230-247`

**Interfaces:**
- Consumes: `enrich_with_reviewer_assignment(outcome, snapshot, areas, routing_config, codeowners_ctx) -> RuleOutcome`.
- Produces: the same `Action::AssignReviewers` recommendation for equivalent draft and ready snapshots.

- [ ] **Step 1: Write the failing reviewer-routing test**

Replace the deferred-assignment test with:

```rust
#[test]
fn assigns_reviewers_for_draft_merge_request() {
    let enriched = enrich_with_reviewer_assignment(
        RuleOutcome::new(),
        &sample_snapshot(true, vec![]),
        &test_areas(),
        &ReviewerRoutingConfig::example(),
        &CodeownersContext::empty(),
    );

    assert!(matches!(
        enriched.action_plan.actions.as_slice(),
        [Action::AssignReviewers { reviewers }] if reviewer_usernames(reviewers) == vec!["principal-reviewer", "bob"]
    ));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `rtk cargo test assigns_reviewers_for_draft_merge_request`

Expected: no `AssignReviewers` action is planned because the draft early return remains.

- [ ] **Step 3: Implement the minimal reviewer-routing change**

Delete the `if snapshot.is_draft` block that appends the deferral finding and returns before existing-reviewer filtering. Leave the tracing field `draft = snapshot.is_draft` intact for observability.

- [ ] **Step 4: Run the focused planner tests to verify they pass**

Run: `rtk cargo test core::actions::planner::tests`

Expected: all reviewer-planner tests pass.

- [ ] **Step 5: Commit the reviewer-routing behavior**

```bash
rtk git add apps/mr-milchick/src/core/actions/planner.rs
rtk git commit -m "fix: assign reviewers to draft merge requests"
```

### Task 3: Remove draft-only application and notification suppression

**Files:**
- Modify: `apps/mr-milchick/src/app.rs:580-625`, `apps/mr-milchick/src/app.rs:673-676`, `apps/mr-milchick/src/app.rs:1219-1221`, `apps/mr-milchick/src/app.rs:1279-1281`
- Test: `apps/mr-milchick/src/app.rs:2173-2225`
- Test: `tests/cli_integration.rs:465-527`

**Interfaces:**
- Consumes: the existing `run_refine_mode`, `run_explain_mode`, `build_observe_notifications`, and `build_notifications` APIs.
- Produces: normal summary/comment/notification execution for draft snapshots without new APIs or configuration.

- [ ] **Step 1: Write failing notification unit tests**

Rename the draft-suppression unit test and assert each builder yields messages for the draft snapshot. Use the existing `plan`, `snapshot`, `selector`, `ctx`, and `sinks` variables that test constructs:

```rust
assert!(!build_observe_notifications(
    &plan,
    &snapshot,
    &crate::core::message_templates::TemplateCatalog::default(),
    &selector,
    &ctx,
    &sinks,
).is_empty());
assert!(!build_notifications(
    &RuleOutcome::new(), &snapshot, &TemplateCatalog::default(), &selector, &ctx, &[], &sinks, None,
).is_empty());
```

- [ ] **Step 2: Run the notification unit test to verify it fails**

Run: `rtk cargo test draft_reviews_generate_notifications_for_every_sink`

Expected: both assertions fail because the helpers return an empty vector for drafts.

- [ ] **Step 3: Write the failing end-to-end refine test**

Rename `refine_mode_does_not_post_platform_or_slack_messages_for_drafts` and assert a successful draft refine run posts one managed review comment and, when built with `slack-app`, the normal three-message first-delivery sequence. Remove the assertion for the suppression stdout text.

```rust
assert_eq!(server.request_count("POST", &review_comments_path()), 1);
#[cfg(feature = "slack-app")]
assert_eq!(server.request_count("POST", "/slack/api/chat.postMessage"), 3);
```

- [ ] **Step 4: Run the integration test to verify it fails**

Run: `rtk cargo test --test cli_integration refine_mode_processes_draft_reviews_end_to_end`

Expected: the current draft path posts zero platform and Slack messages.

- [ ] **Step 5: Implement the minimal application changes**

In `run_refine_mode`, execute `prepared.outcome.action_plan.actions` directly, always generate and execute the summary action, and remove the suppression message/branch. In `run_explain_mode`, remove only the draft early return. Delete the leading `snapshot.is_draft` early returns from both notification builders. Retain the existing blocked-notification policy in `build_notifications`.

- [ ] **Step 6: Run focused application and integration tests to verify they pass**

Run: `rtk cargo test draft_reviews_generate_notifications_for_every_sink`

Then run: `rtk cargo test --test cli_integration refine_mode_processes_draft_reviews_end_to_end`

Expected: both commands pass; the integration test observes the managed summary and Slack delivery when that feature is compiled.

- [ ] **Step 7: Commit the execution changes**

```bash
rtk git add apps/mr-milchick/src/app.rs tests/cli_integration.rs
rtk git commit -m "fix: process draft merge requests end to end"
```

### Task 4: Verify the packaged behavior and documentation consistency

**Files:**
- Modify only if stale language is found: `docs/architecture.md`, `docs/ci-quickstart.md`, `docs/connectors-and-capabilities.md`, `docs/local-testing.md`
- Test: repository-wide Rust suite

**Interfaces:**
- Consumes: completed draft-processing behavior from Tasks 1-3.
- Produces: documentation that no longer says drafts defer refinement or suppress messages.

- [ ] **Step 1: Search for stale draft-deferral documentation**

Run: `rtk proxy rg -n -i "draft.*(defer|suppress|skip)|defer.*draft|suppress.*draft" docs mr-milchick.toml`

Expected: identify only wording that contradicts full draft processing; preserve documentation describing the optional draft label and template data.

- [ ] **Step 2: Update only contradictory documentation**

Replace statements that say draft reviews defer refinement, suppress output, or exit without governance with language that draft MRs receive full first-pipeline governance and retain a draft label while observed as draft.

- [ ] **Step 3: Run the complete suite and formatter check**

Run: `rtk proxy cargo fmt --check`

Then run: `rtk cargo test`

Expected: formatter exits zero and all unit and integration tests pass.

- [ ] **Step 4: Inspect the final change set**

Run: `rtk git diff --check`

Then run: `rtk git status`

Expected: no whitespace errors; only intended source, test, and documentation changes are present.

- [ ] **Step 5: Commit any documentation updates**

```bash
rtk git add docs mr-milchick.toml
rtk git commit -m "docs: describe draft MR governance"
```
