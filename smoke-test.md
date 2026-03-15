First:
`cargo test`

Then local smoke tests.


Case 1 — epic to develop without label
```bash
CI_PROJECT_ID=123 \
CI_MERGE_REQUEST_IID=456 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=epic/big-thing \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
cargo run -- observe
```

Expected:

observation tone

blocking finding printed

Case 2 — same but refine
```bash
CI_PROJECT_ID=123 \
CI_MERGE_REQUEST_IID=456 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=epic/big-thing \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="" \
cargo run -- refine
```
Expected:

blocking tone

blocking finding

process exits with error

Case 3 — epic with label
```bash
CI_PROJECT_ID=123 \
CI_MERGE_REQUEST_IID=456 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=epic/big-thing \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="0. run-tests" \
cargo run -- observe
```

Expected:

info finding

no blocking

no failure


Real MR from monorepo
```shell
CI_PROJECT_ID=412 \
CI_MERGE_REQUEST_IID=3995 \
CI_PIPELINE_SOURCE=merge_request_event \
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=epic/big-thing \
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop \
CI_MERGE_REQUEST_LABELS="0. run-tests" \
cargo run -- explain
```