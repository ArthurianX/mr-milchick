# Message Templates

Mr Milchick now supports connector-specific message templates in `mr-milchick.toml`.

Templates only affect connector output in this version:

- GitLab summary comments
- Slack app root and thread messages
- Slack workflow title and thread payloads

CLI output, rule findings, and policy behavior are unchanged.

## How Templates Work

- Every connector has built-in default templates compiled into the binary.
- You can override any template field in `mr-milchick.toml`.
- Overrides are field-by-field. If you only set one field, the others keep their defaults.
- Templates use `{{placeholder}}` interpolation only.
- Unknown or malformed placeholders cause a warning and that one field falls back to the built-in default.

## Template Layout

```toml
[templates.gitlab]
summary = """..."""

[templates.slack_app]
root = """..."""
thread = """..."""

[templates.slack_workflow]
title = """..."""
thread = """..."""
```

## Available Placeholders

These placeholders are available in all connector templates:

- `mr_number`
- `mr_ref`
- `mr_title`
- `mr_url`
- `mr_author_username`
- `source_branch`
- `target_branch`
- `is_draft`
- `changed_file_count`
- `findings_count`
- `blocking_findings_count`
- `warning_findings_count`
- `info_findings_count`
- `actions_count`
- `reviewers_count`
- `new_reviewers_count`
- `existing_reviewers_count`
- `mr_link`
- `reviewers_list`
- `new_reviewers_list`
- `existing_reviewers_list`
- `findings_block`
- `actions_block`
- `tone_message`
- `tone_category`

Additional shared placeholders currently exposed by the renderer:

- `summary_title`
- `summary_intro`
- `summary_footer`
- `notification_title`
- `notification_subject_action`
- `notification_subject_suffix`
- `mr_line`
- `reviewers_line`
- `mr_ref_link`

GitLab summary templates also support:

- `closing_tone_message`
- `closing_tone_category`

## Placeholder Semantics

Some placeholders are rendered differently per connector on purpose.

- `mr_link` is already formatted for the target connector.
  GitLab example: `[Frontend adjustments](https://gitlab.example.com/...)`
  Slack app example: `<https://gitlab.example.com/...|Frontend adjustments>`
  Slack workflow example: `Frontend adjustments (https://gitlab.example.com/...)`
- `reviewers_list`, `new_reviewers_list`, and `existing_reviewers_list` are already formatted for the connector.
- `findings_block` is preformatted multi-line content for the connector.
- `actions_block` is preformatted multi-line content for the connector.
- Empty optional values resolve to `""`.
- When there are no findings, `findings_block` resolves to `No findings were produced.`
- When there are no visible actions, `actions_block` resolves to `None`

## Tone Behavior

Tone is still deterministic and selected from the tone registry.

- GitLab summary `tone_*` uses `Observation`
- GitLab summary `closing_tone_*` uses the same footer logic as before:
  - `Blocking`
  - `NoAction`
  - `Praise`
  - `Refinement`
- Slack app and Slack workflow `tone_*` use `ReviewRequest` when reviewers are being assigned, otherwise `Observation`

That means you can either render the selected line directly with `{{tone_message}}` or refer to its category with `{{tone_category}}`.

## Default Shape By Connector

The built-in defaults preserve the existing message behavior.

### GitLab Summary

The default GitLab template renders:

- a summary heading
- the opening tone line
- the findings block
- the actions block
- the closing tone footer

Example override:

```toml
[templates.gitlab]
summary = """
## {{summary_title}}

{{tone_message}}

MR: {{mr_link}}
Branches: `{{source_branch}}` -> `{{target_branch}}`
Changed files: {{changed_file_count}}

{{findings_block}}

{{actions_block}}

_{{closing_tone_message}}_
"""
```

Example output:

```md
## Mr. Milchick Review Summary

Mr. Milchick is reviewing the situation.

MR: [Frontend adjustments](https://gitlab.example.com/group/project/-/merge_requests/3995)
Branches: `feat/buttons` -> `develop`
Changed files: 3

- **Warning**: Missing label.

- Assign reviewers: @principal-reviewer, @bob

_A refinement opportunity has been identified._
```

### Slack App

Slack app uses two template fields:

- `root`: the first posted Slack message
- `thread`: the threaded follow-up

Example override:

```toml
[templates.slack_app]
root = ":gitlab: {{notification_subject_action}} {{mr_ref_link}}, by @{{mr_author_username}}{{notification_subject_suffix}}"
thread = """
*{{notification_title}}*
{{mr_line}}
{{reviewers_line}}
{{summary_intro}}
{{findings_block}}
{{actions_block}}
_{{summary_footer}}_
"""
```

Example output:

```text
:gitlab: Reviews Needed for <https://gitlab.example.com/group/project/-/merge_requests/3995|MR #3995>, by @arthur :pepe-review:
```

```text
*Mr. Milchick Review Summary*
Merge request: <https://gitlab.example.com/group/project/-/merge_requests/3995|Frontend adjustments>
_Assign reviewers_ *@principal-reviewer* *@bob*
Mr. Milchick is reviewing the situation.
No findings were produced.
• None
_This request demonstrates admirable clarity._
```

Notes:

- Slack user mention rewriting still happens after template rendering.
- If your template includes `@principal-reviewer`, the Slack app sink can still rewrite it to `<@U...>` through `MR_MILCHICK_SLACK_USER_MAP`.

### Slack Workflow

Slack workflow also uses two fields:

- `title`: sent as `mr_milchick_says`
- `thread`: sent as `mr_milchick_says_thread`

Example override:

```toml
[templates.slack_workflow]
title = ":gitlab: {{notification_subject_action}} {{mr_ref_link}}, by @{{mr_author_username}}{{notification_subject_suffix}}"
thread = """
{{notification_title}}
{{mr_line}}
{{reviewers_line}}
{{summary_intro}}
{{findings_block}}
{{actions_block}}
{{summary_footer}}
"""
```

Example output:

```text
:gitlab: Reviews Needed for MR #3995 (https://gitlab.example.com/group/project/-/merge_requests/3995), by @arthur :pepe-review:
```

```text
Mr. Milchick Review Summary
Merge request: Frontend adjustments (https://gitlab.example.com/group/project/-/merge_requests/3995)
Assign reviewers @principal-reviewer @bob
Mr. Milchick is reviewing the situation.
No findings were produced.
- None
This request demonstrates admirable clarity.
```

## Practical Starting Templates

### Minimal GitLab Customization

```toml
[templates.gitlab]
summary = """
## Review Summary

{{mr_line}}

{{findings_block}}

{{actions_block}}
"""
```

### Minimal Slack App Customization

```toml
[templates.slack_app]
root = ":gitlab: {{mr_ref_link}} needs attention"
```

### Minimal Slack Workflow Customization

```toml
[templates.slack_workflow]
thread = """
{{mr_line}}
{{reviewers_line}}
{{findings_block}}
{{actions_block}}
"""
```

## Authoring Tips

- Start with one connector field at a time.
- Prefer the preformatted placeholders like `mr_link`, `reviewers_list`, `findings_block`, and `actions_block` unless you want a very custom layout.
- For GitLab, use Markdown intentionally because the body is posted directly as markdown after the marker is prepended.
- For Slack app, you can use Slack formatting such as `*bold*`, `_italic_`, and `<url|label>`.
- For Slack workflow, prefer plain text because workflow payloads are usually rendered by a downstream workflow step.
- If a template seems ignored, check logs for a warning about an invalid placeholder and confirm that field fell back to the default.
