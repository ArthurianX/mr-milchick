# Mr. Milchick

A pleasantly unsettling steward for GitLab merge requests.

Mr. Milchick observes.
Mr. Milchick refines.
Mr. Milchick ensures structural harmony.

This project is a Rust-based CLI tool designed to run inside GitLab CI pipelines and enforce merge request policies through calm, polite, and deeply committed automation.

It is not a bot.
It is not a service.
It is not a platform.

It is a binary that cares.

---

## Purpose

Mr. Milchick exists to:

- enforce merge request workflow policies
- assign reviewers intelligently
- validate labels and branch conventions
- guide developers toward structural compliance
- reduce human coordination overhead
- introduce a mild but controlled sense of existential discomfort

This tool is designed for organizations where:

- GitLab apps or integrations are restricted
- external bots are difficult to approve
- pipeline-native automation is preferred
- workflow governance must be deterministic and auditable

---

## Philosophy

Mr. Milchick operates under the following principles:

### Deterministic behavior
Automation must be predictable.  
The same merge request should produce the same outcomes.

### Policy as code
Workflow rules live in version control and evolve through review.

### Calm enforcement
Strict automation does not need to be hostile.  
Politeness increases compliance.

### Structured tone
Human acceptance of automation is emotional.  
Tone is part of system design.

### Minimal infrastructure
No servers.  
No daemons.  
No long-lived tokens outside CI.

Only the pipeline.  
Only the process.

---

## Command Model

Mr. Milchick currently supports:
```shell
mr-milchick observe
mr-milchick refine
mr-milchick explain
```

### observe

Performs a dry-run evaluation of the merge request.

- Reads GitLab CI context
- Fetches MR metadata (future)
- Evaluates workflow rules
- Produces an action plan
- Does not mutate GitLab state

### refine

Executes the approved action plan.

May:

- assign reviewers
- post comments
- enforce policy gates
- fail the pipeline when required

### explain

Provides detailed reasoning behind decisions.

This command is intended for debugging policy behavior and understanding automation outcomes.

---

## Tone System

Mr. Milchick communicates using structured tonal categories:

- Observation
- Refinement Opportunity
- Pleasant Resolution
- Blocking Experience
- Praise (future)

Tone selection is:

- deterministic per merge request (default)
- configurable in future versions
- designed to be unsettling but professional

The goal is not humor.

The goal is **compliant emotional atmosphere**.

---


## Architecture Overview

```shell
CI Context + GitLab Data
↓
Rules Engine
↓
Action Plan
↓
Execution Layer
↓
Structured Output
```

Key architectural boundaries:

### context/
Handles CI environment parsing and normalization.

### gitlab/
Responsible for API communication and DTO mapping.

### rules/
Pure policy logic. No side effects.

### actions/
Executes GitLab mutations safely and idempotently.

### tone/
Narrative engine for user-facing communication.

### output/
Human-readable reporting for CI logs.

---

## Why Rust

This project uses Rust because:

- static binaries simplify CI integration
- strong typing reduces workflow automation risk
- async performance is suitable for API-heavy pipelines
- ownership model encourages explicit design
- long-term maintainability matters

This is not a scripting experiment.  
This is intended to be durable infrastructure.

---

## Development Status

Current phase:

**Chapter 1 — CLI Skeleton + Tone Engine**

Upcoming:

- Strongly typed CI context
- GitLab API client
- Rule engine v1
- Action execution layer
- Deterministic tone selection
- Policy configuration model
- Integration testing strategy

---

## Example Local Run

```shell
CI_PROJECT_ID=123
CI_MERGE_REQUEST_IID=456
CI_PIPELINE_SOURCE=merge_request_event
CI_MERGE_REQUEST_SOURCE_BRANCH_NAME=feat/example
CI_MERGE_REQUEST_TARGET_BRANCH_NAME=develop
CI_MERGE_REQUEST_LABELS="backend,needs-review"
cargo run -- observe
```


Mr. Milchick will begin observation.

---

## Long-Term Vision

Future capabilities may include:

- CODEOWNERS-aware reviewer routing
- risk-based merge request scoring
- policy severity levels
- workflow analytics
- praise reinforcement system
- configurable tone intensity
- team-aware behavioral tuning

The objective is not merely automation.

The objective is **workflow civilization**.

---

## Contributing

Contributions should maintain:

- architectural clarity
- deterministic behavior
- calm narrative tone
- strict separation of concerns

Unstructured enthusiasm will be refined.

---

## Disclaimer

Mr. Milchick is fictional.  
The policies he enforces are not.

Proceed responsibly.