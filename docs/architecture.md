# Architecture

Mr Milchick is a CLI with two explicit input boundaries:

- `context/`: review context from CI and review-event payloads
- `config/`: application policy/runtime config from `mr-milchick.toml` plus a small env secret layer

That split is now enforced in code so review metadata and operational configuration do not drift together.

## Module Layout

- `apps/mr-milchick/src/app.rs`: CLI orchestration, wiring, command dispatch
- `apps/mr-milchick/src/context`: CI/review context loading and normalization
- `apps/mr-milchick/src/config`: config schema, env secret/path loading, resolved config validation
- `apps/mr-milchick/src/core`: pure policy logic, routing, CODEOWNERS planning, summaries, tone, templates
- `apps/mr-milchick/src/runtime`: execution strategy, connector traits, execution reporting
- `apps/mr-milchick/src/connectors`: GitLab/GitHub platform connectors and optional notification sinks

## Execution Flow

```text
CI / event payload
  -> context builder
  -> resolved app config
  -> platform snapshot load
  -> rules
  -> reviewer routing + optional CODEOWNERS override
  -> action plan
  -> governance summary render

observe
  -> print deterministic diagnostics only

refine
  -> execute governance actions
  -> upsert deterministic summary comment + hidden governance metadata
  -> optional notification fanout

explain
  -> reload Milchick governance summary comment
  -> gate on prior governance metadata
  -> optional advisory inference
  -> upsert separate advisory explain comment
```

The three commands still share one deterministic planning path, but they now diverge more intentionally at the end.

## Command Semantics

- `observe`: verbose deterministic inspection only. It prints findings, the governance action plan, the rendered governance summary preview, snapshot details, CODEOWNERS details, and fixture notification previews. It never mutates review platforms and never invokes inference.
- `refine`: fast governance execution. It applies reviewer or label actions, always upserts the deterministic governance summary comment, can deliver configured notifications, and fails the current pipeline when blocking policy remains unresolved.
- `explain`: slow advisory follow-up. It first reloads Milchick's managed governance summary comment, parses the hidden metadata appended by `refine`, and skips itself when the latest governance pass reported no applied effect and no blocking outcome. When the gate passes, it runs advisory inference and upserts only the managed explain comment.

## Managed Comments

Milchick now owns two separate platform comments:

- the governance summary comment, marked with `<!-- mr-milchick:summary -->`
- the advisory explain comment, marked with `<!-- mr-milchick:explain -->`

`refine` owns the summary comment and `explain` owns the explain comment. `explain` does not rewrite the governance summary.

The summary comment also carries a hidden JSON metadata block that records:

- whether the last `refine` ran as `real` or `dry-run`
- whether the outcome remained blocked
- which governance action kinds were applied
- whether the run had a governance effect worth explaining later

## Config Boundary

`config/` now resolves one typed `ResolvedConfig` at startup.

- TOML owns non-secret runtime settings such as reviewer routing, CODEOWNERS options, dry-run policy, Slack sink enablement, and inference tuning.
- Env is limited to config-path selection and secrets.
- Legacy env-driven runtime configuration is rejected instead of silently merged.

This keeps merge logic out of `app.rs` and makes new features land in one place instead of four.

## Runtime Rules

- Core rules stay pure and side-effect free.
- Platform reads and writes always go through the same compiled connector.
- Notification sinks never change planning decisions.
- `dry_run` affects platform writes in `refine` and `explain`; `observe` is preview-only already.
- Notification delivery follows the resolved notification policy, not sink-specific heuristics.
- `observe` and `refine` do not wire the inference backend.
- `explain` never assigns reviewers, changes labels, fails the pipeline, or sends notifications.

## Current Implemented Surface

- platform connectors: GitLab, GitHub
- notification sinks: Slack app, Slack workflow
- advisory inference backend: local llama.cpp when compiled with `llm-local`

For config details, see [config-reference.md](config-reference.md). For build capability mapping, see [connectors-and-capabilities.md](connectors-and-capabilities.md).
