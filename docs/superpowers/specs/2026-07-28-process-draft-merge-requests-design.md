# Process Draft Merge Requests Design

## Summary

Mr. Milchick will process draft merge requests through the same governance
workflow as ready merge requests. A draft MR's first pipeline will perform
intake, reviewer routing, platform-summary delivery, notifications, and any
configured advisory explanation. Draft state remains metadata used for labels
and rendered context; it will no longer defer, suppress, or short-circuit
governance.

This removes the gap where changing a draft MR to ready does not create a new
GitLab pipeline and therefore leaves work unperformed.

## Goals

- Take full governance custody of a draft MR during its first pipeline.
- Assign reviewers and emit normal platform and notification output for drafts.
- Apply the same intake checks to drafts as to ready merge requests.
- Add the configured draft label while an MR is draft and remove it when a
  later Milchick run observes that the MR is ready.
- Preserve draft state in snapshot output, templates, and rule predicates.

## Non-goals

- Adding GitLab webhooks, scheduled reconciliation, or a separate recheck
  service.
- Forcing GitLab to create a pipeline when the draft flag changes.
- Removing support for draft-aware label rules or template fields.

## Behavior

`observe` will assess description and changed-path completeness for both draft
and ready MRs. Its risk label behavior is unchanged. It adds `draft_label` when
the MR is draft and removes that same label when the MR is ready, making the
label converge on the snapshot's current state.

`refine` will always execute its planned governance actions, write or update
the managed summary comment, and deliver configured notifications. Reviewer
routing will no longer defer assignment for drafts.

`explain` will no longer return early for drafts. When its ordinary governance
metadata gate permits advisory output, it will use the existing inference and
comment-rendering path.

The draft status remains available to templates, inference prompts, and
label-rule predicates. Operators that want draft-specific labels can retain
their existing label rules.

## Implementation Boundaries

The change is limited to the existing draft-specific gates in the observe
planner, reviewer action planner, and application execution/notification
helpers. It does not alter connector contracts, CI context parsing, template
schema, or rule-engine purity.

## Testing

Regression coverage will prove that a draft MR:

- receives reviewer assignment from `refine`;
- receives its managed summary and configured notification delivery;
- is eligible for `explain` when its normal governance gate allows it; and
- receives the same intake blockers as an equivalent ready MR.

Unit coverage will prove that `observe` adds the draft label for drafts and
removes it for ready MRs. Existing ready-MR behavior remains covered by the
full test suite.

## Error Handling and Compatibility

No new external calls, configuration fields, or failure modes are introduced.
Existing TOML `draft_label`, draft-aware label rules, and template values remain
compatible. A ready MR only loses the configured draft label through normal
label mutation; failure semantics remain those of the existing executor.
