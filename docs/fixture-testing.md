# Fixture Testing

Fixture mode lets you run `observe`, `explain`, and `refine` without a live GitLab merge request.

Instead of loading CI context and a remote snapshot, Mr Milchick reads a local TOML fixture and runs the same downstream rendering and notification logic against that synthetic review state.

## What Fixture Mode Is For

Use `--fixture` when you want to:

- iterate on Slack notifications
- test message templates locally
- preview a summary comment without GitLab access
- reproduce a scenario for demos or debugging

## Safety Model

- `observe --fixture` never sends anything
- `explain --fixture` never sends anything
- `refine --fixture` is safe by default and uses dry-run execution
- `refine --fixture --send-notifications` can send Slack notifications
- fixture mode never mutates GitLab review state

## Sample Fixtures

The repo now ships with starter fixtures in [`fixtures/`](/Users/arthur.kovacs/Work/mr-milchick/fixtures):

- [`review-request.toml`](/Users/arthur.kovacs/Work/mr-milchick/fixtures/review-request.toml): a clean review request that produces reviewer notifications
- [`blocking-refine.toml`](/Users/arthur.kovacs/Work/mr-milchick/fixtures/blocking-refine.toml): a blocking scenario that fails the run
- [`no-findings-update.toml`](/Users/arthur.kovacs/Work/mr-milchick/fixtures/no-findings-update.toml): a no-findings scenario for summary/comment output

## Quick Commands

Preview the planned actions and notification bodies:

```bash
cargo run -- observe --fixture fixtures/review-request.toml
```

Print the summary comment preview, snapshot details, routing details, and notification previews:

```bash
cargo run -- explain --fixture fixtures/review-request.toml
```

Run the refine pipeline safely with no external delivery:

```bash
cargo run -- refine --fixture fixtures/review-request.toml
```

Actually send Slack notifications from a fixture:

```bash
MR_MILCHICK_SLACK_ENABLED=true \
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token \
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X \
cargo run -- refine --fixture fixtures/review-request.toml --send-notifications
```

Send through Slack workflow instead:

```bash
MR_MILCHICK_SLACK_ENABLED=true \
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/... \
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X \
cargo run -- refine --fixture fixtures/review-request.toml --send-notifications
```

## How Fixture Mode Interacts With Templates

Fixture mode uses the same template loading path as normal execution:

- built-in defaults still apply
- `mr-milchick.toml` overrides still apply
- invalid template fields still warn and fall back to defaults

That makes fixture mode a good way to edit `[templates.*]` sections and immediately see the result.

## Fixture File Shape

Example:

```toml
project_id = "412"
merge_request_iid = "3995"
pipeline_source = "merge_request_event"

[merge_request]
title = "Frontend adjustments"
url = "https://gitlab.example.com/group/project/-/merge_requests/3995"
author = "arthur"
source_branch = "feat/intentional-cleanup"
target_branch = "develop"
labels = ["frontend"]
existing_reviewers = ["principal-reviewer"]
is_draft = false
default_branch = "develop"

[[merge_request.changed_files]]
path = "apps/frontend/button.tsx"
change_type = "modified"
additions = 24
deletions = 8

[[findings]]
severity = "warning"
message = "Missing label."

[[actions]]
kind = "assign-reviewers"
reviewers = ["bob"]
```

## Supported Fields

Top-level:

- `project_id`
- `merge_request_iid`
- `pipeline_source`

`[merge_request]`:

- `title`
- `url`
- `author`
- `description`
- `source_branch`
- `target_branch`
- `labels`
- `existing_reviewers`
- `changed_files`
- `is_draft`
- `default_branch`

`[[merge_request.changed_files]]`:

- `path`
- `change_type`
- `additions`
- `deletions`

Supported `change_type` values:

- `added`
- `modified`
- `deleted`
- `renamed`
- `unknown`

`[[findings]]`:

- `severity`
- `message`

Supported `severity` values:

- `info`
- `warning`
- `blocking`

`[[actions]]`:

- `kind`
- `reviewers`
- `labels`
- `reason`

Supported `kind` values:

- `assign-reviewers`
- `add-labels`
- `remove-labels`
- `fail-pipeline`

## Testing Guidelines

- Start with `observe --fixture` while iterating on templates.
- Use `explain --fixture` when you want the GitLab summary preview plus routing context.
- Use `refine --fixture` before `--send-notifications` to confirm the message shape.
- Keep one fixture per scenario you care about:
  - review request
  - blocking failure
  - no findings
  - multiple reviewers
  - draft merge request
- If a notification preview looks wrong, fix the fixture or template first before trying a live MR.

## Practical Workflow

1. Edit [`mr-milchick.toml`](/Users/arthur.kovacs/Work/mr-milchick/mr-milchick.toml) templates.
2. Run `cargo run -- observe --fixture fixtures/review-request.toml`.
3. Run `cargo run -- explain --fixture fixtures/review-request.toml` if you want more context.
4. When the rendered text looks right, add Slack env vars and run `cargo run -- refine --fixture fixtures/review-request.toml --send-notifications`.
5. Iterate on the fixture and template until the delivered message feels right.
