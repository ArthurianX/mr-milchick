# Local Testing

Start with the unit test suite:

```bash
cargo test
```

Then run one or more local smoke tests by setting the GitLab CI environment variables manually.

## Observe: epic to `develop` without label

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_PATH=.github/CODEOWNERS \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=epic/big-thing \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
cargo run -- observe
```

Expected:

- observation tone
- blocking finding printed

## Refine: epic to `develop` without label

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_PATH=CODEOWNERS \
MR_MILCHICK_DRY_RUN=true \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=epic/big-thing \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
cargo run -- refine
```

Expected:

- blocking tone
- blocking finding
- structured Mr Milchick summary comment planned
- process exits with error

## Observe: epic to `develop` with label

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_PATH=.github/CODEOWNERS \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=epic/big-thing \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="0. run-tests" \
cargo run -- observe
```

Expected:

- info finding
- no blocking
- no failure
- structured Mr Milchick summary comment planned

## Explain: real MR from the monorepo

```bash
MR_MILCHICK_REVIEWERS='[{"username":"milchick-duty","fallback":true},{"username":"alice","areas":["frontend"]},{"username":"carol","areas":["backend"]}]' \
MR_MILCHICK_CODEOWNERS_PATH=CODEOWNERS \
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=4009 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/ERD-000000/test-mr-milchick \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="3. Ready to be merged" \
cargo run -- explain
```

Expected:

- decision explanation
- structured summary comment preview
- changed files listed
- CODEOWNERS matches listed
- no GitLab mutation because `explain` does not execute the action plan

## Notes

- `observe` and `explain` do not execute the action plan, so `MR_MILCHICK_DRY_RUN` is only relevant for `refine`.
- In merge request mode, `observe` and `explain` still fetch the MR snapshot from GitLab, so you may still need `GITLAB_TOKEN`.
- `MR_MILCHICK_REVIEWERS` accepts a JSON array of reviewer capability objects, for example `{"username":"alice","areas":["frontend","packages"]}`.
- `MR_MILCHICK_CODEOWNERS_ENABLED` defaults to `true`. Set it to `false` to disable ownership-based routing completely.
