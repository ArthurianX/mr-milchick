# Reviewer Routing

This page describes the reviewer assignment behavior the current planner implements.

## Decision Order

Reviewer assignment happens in this order:

1. Classify changed files into code areas.
2. Build an area-based reviewer recommendation from `MR_MILCHICK_REVIEWERS`.
3. If CODEOWNERS is enabled and at least one section matches the merge request, replace that area-based recommendation with a CODEOWNERS assignment plan.
4. Prepend mandatory reviewers to the final recommendation.
5. Skip already-assigned reviewers when building the action plan.
6. Defer assignment completely if the merge request is draft.

The merge request author is excluded from both area-based routing and CODEOWNERS-derived assignment.

## Area-Based Routing

Area-based routing is driven by `MR_MILCHICK_REVIEWERS`. Each entry can describe:

- one or more `areas`
- a `fallback` reviewer
- a `mandatory` reviewer

Example:

```bash
MR_MILCHICK_REVIEWERS='[
  {"username":"milchick-duty","fallback":true},
  {"username":"principal-reviewer","mandatory":true},
  {"username":"alice","areas":["frontend","packages"]},
  {"username":"carol","areas":["backend"]},
  {"username":"grace","areas":["devops"]}
]'
```

Changed files are classified into these code areas:

| Area | Config keys | Typical path matches |
| --- | --- | --- |
| `frontend` | `frontend`, `apps`, `ui` | `apps/frontend`, `.tsx`, `.jsx`, `/ui/` |
| `backend` | `backend`, `api` | `services/`, `/backend/`, `.rs`, `.go` |
| `packages` | `packages`, `shared`, `bootstrap` | `libs/`, `/shared/` |
| `devops` | `devops`, `ops`, `infrastructure`, `scripts`, `patches`, `proxy` | `.gitlab`, `docker`, `k8s`, `infra` |
| `documentation` | `documentation`, `docs`, `reports` | `docs/`, `.md` |
| `tests` | `tests`, `test`, `qa` | paths containing `test` or `spec` |
| `unknown` | `unknown` | everything else |

The planner sorts areas by changed-file count and then chooses reviewers from the configured pools in that order.

## Mandatory And Fallback Reviewers

Mandatory reviewers are added before normal area routing and are kept even if CODEOWNERS later replaces the area-based recommendation.

Fallback reviewers are only used when:

- no significant area is detected
- no eligible area reviewer can be selected at all

Fallback reviewers do not replace matched CODEOWNERS sections.

## What `MR_MILCHICK_MAX_REVIEWERS` Does

`MR_MILCHICK_MAX_REVIEWERS` limits only the non-mandatory area-routed portion of the recommendation.

It does not limit:

- mandatory reviewers
- reviewers already assigned on the merge request
- CODEOWNERS-driven assignments once CODEOWNERS has taken over

If `MR_MILCHICK_MAX_REVIEWERS=2` and there is one mandatory reviewer, the planner can still recommend three total reviewers: one mandatory reviewer plus two area-routed reviewers.

## CODEOWNERS Override

If CODEOWNERS is enabled and the parsed file matches at least one section for the changed files, the planner switches from area routing to CODEOWNERS-driven coverage planning.

That means:

- the earlier area-based recommendation is discarded
- the CODEOWNERS planner selects additional reviewers needed to satisfy matched sections
- current reviewers already on the merge request count toward coverage
- mandatory reviewers are prepended to the CODEOWNERS-derived assignments

If matched sections still cannot be fully covered, Mr Milchick records a warning finding and includes the uncovered sections in the explanation output.

## Drafts And Existing Reviewers

Draft merge requests never receive an `AssignReviewers` action. If a recommendation exists, the planner instead records an informational finding saying assignment is deferred for draft state.

For non-draft merge requests, recommended reviewers already present in the MR are filtered out before the action plan is built. If nothing is left to add, the planner records an informational finding that all recommended reviewers are already assigned.

## Practical Mental Model

The simplest accurate summary is:

- area routing is the default path
- CODEOWNERS overrides that path when matches exist
- mandatory reviewers always stay on top
- draft state defers assignment

For configuration details, see [config-reference.md](config-reference.md). For the end-to-end command flow, see [architecture.md](architecture.md).
