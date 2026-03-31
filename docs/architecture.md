# Architecture

Mr Milchick is a CLI application with a small runtime surface and explicit boundaries between planning, execution, platform connectors, and notification sinks.

## Module Layout

Mr Milchick is published as a single crate from the repository root. The internal architecture is preserved as module boundaries inside that crate:

- `apps/mr-milchick/src/app.rs`: CLI orchestration, environment loading, flavor validation, runtime wiring, and command dispatch
- `apps/mr-milchick/src/core`: policy logic, reviewer routing, CODEOWNERS planning, summary rendering, and tone selection
- `apps/mr-milchick/src/runtime`: platform connector traits, notification sink traits, execution strategy, and execution reporting
- `apps/mr-milchick/src/connectors`: GitLab and GitHub platform connectors plus optional notification sinks

## Execution Flow

The app follows a linear flow:

```text
CI environment
  -> context builder
  -> runtime config + optional flavor config
  -> platform connector snapshot load
  -> rule evaluation
  -> reviewer routing and optional CODEOWNERS override
  -> summary message render
  -> action plan
  -> execution via the same platform connector
  -> optional notification fanout
```

The planning path is shared across `observe`, `refine`, and `explain`. The difference is what happens after the action plan is built.

## Command Behavior

- `observe` prints findings and the actions that `refine` would take.
- `explain` prints findings, the action plan, a rendered summary preview, merge request details, and routing/CODEOWNERS reasoning.
- `refine` executes the action plan through the runtime wiring layer.
- `version` prints build metadata and compiled capabilities without entering the planning flow.

Even the non-mutating commands still load the merge request snapshot through the platform connector, so they may require GitLab or GitHub credentials in real environments.

## Runtime Boundaries

The runtime wiring layer enforces a few important rules:

- review reads and writes always go through the same compiled platform connector
- notification sinks never change the review plan
- dry-run affects execution only, not planning
- notifications are only considered during real `refine` execution when the pipeline is not already blocked
- notification delivery is controlled by the resolved notification policy, not by individual connectors or sinks

That keeps the core planner deterministic and makes the side effects easy to reason about in CI logs.

## Platform Connector Behavior

Each platform connector is responsible for:

- loading the review snapshot
- merging new reviewers with existing ones before assignment
- creating or updating the Mr Milchick summary comment
- skipping summary writes when the rendered body is unchanged

Summary comment idempotency is based on the marker:

```text
<!-- mr-milchick:summary -->
```

That marker lets the connector update the existing comment instead of duplicating it on every run.

## Current Scope

The core model already has placeholders for additional connectors and sinks, but the implemented runtime surface today is:

- GitLab platform connector
- GitHub platform connector
- Slack app sink
- Slack workflow sink

For the build and capability model, see [connectors-and-capabilities.md](connectors-and-capabilities.md). For routing details, see [reviewer-routing.md](reviewer-routing.md).
