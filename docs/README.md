# Documentation

This directory holds the detailed docs that used to live in the root README. Start with the page that matches the job you are trying to do.

## Start Here

- [CI quickstart](ci-quickstart.md): build the binary in GitLab CI, run `observe` or `refine`, and connect either Slack workflow or Slack app delivery.
- [Configuration reference](config-reference.md): every supported environment variable, the optional `mr-milchick.toml` file, and the current precedence rules.

## Review Routing

- [Reviewer routing](reviewer-routing.md): area-based selection, CODEOWNERS override behavior, mandatory reviewers, fallback reviewers, and `MR_MILCHICK_MAX_REVIEWERS`.

## Runtime Surface

- [Connectors and capabilities](connectors-and-capabilities.md): what is implemented today, how Cargo features map to runtime behavior, and how to verify the compiled artifact.
- [Architecture](architecture.md): crate boundaries, execution flow, command behavior, and idempotent GitLab summary handling.

## Output And Tone

- [Tone and messages](tone-and-messages.md): deterministic tone selection, category usage, and where those messages appear.

## Validation

- [Local testing](local-testing.md): local smoke-test commands, expected behavior, and notes for testing GitLab and Slack integrations without changing the docs setup flow.
