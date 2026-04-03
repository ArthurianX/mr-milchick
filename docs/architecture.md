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
  -> summary render
  -> action plan
  -> execution
  -> optional notification fanout
```

`observe`, `explain`, and `refine` share the same planning path. Only the last step changes.

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
- `dry_run` affects execution only.
- Notification delivery follows the resolved notification policy, not sink-specific heuristics.

## Current Implemented Surface

- platform connectors: GitLab, GitHub
- notification sinks: Slack app, Slack workflow
- advisory inference backend: local llama.cpp when compiled with `llm-local`

For config details, see [config-reference.md](config-reference.md). For build capability mapping, see [connectors-and-capabilities.md](connectors-and-capabilities.md).
