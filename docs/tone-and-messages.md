# Tone And Messages

Mr Milchick treats tone as part of the product surface, but the tone system is deterministic and separated from policy logic.

## How Tone Is Chosen

Tone selection is based on a stable hash of:

- project ID
- merge request IID, or `no-mr` when unavailable
- tone category

That seed is used to choose one message from the static registry for the requested category. The same merge request therefore gets the same message for the same category across runs.

## Tone Categories

The current registry contains these categories:

- `Observation`
- `Refinement`
- `Resolution`
- `Blocking`
- `Praise`
- `ReviewRequest`
- `NoAction`
- `ReviewerAssigned`

Messages are stored as static arrays in the tone registry and selected by index from the stable hash.

## Where Tone Appears

Tone shows up in three main places:

- the command-line output shown during execution
- the rendered GitLab summary message
- Slack notification messages built during reviewer assignment

With connector templates enabled, those connector messages may now reference the selected tone through placeholders such as `{{tone_message}}`, `{{tone_category}}`, `{{closing_tone_message}}`, and `{{closing_tone_category}}`.

Tone does not change rule evaluation, reviewer selection, CODEOWNERS behavior, or blocking policy. It only changes the human-facing phrasing wrapped around those outcomes.

## Why The Registry Matters

Because the selection is index-based, message ordering is part of the behavior. Reordering existing entries changes which message older merge requests would receive for the same seed.

Appending new lines is the safe change when you want to expand the tone set without shifting existing deterministic outputs.

## Operational Goal

The goal is not randomness and not roleplay. The goal is predictable messaging that softens strict automation without making the result ambiguous.

For the command and execution flow around those messages, see [architecture.md](architecture.md).
For template authoring, see [message-templates.md](message-templates.md).
