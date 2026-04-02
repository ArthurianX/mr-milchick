# Documentation

This directory holds the detailed docs that used to live in the root README. Start with the page that matches the job you are trying to do.

## Start Here

- [CI quickstart](ci-quickstart.md): build the binary in GitLab CI, run `observe` or `refine`, and connect either Slack workflow or Slack app delivery.
- [Configuration reference](config-reference.md): every supported environment variable, the optional `mr-milchick.toml` file, and the current precedence rules.

## Review Routing

- [Reviewer routing](reviewer-routing.md): area-based selection, CODEOWNERS override behavior, mandatory reviewers, fallback reviewers, and `MR_MILCHICK_MAX_REVIEWERS`.

## Runtime Surface

- [Connectors and capabilities](connectors-and-capabilities.md): what is implemented today, how platform connector and notification features map to runtime behavior, and how to verify the compiled artifact.
- [Architecture](architecture.md): crate boundaries, execution flow, command behavior, and idempotent GitLab summary handling.
- [Local LLM review](local-llm.md): optional GGUF-backed advisory review, environment variables, CI model distribution, smoke tests, and repeatable benchmarks.

## Output And Tone

- [Tone and messages](tone-and-messages.md): deterministic tone selection, category usage, and where those messages appear.
- [Message templates](message-templates.md): connector template fields, placeholders, tone variables, and example templates for GitLab and Slack.

## Validation

- [Local testing](local-testing.md): local smoke-test commands, expected behavior, and notes for testing GitLab and Slack integrations without changing the docs setup flow.
- [Fixture testing](fixture-testing.md): run `observe`, `explain`, and `refine` from local TOML fixtures, preview notifications, and optionally send Slack notifications without a live MR.
