# Message Templates

Connector and notification templates are still configured from `mr-milchick.toml`.

## Supported Template Fields

```toml
[templates.gitlab]
summary = "..."
explain = "..."

[templates.github]
summary = "..."
explain = "..."

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

## Comment Ownership

Milchick now owns two separate review-platform comment templates:

- `summary`: the deterministic governance comment upserted by `refine`
- `explain`: the slower advisory comment upserted by `explain`

`refine` always owns the summary comment and appends its hidden governance metadata block after rendering. Template authors should not add that metadata block manually.

`explain` never rewrites the governance summary comment; it updates only the advisory explain comment.

## Placeholders

Templates still use `{{placeholder}}` interpolation only.

Common placeholders include:

- `mr_number`, `mr_ref`, `mr_title`, `mr_url`, `mr_author_username`
- `source_branch`, `target_branch`, `changed_file_count`
- `findings_block`, `actions_block`
- `pipeline_status_block`, `pipeline_status_count`
- `pipeline_status_passed_count`, `pipeline_status_failed_count`, `pipeline_status_unknown_count`
- `summary_title`, `summary_intro`, `summary_footer`
- `tone_message`, `tone_category`, `closing_tone_message`, `closing_tone_category`
- `notification_title`, `notification_subject`
- `llm_summary`, `recommendations_block`
- `reviewers_line`, `mr_ref_link`

Invalid placeholders warn and fall back to the built-in field template.

For backward compatibility, `summary` templates may still reference `{{llm_summary}}` or `{{recommendations_block}}`, but `refine` renders those values as empty strings. Milchick only populates them in `explain`.

## Built-In Shape

The built-in review-platform templates now separate governance and advisory output:

- `summary` renders findings, planned/applied governance actions, and deterministic tone.
- `explain` renders the same governance context plus the advisory `{{recommendations_block}}`.

Slack templates remain notification-only and are still used only by `refine`.

## Optional Pipeline Status Block

If `[notifications.pipeline_status]` is enabled, Milchick can enrich Slack notifications with status lines built from `*/milchick-status/*.json` files that already exist in the workspace.

This is optional and mainly useful for internal CI setups where earlier jobs emit compact JSON summaries for later pipeline stages.

The main placeholders are:

- `pipeline_status_block`
- `pipeline_status_count`
- `pipeline_status_passed_count`
- `pipeline_status_failed_count`
- `pipeline_status_unknown_count`

The built-in Slack templates already include `{{pipeline_status_block}}`. If no status files are found, the placeholder renders as an empty string.

## Slack Mention Rewriting

Slack mention rewriting now comes from TOML:

```toml
[notifications.slack_app.user_map]
"principal-reviewer" = "U01234567"
"alice" = "U07654321"
```

That mapping is applied by the Slack app sink when it rewrites `@username` into Slack user mentions.

For the full config schema, see [config-reference.md](config-reference.md).
