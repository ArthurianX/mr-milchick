# Reviewer Routing

Reviewer routing now comes from `mr-milchick.toml`, not from runtime env JSON.

## Example

```toml
[reviewers]
max_reviewers = 2
assignment_mode = "minimal"

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
```

## Selection Order

1. Build an area summary from changed files.
2. Recommend non-mandatory reviewers from matching areas.
3. Prepend mandatory reviewers.
4. If no area match exists, fall back to reviewers marked `fallback = true`.
5. If CODEOWNERS is enabled and matches, CODEOWNERS planning can replace area-based routing.

The merge request author is excluded from both area-based and CODEOWNERS-derived assignment.

## `assignment_mode`

`assignment_mode` controls **how many** of the eligible reviewers get assigned. It never changes how many approvals a merge request requires.

| Mode | Behaviour |
| --- | --- |
| `"minimal"` (default) | One reviewer per matching area. |
| `"all-eligible"` | Every eligible reviewer in every matching area. |

The same setting exists under `[codeowners]` and governs CODEOWNERS-derived assignment. The two are independent — set both if CODEOWNERS routing is enabled, because a CODEOWNERS match replaces area-based routing entirely.

```toml
[reviewers]
max_reviewers = 0
assignment_mode = "all-eligible"

[codeowners]
enabled = true
assignment_mode = "all-eligible"
```

Under `minimal`, CODEOWNERS planning solves for the smallest set of reviewers that satisfies each section's `[N]` approval demand. A section written `[Roulette][2]` with four eligible owners therefore yields two assigned reviewers. Under `all-eligible`, all four are assigned while the section still requires only two approvals to merge.

## `max_reviewers`

`max_reviewers` limits only the non-mandatory area-routed portion.

If `max_reviewers = 2` and there is one mandatory reviewer, Milchick can still recommend three total reviewers:

- one mandatory reviewer
- two area-routed reviewers

Set `max_reviewers = 0` to remove the ceiling entirely. This is required for `assignment_mode = "all-eligible"` to assign a whole team — otherwise the cap silently truncates the list. The cap applies to CODEOWNERS-derived reviewers too.

## Supported Area Keys

- `frontend`, `apps`, `ui`
- `backend`, `api`
- `packages`, `shared`, `bootstrap`
- `devops`, `ops`, `infrastructure`, `scripts`, `patches`, `proxy`
- `documentation`, `docs`, `reports`
- `tests`, `test`, `qa`
- `unknown`

For CODEOWNERS configuration, see [config-reference.md](config-reference.md).
