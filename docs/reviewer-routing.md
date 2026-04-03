# Reviewer Routing

Reviewer routing now comes from `mr-milchick.toml`, not from runtime env JSON.

## Example

```toml
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
```

## Selection Order

1. Build an area summary from changed files.
2. Recommend non-mandatory reviewers from matching areas.
3. Prepend mandatory reviewers.
4. If no area match exists, fall back to reviewers marked `fallback = true`.
5. If CODEOWNERS is enabled and matches, CODEOWNERS planning can replace area-based routing.

The merge request author is excluded from both area-based and CODEOWNERS-derived assignment.

## `max_reviewers`

`max_reviewers` limits only the non-mandatory area-routed portion.

If `max_reviewers = 2` and there is one mandatory reviewer, Milchick can still recommend three total reviewers:

- one mandatory reviewer
- two area-routed reviewers

## Supported Area Keys

- `frontend`, `apps`, `ui`
- `backend`, `api`
- `packages`, `shared`, `bootstrap`
- `devops`, `ops`, `infrastructure`, `scripts`, `patches`, `proxy`
- `documentation`, `docs`, `reports`
- `tests`, `test`, `qa`
- `unknown`

For CODEOWNERS configuration, see [config-reference.md](config-reference.md).
