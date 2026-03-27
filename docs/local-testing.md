# Local Testing

Start with the automated test suite:

```bash
cargo test
```

Then run local smoke tests by setting the same environment variables the CI job would provide.

If you want to iterate on output and notifications without a live merge request, see [fixture-testing.md](fixture-testing.md).

## Base Notes

- `observe` and `explain` do not execute review actions.
- `refine` is the only command affected by `MR_MILCHICK_DRY_RUN`.
- In merge request mode, even `observe` and `explain` still load the merge request snapshot from GitLab, so you may need `GITLAB_TOKEN`.
- With `--fixture`, no live GitLab snapshot is loaded.
- `refine --fixture` is dry-run by default. Add `--send-notifications` if you want real Slack delivery.
- Reviewer configuration is always read from `MR_MILCHICK_REVIEWERS`.
- `MR_MILCHICK_CODEOWNERS_ENABLED` defaults to `true`.

For the full variable reference, see [config-reference.md](config-reference.md). For a CI-first setup guide, see [ci-quickstart.md](ci-quickstart.md).

## Observe A Blocking Epic Branch

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"principal-reviewer","mandatory":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_PATH=.github/CODEOWNERS \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=1 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=epic/big-thing \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- observe
```

Expected result:

- observation output
- a blocking finding about the missing `0. run-tests` label
- no external mutation

## Observe A Fixture Scenario

```bash
cargo run -- observe --fixture fixtures/review-request.toml
```

Expected result:

- findings and planned actions from the fixture
- notification previews for configured sinks
- no GitLab or Slack mutation

## Observe A Passing Epic Branch

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"principal-reviewer","mandatory":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_PATH=.github/CODEOWNERS \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=1 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=epic/big-thing \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="0. run-tests" \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- observe
```

Expected result:

- an informational finding confirming the required label
- no blocking failure
- planned follow-up actions printed for `refine`

## Refine A Blocking Merge Request

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"principal-reviewer","mandatory":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_PATH=CODEOWNERS \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=1 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="3. Ready to be merged" \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- refine
```

Expected result:

- blocking findings printed
- summary comment action planned
- command exits with an error because the merge request still violates policy

## Refine With Slack App Delivery

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"principal-reviewer","mandatory":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_ENABLED=false \
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token \
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X \
MR_MILCHICK_SLACK_USER_MAP='{"principal-reviewer":"U01234567","alice":"U07654321"}' \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=1 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="0. run-tests" \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- refine
```

Expected result:

- reviewers assigned in GitLab
- summary comment created or updated
- one compact Slack message plus one threaded Slack reply
- Slack user IDs used for mapped reviewers

## Refine With Slack Workflow Delivery

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"principal-reviewer","mandatory":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_ENABLED=false \
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/... \
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=1 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="0. run-tests" \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- refine
```

Expected result:

- reviewers assigned in GitLab
- summary comment created or updated
- one Slack workflow trigger sent with `mr_milchick_talks_to`, `mr_milchick_says`, and `mr_milchick_says_thread`

## Refine A Fixture And Send Slack Notifications

```bash
MR_MILCHICK_SLACK_ENABLED=true \
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-your-slack-bot-token \
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X \
cargo run -- refine --fixture fixtures/review-request.toml --send-notifications
```

Expected result:

- no GitLab reads or writes
- notification previews printed before execution
- one Slack root message and one Slack thread message when Slack app is enabled
- the same message templates used in real execution

## Explain Routing And CODEOWNERS

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"principal-reviewer","mandatory":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_PATH=CODEOWNERS \
CI_PROJECT_ID=1 \
CI_MERGE_REQUEST_IID=1 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="3. Ready to be merged" \
GITLAB_TOKEN=your-gitlab-token \
cargo run -- explain
```

Expected result:

- findings and planned actions
- summary comment preview
- changed file count and merge request details
- CODEOWNERS matches and routing reasons

## Helpful Extras

- Use `cargo run -- version` to confirm the current build surface before a smoke test.
- Set `MR_MILCHICK_DRY_RUN=true` with `refine` if you want an execution-shaped report without live GitLab or Slack writes.
- `MR_MILCHICK_SLACK_BASE_URL` is available for local mocks and connector tests; the production default is `https://slack.com/api`.
- Use `--fixture` whenever you want to iterate on templates and notifications before testing against a real merge request.
