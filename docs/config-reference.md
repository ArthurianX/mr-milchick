# Configuration Reference

Mr Milchick now resolves application configuration from one place:

1. compiled capabilities from Cargo features
2. an optional `mr-milchick.toml` file
3. a small env layer for secrets and config-path selection

CI review context is separate. `context/` still reads `CI_*`, `GITHUB_*`, and the review-context override vars. `config/` does not.

## Sources And Precedence

- Cargo features decide which platform connector and notification sinks exist in the binary.
- `mr-milchick.toml` is the canonical source for non-secret runtime configuration.
- TOML supports env interpolation before parsing:
  - `${VAR}` for required env vars
  - `${VAR:-default}` for optional env vars with a fallback
- Env is limited to:
  - `MR_MILCHICK_CONFIG_PATH`
  - `GITLAB_TOKEN`
  - `GITHUB_TOKEN`
  - `MR_MILCHICK_SLACK_BOT_TOKEN`
  - `MR_MILCHICK_SLACK_WEBHOOK_URL`
- Removed env-driven runtime config is rejected with an error. That includes the old reviewer, CODEOWNERS, dry-run, Slack, LLM, and `MR_MILCHICK_FLAVOR_PATH` variables.

## Default File

- Default path: `mr-milchick.toml`
- Override path: `MR_MILCHICK_CONFIG_PATH`

If the file is missing, Milchick uses defaults:

- platform kind: compiled platform
- platform base URL: GitLab `https://gitlab.com/api/v4`, GitHub `https://api.github.com`
- notification policy: `always`
- dry run: `false`
- reviewers: empty list, `max_reviewers = 2`
- CODEOWNERS: enabled with auto-discovery
- inference: disabled
- Slack sinks: disabled
- templates: built-in defaults

## TOML Interpolation

Mr Milchick expands interpolation markers before it hands the file to the TOML parser.

- `${VAR}` requires `VAR` to be present and non-empty.
- `${VAR:-default}` falls back to `default` when `VAR` is missing or empty.
- Interpolation happens before TOML parsing, so scalar values can be injected directly:
  - `enabled = ${MILCHICK_LLM_ENABLED:-false}`
- String values should still be quoted in TOML:
  - `model_path = "${CI_PROJECT_DIR}/models/${MILCHICK_LLM_MODEL}"`

This syntax is intentionally Milchick-specific and is not part of standard TOML. Generic TOML tooling and IDE plugins may flag it even though Milchick resolves it successfully at runtime.

## Example Config

```toml
[platform]
kind = "gitlab"
base_url = "https://gitlab.com/api/v4"

[execution]
dry_run = false
notification_policy = "always"

[reviewers]
max_reviewers = 2

[[reviewers.definitions]]
username = "milchick-duty"
fallback = true

[[reviewers.definitions]]
username = "principal-reviewer"
mandatory = true

[[reviewers.definitions]]
username = "alice"
areas = ["frontend", "packages"]

[[reviewers.definitions]]
username = "carol"
areas = ["backend"]

[codeowners]
enabled = true

[inference]
enabled = ${MR_MILCHICK_LLM_ENABLED:-false}
model_path = "${CI_PROJECT_DIR}/models/review.gguf"
timeout_ms = 15000
max_patch_bytes = 32768
context_tokens = 4096
trace = false

[notifications.slack_app]
enabled = true
channel = "C0ALY38CW3X"
base_url = "https://slack.com/api"

[notifications.slack_app.user_map]
"principal-reviewer" = "U01234567"
"alice" = "U07654321"

[notifications.slack_workflow]
enabled = false
channel = "C0ALY38CW3X"

[notifications.pipeline_status]
enabled = true
fail_pipeline_on_failed = true
search_root = "${CI_PROJECT_DIR}"

[templates.gitlab]
summary = """## {{summary_title}}

{{tone_message}}

{{findings_block}}

{{actions_block}}

_{{closing_tone_message}}_"""
```

## TOML Surface

### `[platform]`

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `kind` | No | compiled platform | Must match the compiled binary if present. |
| `base_url` | No | platform default | GitLab or GitHub API base URL. |

### `[platform.gitlab]`

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `all_pipelines_pass_label` | No | none | Adds this GitLab MR label only when parsed `milchick-status` entries exist and every entry is `Passed`. |

If `all_pipelines_pass_label` is configured but Milchick does not find any `*/milchick-status/*.json` data, Milchick emits a warning and does not plan the label action.

### `[execution]`

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `dry_run` | No | `false` | Only affects `refine`. |
| `notification_policy` | No | `always` | `always` or `on-applied-action`. |

### `[reviewers]` and `[[reviewers.definitions]]`

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `max_reviewers` | No | `2` | Caps only non-mandatory area-routed reviewers. |
| `username` | Yes | none | Reviewer username. |
| `areas` | No | `[]` | Area keys such as `frontend`, `backend`, `packages`, `devops`, `docs`, `tests`, `unknown`. |
| `fallback` | No | `false` | Marks fallback reviewer. |
| `mandatory` | No | `false` | Always prepend this reviewer when eligible. |

### `[codeowners]`

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `enabled` | No | `true` | Enables CODEOWNERS planning. |
| `path` | No | auto-discovery | Overrides lookup path. |

Auto-discovery order:

- `CODEOWNERS`
- `.github/CODEOWNERS`
- `.gitlab/CODEOWNERS`
- `.CODEOWNERS`

### `[inference]`

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `enabled` | No | `false` | Enables advisory local review. |
| `model_path` | No | none | Required when `enabled = true`. |
| `timeout_ms` | No | `15000` | Must be greater than zero. |
| `max_patch_bytes` | No | `32768` | Must be greater than zero. |
| `context_tokens` | No | `4096` | Must be greater than zero. |
| `trace` | No | `false` | Prints detailed inference output in CLI flows. |

### `[notifications.slack_app]`

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `enabled` | No | `false` | Must be `true` to activate the sink. |
| `channel` | No | none | Default Slack destination. |
| `base_url` | No | `https://slack.com/api` | Useful for tests and local mocks. |

`[notifications.slack_app.user_map]` is optional and maps GitLab or GitHub usernames to Slack user IDs.

### `[notifications.slack_workflow]`

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `enabled` | No | `false` | Must be `true` to activate the sink. |
| `channel` | No | none | Sent in workflow payloads as `mr_milchick_talks_to`. |

### `[notifications.pipeline_status]`

| Field | Required | Default | Notes |
| --- | --- | --- | --- |
| `enabled` | No | `false` | Enables Slack notification enrichment from local status JSON files. |
| `fail_pipeline_on_failed` | No | `false` | Fails Milchick's pipeline when any parsed `milchick-status` entry is explicitly `Failed`. Missing data only warns. |
| `search_root` | No | current working directory | Milchick recursively scans below this path for `*/milchick-status/*.json`. |

This feature is optional and primarily intended for internal CI setups that already emit job result snapshots into the workspace before Milchick runs.

It also powers the optional GitLab success-label flow under `[platform.gitlab]` and the optional pipeline failure gate. Without parsed status entries, Milchick will not attempt the label action and will only warn instead of failing the pipeline.

Milchick accepts a tolerant JSON shape and currently looks for fields such as:

- label/name: `label`, `name`, `job`, `task`, `step`
- state: `status`, `state`, `success`, `passed`, `ok`
- detail: `summary`, `message`, `detail`, `details`, `description`

Example:

```json
{
  "job": "unit_tests",
  "status": "success",
  "summary": "All tests passed",
  "blocking": true,
  "job_url": "https://gitlab.example.com/.../jobs/123",
  "pipeline_url": "https://gitlab.example.com/.../pipelines/456"
}
```

Unknown extra fields are ignored. Today `blocking`, `job_url`, and `pipeline_url` are preserved in the file format but are not rendered into notifications by default.

### `[templates.*]`

Template overrides stay field-by-field and keep built-in defaults when omitted.

- `[templates.gitlab].summary`
- `[templates.github].summary`
- `[templates.slack_app].first_root`
- `[templates.slack_app].first_thread`
- `[templates.slack_app].update_root`
- `[templates.slack_app].update_thread`
- `[templates.slack_workflow].first_title`
- `[templates.slack_workflow].first_thread`
- `[templates.slack_workflow].update_title`
- `[templates.slack_workflow].update_thread`

Useful notification placeholders include:

- `pipeline_status_block`
- `pipeline_status_count`
- `pipeline_status_passed_count`
- `pipeline_status_failed_count`
- `pipeline_status_unknown_count`

Template placeholder validation still happens at render time. Invalid placeholders warn and fall back to the built-in field template.

## Supported Env Vars

### Application Config

| Variable | Required | Purpose |
| --- | --- | --- |
| `MR_MILCHICK_CONFIG_PATH` | No | Alternate config file path. |
| `GITLAB_TOKEN` | GitLab live runs | GitLab API token. |
| `GITHUB_TOKEN` | GitHub live runs | GitHub API token. |
| `MR_MILCHICK_SLACK_BOT_TOKEN` | Slack app only | Slack bot token. |
| `MR_MILCHICK_SLACK_WEBHOOK_URL` | Slack workflow only | Slack Workflow webhook URL. |

### Review Context

These still belong to the CI context layer, not the app config layer:

| Variable | Notes |
| --- | --- |
| `CI_PROJECT_ID`, `CI_MERGE_REQUEST_IID`, `CI_PIPELINE_SOURCE`, `CI_MERGE_REQUEST_SOURCE_BRANCH_NAME`, `CI_MERGE_REQUEST_TARGET_BRANCH_NAME`, `CI_MERGE_REQUEST_LABELS` | GitLab review context |
| `GITHUB_ACTIONS`, `GITHUB_EVENT_NAME`, `GITHUB_EVENT_PATH`, `GITHUB_REPOSITORY`, `GITHUB_HEAD_REF`, `GITHUB_BASE_REF` | GitHub review context |
| `MR_MILCHICK_PROJECT_KEY`, `MR_MILCHICK_REVIEW_ID`, `MR_MILCHICK_PIPELINE_SOURCE`, `MR_MILCHICK_SOURCE_BRANCH`, `MR_MILCHICK_TARGET_BRANCH`, `MR_MILCHICK_LABELS` | Explicit review-context overrides |

## Notes

- `observe` and `explain` are already non-mutating. `dry_run` only changes `refine`.
- Slack notifications are planned from resolved config but only sent during real `refine`.
- Config validation is strict. Unknown TOML fields fail parsing, and legacy app-config env vars fail startup.
- Platform and sink configuration must agree with compiled capabilities.
