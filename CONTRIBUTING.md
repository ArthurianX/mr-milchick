

# Contributing to Mr Milchick

Thank you for considering contributing.

Mr Milchick is not just a CLI tool.
It is a deterministic governance system designed to operate safely inside CI pipelines.

This means contributions must prioritize:

- predictability
- clarity
- safety
- architectural discipline
- minimal operational risk

If a change makes the system more “clever” but less predictable, it will likely be rejected.

---

## Core Principles

### Determinism over cleverness
Given identical inputs, the system must produce identical outputs.

Avoid:
- hidden randomness
- time‑dependent behavior
- implicit external state

### Policy isolation
Policy evaluation must remain separate from execution.

Do not:
- mix GitLab mutation logic inside rule evaluation
- introduce side effects in planning phases

### CI safety first
This tool runs inside pipelines.
A mistake can block teams.

Changes that affect:
- pipeline failure logic
- reviewer assignment
- comment behavior
must be extremely well justified.

### Tone is architecture
User‑facing messages are part of system design.

Avoid:
- humor spikes
- tone randomness
- informal language drift

Tone must remain:
- calm
- structured
- slightly institutional

---

## Contribution Types

We welcome:

- bug fixes
- rule engine improvements
- performance optimizations
- deterministic routing improvements
- test coverage expansion
- documentation improvements
- GitLab API robustness work

We are cautious about:

- large architectural rewrites
- introducing new external dependencies
- adding runtime configuration complexity
- speculative features without real governance value

---

## Development Workflow

1. Fork the repository
2. Create a focused branch
3. Make minimal, well‑scoped changes
4. Add tests where behavior changes
5. Update CHANGELOG if user‑visible behavior is affected
6. Open a merge request with clear rationale

---

## Code Guidelines

- Prefer explicit types over inference in domain logic
- Keep modules single‑responsibility
- Avoid “god structs” and “god modules”
- Favor composition over inheritance‑style patterns
- Maintain clear separation between:
  - context
  - rules
  - planning
  - execution
  - rendering

Rust‑specific expectations:

- no unnecessary cloning in hot paths
- avoid premature async complexity
- prefer pure functions for rule logic
- treat error modeling as part of domain design

---

## Testing Expectations

New logic should include:

- deterministic unit tests
- edge‑case handling tests
- planner decision validation tests when applicable

Future integration tests will simulate real CI contexts.
Design code to be testable in isolation.

---

## Behavioral Changes & Versioning

If your change affects:

- pipeline pass/fail outcomes
- reviewer assignment logic
- tone classification
- rule evaluation semantics

It may be considered **breaking**, even if no public API changes.

Discuss such changes before implementation.

---

## Communication Style

In code, comments, and MR discussions:

- be precise
- be calm
- be explicit about trade‑offs
- avoid rhetorical arguments

This project optimizes for long‑term institutional clarity, not short‑term velocity.

---

## Final Note

Mr Milchick is intended to become infrastructure.

Contribute accordingly.