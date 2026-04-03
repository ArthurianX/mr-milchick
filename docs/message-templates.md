# Message Templates

Connector and notification templates are still configured from `mr-milchick.toml`.

## Supported Template Fields

```toml
[templates.gitlab]
summary = "..."

[templates.github]
summary = "..."

[templates.slack_app]
first_root = "..."
first_thread = "..."
update_root = "..."
update_thread = "..."

[templates.slack_workflow]
first_title = "..."
first_thread = "..."
update_title = "..."
update_thread = "..."
```

Missing fields keep the built-in defaults.

## Placeholders

Templates still use `{{placeholder}}` interpolation only.

Common placeholders include:

- `mr_number`, `mr_ref`, `mr_title`, `mr_url`, `mr_author_username`
- `source_branch`, `target_branch`, `changed_file_count`
- `findings_block`, `actions_block`
- `summary_title`, `summary_intro`, `summary_footer`
- `notification_title`, `notification_subject`
- `reviewers_line`, `mr_ref_link`

Invalid placeholders warn and fall back to the built-in field template.

## Slack Mention Rewriting

Slack mention rewriting now comes from TOML:

```toml
[notifications.slack_app.user_map]
"principal-reviewer" = "U01234567"
"alice" = "U07654321"
```

That mapping is applied by the Slack app sink when it rewrites `@username` into Slack user mentions.

For the full config schema, see [config-reference.md](config-reference.md).
