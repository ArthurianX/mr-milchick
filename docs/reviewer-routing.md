# Reviewer Routing

This document describes the reviewer assignment behavior that Mr. Milchick actually uses today.

## Decision Order

Reviewer assignment is planned in this order:

1. Build an area summary from the changed files.
2. Compute a reviewer recommendation from the configured reviewer pool.
3. If a CODEOWNERS file is available and at least one section matches the merge request, replace that earlier recommendation with the CODEOWNERS assignment plan.
4. Add mandatory reviewers in front of the final recommendation.
5. Remove reviewers who are already assigned.
6. Skip assignment entirely for draft merge requests.

The implementation lives in [`crates/milchick-core/src/actions/planner.rs`](crates/milchick-core/src/actions/planner.rs).

## Area-Based Routing

The default recommendation path uses the reviewer routing config:

- reviewers can be mapped to one or more code areas
- reviewers can be marked as `fallback`
- reviewers can be marked as `mandatory`
- `max_reviewers` limits only the area-routed portion of the recommendation

Important detail:

- mandatory reviewers do not consume the `max_reviewers` budget
- if no significant area is found, a fallback reviewer may be selected
- if a significant area has no eligible reviewer left, Milchick records that in the reasoning output

The area-based selection logic lives in [`crates/milchick-core/src/domain/reviewer_routing.rs`](crates/milchick-core/src/domain/reviewer_routing.rs).

## What `max_reviewers` Actually Does

`MR_MILCHICK_MAX_REVIEWERS` is still active.

It applies when Milchick is selecting reviewers from the configured reviewer pools by code area. In that path, the code stops adding non-mandatory area reviewers once the configured limit is reached.

It does not cap:

- mandatory reviewers
- already-assigned reviewers
- CODEOWNERS-driven reviewer selection when CODEOWNERS matches are present

So if `max_reviewers=2` and there is one mandatory reviewer, Milchick can still recommend three reviewers total: one mandatory reviewer plus two area-routed reviewers.

## CODEOWNERS Override Behavior

If CODEOWNERS is enabled and a parsed CODEOWNERS file produces matched sections for the changed files, Milchick switches to the CODEOWNERS plan.

That means:

- the earlier area-based recommendation is discarded
- the CODEOWNERS planner selects reviewers needed to satisfy CODEOWNERS coverage
- mandatory reviewers are then prepended to that CODEOWNERS-derived list

This is why `max_reviewers` may appear to be ignored: once CODEOWNERS matches and takes over, the area-routing cap is no longer the governing rule.

The CODEOWNERS planner lives in [`crates/milchick-core/src/domain/codeowners/planner.rs`](crates/milchick-core/src/domain/codeowners/planner.rs).

## When `max_reviewers` Still Matters

`max_reviewers` still matters when any of these are true:

- CODEOWNERS is disabled
- no CODEOWNERS file was found
- the CODEOWNERS file could not be parsed
- the CODEOWNERS file exists but no sections match the changed files

In those cases, Milchick falls back to the normal area-based reviewer recommendation and the cap is enforced there.

## Current Practical Rule

The simplest accurate mental model is:

- area routing uses `max_reviewers`
- CODEOWNERS matching overrides area routing
- mandatory reviewers are always added on top when eligible

## Configuration Notes

Today, reviewer routing, CODEOWNERS toggles, and the reviewer cap are loaded from environment variables, not from `mr-milchick.toml`.

Relevant inputs:

- `MR_MILCHICK_REVIEWERS`
- `MR_MILCHICK_MAX_REVIEWERS`
- `MR_MILCHICK_CODEOWNERS_ENABLED`
- `MR_MILCHICK_CODEOWNERS_PATH`

See [`docs/config-reference.md`](docs/config-reference.md) for the current split between env-driven runtime config and the flavor TOML file.
