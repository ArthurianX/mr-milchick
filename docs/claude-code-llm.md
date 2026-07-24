# Claude Code Advisory Review

Claude Code is an optional remote advisory backend for `mr-milchick explain`.
It receives the same bounded review snapshot as local inference and does not
participate in `observe` or `refine`.

```toml
[inference]
enabled = true
backend = "claude-code"
model = "fully-versioned-model-name"
timeout_ms = 120000
max_patch_bytes = 32768
```

Build with `llm-claude-code` and provide `CLAUDE_CODE_OAUTH_TOKEN` as a masked
pipeline secret. This is a Claude Code subscription/OAuth token, not an
Anthropic API key. The CLI must be version 2.1.218 or newer; the GitLab example
pins that version.

Claude Code runs non-interactively with tools disabled and receives the prompt
through stdin. Failed authentication, a missing token or CLI, malformed output,
and timeouts are advisory: governance continues without a local fallback.

Never expose this token to fork or untrusted contributor pipelines. Use the
[GitLab example](../examples/gitlab/claude-code-explain.gitlab-ci.yml) only for
trusted same-project merge requests.
