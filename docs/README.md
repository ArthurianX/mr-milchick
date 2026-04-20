# Documentation

This directory holds the detailed docs that used to live in the root README. Start with the page that matches the job you are trying to do.

## Start Here

- [CI quickstart](ci-quickstart.md): build the binary in GitLab CI, start with `observe`, move to `refine`, and optionally add a slower follow-up `explain` job plus Slack delivery.
- [Configuration reference](config-reference.md): the resolved config model, the supported `mr-milchick.toml` schema, and the remaining env inputs.

## Review Routing

- [Reviewer routing](reviewer-routing.md): area-based selection, CODEOWNERS override behavior, mandatory reviewers, fallback reviewers, and the `[reviewers]` TOML section.

## Runtime Surface

- [Connectors and capabilities](connectors-and-capabilities.md): what is implemented today, how compiled features map to runtime behavior, and how to verify the artifact you built.
- [Architecture](architecture.md): crate boundaries, execution flow, the split between fast governance and slower advisory explain, and managed-comment ownership.
- [Local LLM review](local-llm.md): optional GGUF-backed advisory review, the explain gate, environment variables, CI model distribution, smoke tests, and repeatable benchmarks.

## Output And Tone

- [Tone and messages](tone-and-messages.md): deterministic tone selection, category usage, and where those messages appear.
- [Message templates](message-templates.md): governance-summary, advisory-explain, and Slack template fields, placeholders, tone variables, and example templates.

## Validation

- [Local testing](local-testing.md): local smoke-test commands, expected behavior, and notes for testing GitLab and Slack integrations without changing the docs setup flow.
- [Fixture testing](fixture-testing.md): run `observe`, `explain`, and `refine` from local TOML fixtures, understand the explain gate, preview notifications, and optionally send Slack notifications without a live MR.
