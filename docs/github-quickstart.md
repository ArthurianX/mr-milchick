# GitHub Quickstart

This guide explains how to integrate Mr Milchick into GitHub Actions workflows using a source-built binary. It covers prerequisites, configuration, security considerations, and rollout strategies.

## Prerequisites

- **Linux Runner**: The release binaries are built for `x86_64-unknown-linux-musl`. Ensure your runner is Linux-based.
- **Rust Toolchain**: You need a stable Rust toolchain to build the binary if you are not using pre-built artifacts. Use `actions-rs/toolchain@v1` with `toolchain: stable` or pin a specific commit.
- **Source Build Latency**: Building from source adds latency to CI jobs. For faster execution, consider caching the build or using published release assets (once available and pinned).

## Immutable Action Commit Pin

The GitHub Action for Mr Milchick must be pinned to a specific commit SHA that contains the root `action.yml`. 

**Important**: The historical release `v3.0.1` does **not** contain the new action interface. Do not use it. You must pin to a commit containing `action.yml` (e.g., on `develop` or `master`).

```yaml
uses: ArthurianX/mr-milchick@REPLACE_WITH_RELEASE_COMMIT
```

Replace `REPLACE_WITH_RELEASE_COMMIT` with an immutable commit SHA. Never invent a SHA; use one from your repository's history.

## Token Permissions

The workflow requires specific permissions:
- `contents: read`: To checkout the trusted base SHA.
- `pull-requests: write`: To assign reviewers and post comments.
- `issues: write`: To apply labels.

Use `${{ secrets.GITHUB_TOKEN }}` for authentication. This token is automatically provided by GitHub Actions.

## Required Workflow Checks

Ensure your repository has branch protection rules enabled. Mr Milchick complements CODEOWNERS and branch protection but does not enforce approval counts itself (GitHub snapshot counts are absent in the API response used).

## Configuration

Create a `mr-milchick.github.toml` file at the root of your repository. You can copy the example from `examples/github/mr-milchick.toml`. A minimal GitHub-oriented example:

```toml
[platform]
kind = "github"

[[areas.definitions]]
key = "frontend"
paths = ["apps/frontend/**"]
risk = "medium"

[[areas.definitions]]
key = "packages"
paths = ["packages/**"]
risk = "medium"

[[areas.definitions]]
key = "ci"
paths = [".github/**", "scripts/**"]
risk = "high"

[reviewers]
max_reviewers = 1

# Replace REPLACE_WITH_COLLABORATOR with a real eligible collaborator before enabling live workflow.
[[reviewers.definitions]]
username = "REPLACE_WITH_COLLABORATOR"
fallback = true
```

**Reviewer Example**: Replace `REPLACE_WITH_COLLABORATOR` with a valid GitHub username. Ensure the user has write access to the repository.

## Security: Fork Processing

For fork PRs, use `pull_request_target` to access secrets and write permissions. 

**Critical**: Do **not** checkout or execute code from the PR head (`github.event.pull_request.head.sha`). Instead, checkout the trusted base SHA (`github.event.pull_request.base.sha`) to ensure that only trusted configuration files (config, CODEOWNERS, templates) are used.

```yaml
on:
  pull_request_target:
    types:
      - opened
      - synchronize
      - reopened
      - edited
      - ready_for_review
      - converted_to_draft

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout trusted base SHA
        uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262
        with:
          persist-credentials: false
          ref: ${{ github.event.pull_request.base.sha }}

      - name: Observe pull request
        uses: ArthurianX/mr-milchick@REPLACE_WITH_RELEASE_COMMIT
        with:
          command: observe
          config-path: mr-milchick.github.toml
          token: ${{ secrets.GITHUB_TOKEN }}
```

## Rollout Strategy

1. **Observe**: Start with `observe` to validate intake requirements and apply labels without assigning reviewers.
2. **Refine**: Switch to `refine` for live reviewer assignment and governance summary sync.
3. **Dry-Run**: Use `[execution] dry_run = true` in your config during the refine phase to preview actions without mutating the PR.

## Complementary Features

Mr Milchick complements CODEOWNERS and branch protection by providing deterministic reviewer routing and governance summaries. It does not replace branch protection rules but enhances visibility into review status.

## Example Workflow

See [`examples/github/milchick.yml`](../examples/github/milchick.yml) for a complete workflow example including permissions, concurrency, and refine steps.