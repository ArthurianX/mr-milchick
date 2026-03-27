# Configuration Reference

This page lists the configuration inputs the application reads today and how they fit together.

## Configuration Sources

Mr Milchick combines three layers:

1. compiled capabilities from Cargo features
2. an optional `mr-milchick.toml` flavor file
3. environment variables for runtime behavior and secrets

The important rule is that the flavor file cannot request a review platform or notification sink that was not compiled into the binary. Runtime environment variables then provide the live CI context, reviewer pools, CODEOWNERS behavior, GitLab credentials, Slack credentials, and dry-run mode.

## Precedence And Validation

- Cargo features decide which connectors and sinks exist in the artifact.
- `mr-milchick.toml` validates that runtime intent matches the compiled artifact.
- Environment variables drive runtime behavior.
- `MR_MILCHICK_SLACK_USER_MAP` overrides `[slack_app.user_map]` from TOML.
- If no flavor file is present, the binary runs without flavor validation and compiled notification sinks are treated as enabled by default.

## Flavor File

The default runtime path is `mr-milchick.toml`. Override it with `MR_MILCHICK_FLAVOR_PATH` if needed.

```toml
notification_policy = "on-applied-action"

[review_platform]
kind = "gitlab"

[[notifications]]
kind = "slack-workflow"
enabled = true

[[notifications]]
kind = "slack-app"
enabled = false

[slack_app.user_map]
"engineer.lady1" = "U01234567"
"engineer.guy1" = "U07654321"

[templates.gitlab]
summary = """## {{summary_title}}

{{tone_message}}

{{findings_block}}

{{actions_block}}

_{{closing_tone_message}}_"""

[templates.slack_app]
root = ":gitlab: {{notification_subject_action}} {{mr_ref_link}}, by @{{mr_author_username}}{{notification_subject_suffix}}"
thread = """*{{notification_title}}*
{{mr_line}}
{{reviewers_line}}
{{summary_intro}}
{{findings_block}}
{{actions_block}}
_{{summary_footer}}_"""

[templates.slack_workflow]
title = ":gitlab: {{notification_subject_action}} {{mr_ref_link}}, by @{{mr_author_username}}{{notification_subject_suffix}}"
thread = """{{notification_title}}
{{mr_line}}
{{reviewers_line}}
{{summary_intro}}
{{findings_block}}
{{actions_block}}
{{summary_footer}}"""
```

Supported TOML fields today:

- `notification_policy`
- `review_platform.kind`
- `[[notifications]].kind`
- `[[notifications]].enabled`
- `[slack_app.user_map]`
- `[templates.gitlab].summary`
- `[templates.slack_app].root`
- `[templates.slack_app].thread`
- `[templates.slack_workflow].title`
- `[templates.slack_workflow].thread`

Behavior notes:

- `notification_policy` may be `always` or `on-applied-action`.
- `review_platform.kind` must match the compiled review connector. Today that means `gitlab`.
- `[[notifications]]` entries may use `slack-app` and `slack-workflow`.
- If `enabled` is omitted for a notification entry, it defaults to `true`.
- If a flavor file is present, only notification entries that are listed and enabled are activated.
- Template overrides are field-by-field. Missing fields keep the built-in default.
- Invalid template fields warn and fall back to the built-in default for that field.

## Message Templates

Connector output can be customized from `mr-milchick.toml`.

- GitLab uses `[templates.gitlab].summary`
- Slack app uses `[templates.slack_app].root` and `[templates.slack_app].thread`
- Slack workflow uses `[templates.slack_workflow].title` and `[templates.slack_workflow].thread`

Templates use `{{placeholder}}` interpolation only.

Core placeholders:

- `mr_number`, `mr_ref`, `mr_title`, `mr_url`, `mr_author_username`
- `source_branch`, `target_branch`, `is_draft`, `changed_file_count`
- `findings_count`, `blocking_findings_count`, `warning_findings_count`, `info_findings_count`
- `actions_count`, `reviewers_count`, `new_reviewers_count`, `existing_reviewers_count`
- `mr_link`, `reviewers_list`, `new_reviewers_list`, `existing_reviewers_list`
- `findings_block`, `actions_block`
- `tone_message`, `tone_category`

GitLab-only placeholders:

- `closing_tone_message`
- `closing_tone_category`

Useful renderer helpers:

- `summary_title`, `summary_intro`, `summary_footer`
- `notification_title`, `notification_subject_action`, `notification_subject_suffix`
- `mr_line`, `reviewers_line`, `mr_ref_link`

See [message-templates.md](message-templates.md) for complete examples and connector-specific output details.

## Environment Variables

### CI Context

These variables are read from the GitLab job environment:

| Variable | Required | Notes |
| --- | --- | --- |
| `CI_PROJECT_ID` | Yes | Required for all runs. |
| `CI_MERGE_REQUEST_IID` | For MR execution | Required in practice for merge request pipelines because the GitLab connector loads an MR snapshot. |
| `CI_PIPELINE_SOURCE` | No | Parsed to detect merge request pipelines; `merge_request_event` activates the MR flow. |
| `CI_MERGE_REQUEST_SOURCE_BRANCH_NAME` | No | Used in context and snapshot metadata. |
| `CI_MERGE_REQUEST_TARGET_BRANCH_NAME` | No | Used in context and snapshot metadata. |
| `CI_MERGE_REQUEST_LABELS` | No | Comma-separated labels. |

If the pipeline is not a merge request pipeline, Mr Milchick prints a no-op message and exits cleanly after capability reporting.

### GitLab Connector

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `GITLAB_TOKEN` | Yes for GitLab snapshot reads and live execution | None | Required by the GitLab connector. |
| `GITLAB_BASE_URL` | No | `https://gitlab.com/api/v4` | Override for self-managed GitLab or tests. |

### Reviewer Routing

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `MR_MILCHICK_REVIEWERS` | No | empty list | JSON array of reviewer definitions. |
| `MR_MILCHICK_MAX_REVIEWERS` | No | `2` | Caps only non-mandatory area-routed reviewers. |

Example:

```bash
MR_MILCHICK_REVIEWERS='[
  {"username":"milchick-duty","fallback":true},
  {"username":"principal-reviewer","mandatory":true},
  {"username":"alice","areas":["frontend","packages"]},
  {"username":"carol","areas":["backend"]},
  {"username":"grace","areas":["devops"]}
]'
MR_MILCHICK_MAX_REVIEWERS=2
```

Each reviewer entry may contain:

- `username`
- `areas`
- `fallback`
- `mandatory`

Supported area keys today:

- `frontend`, `apps`, `ui`
- `backend`, `api`
- `packages`, `shared`, `bootstrap`
- `devops`, `ops`, `infrastructure`, `scripts`, `patches`, `proxy`
- `documentation`, `docs`, `reports`
- `tests`, `test`, `qa`
- `unknown`

See [reviewer-routing.md](reviewer-routing.md) for the selection rules.

### CODEOWNERS

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `MR_MILCHICK_CODEOWNERS_ENABLED` | No | `true` | Accepts `true/false/1/0/yes/no/on/off`. |
| `MR_MILCHICK_CODEOWNERS_PATH` | No | auto-discovery | Overrides CODEOWNERS lookup. |

Auto-discovery checks these paths in order:

- `CODEOWNERS`
- `.github/CODEOWNERS`
- `.gitlab/CODEOWNERS`
- `.CODEOWNERS`

### Execution

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `MR_MILCHICK_DRY_RUN` | No | `false` | If enabled, `refine` produces a dry-run execution report and skips external writes. |
| `MR_MILCHICK_FLAVOR_PATH` | No | `mr-milchick.toml` | Alternative flavor file path. |
| `MR_MILCHICK_NOTIFICATION_POLICY` | No | `always` | Accepts `always` or `on-applied-action`. Overrides the flavor file when set. |

`observe` and `explain` are already non-mutating. `MR_MILCHICK_DRY_RUN` only affects `refine`.

### Slack

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `MR_MILCHICK_SLACK_ENABLED` | No | `true` | Global toggle for Slack sinks. |
| `MR_MILCHICK_SLACK_CHANNEL` | Depends on sink | None | Channel ID used by Slack app and sent in workflow payloads. |
| `MR_MILCHICK_SLACK_BASE_URL` | No | `https://slack.com/api` | Mostly useful for tests and local mocks. |
| `MR_MILCHICK_SLACK_BOT_TOKEN` | Slack app only | None | Bot token for direct Slack API posting. |
| `MR_MILCHICK_SLACK_WEBHOOK_URL` | Slack workflow only | None | Slack Workflow input webhook URL. |
| `MR_MILCHICK_SLACK_USER_MAP` | No | empty map | JSON object mapping GitLab usernames to Slack user IDs. |

Example Slack app mapping:

```bash
MR_MILCHICK_SLACK_USER_MAP='{"engineer.lady1":"U01234567","engineer.guy1":"U07654321"}'
```

If a mapping value is blank, it is ignored. In TOML, quote usernames that contain dots:

```toml
[slack_app.user_map]
"engineer.lady1" = "U01234567"
```

## Notes

- Slack notifications are optional and do not affect review planning.
- Notifications only run during real `refine` execution.
- With `notification_policy = "always"`, enabled sinks receive notifications even if the summary comment is unchanged.
- With `notification_policy = "on-applied-action"`, enabled sinks only receive notifications when the review connector actually applies the summary upsert.
- GitLab is the only implemented review connector today.
- Slack app and Slack workflow are the only implemented notification sinks today.
- Connector templates only affect GitLab and Slack output in this version. CLI output remains unchanged.

For setup examples, see [ci-quickstart.md](ci-quickstart.md). For capability validation, see [connectors-and-capabilities.md](connectors-and-capabilities.md).
