![milchick.jpg](milchick.jpg)

# Mr. Milchick

A pleasantly unsettling steward for GitLab merge requests.

Mr. Milchick observes.
Mr. Milchick refines.
Mr. Milchick ensures structural harmony.

This project is a Rust-based CLI tool designed to run inside GitLab CI pipelines and enforce merge request governance through calm, structured, policy‑driven automation.

It is not a bot.  
It is not a service.  
It is not a platform.  

It is a binary that cares.

---

## Purpose

Mr. Milchick exists to:

- enforce merge request workflow policies
- assign reviewers deterministically
- integrate CODEOWNERS intelligence into routing
- validate labels, branches and MR topology
- reduce coordination overhead in engineering teams
- provide explainable governance decisions

This tool is intended for environments where:

- GitLab Apps or external bots are restricted
- pipeline‑native automation is preferred
- workflow enforcement must be auditable
- governance logic must live in version control

---

## Philosophy

Mr. Milchick operates under these principles:

### Determinism over improvisation
The same merge request must produce the same outcome.

### Policy as code
Workflow governance evolves through normal code review.

### Calm enforcement
Strict automation does not need to be hostile.  
Politeness increases compliance.

### Structured tone
Human acceptance of automation is emotional.  
Tone is part of system design.

### Minimal infrastructure
No servers.  
No daemons.  
No persistent runtime.  

Only CI.  
Only execution.

---

## Command Model

Mr. Milchick supports three operational modes:

```
mr-milchick observe
mr-milchick refine
mr-milchick explain
```

I mean ... four if we're being honest: 
```
mr-milchick version

OUTPUT > mr-milchick 0.3.1 (2076d86 2026-03-18)
```


### observe

Policy evaluation dry‑run.

- reads CI context
- builds MR snapshot
- evaluates rule engine
- produces action plan
- does NOT mutate GitLab

### refine

Executes the planned governance actions.

May:

- assign reviewers
- post summary comments
- enforce blocking policies
- fail pipeline when required

### explain

Produces deep reasoning output.

Used for:

- debugging policy behavior
- understanding reviewer routing
- validating ownership logic
- inspecting rule outcomes

### version

Prints the binary version, git SHA and build date.

```
mr-milchick version
→ mr-milchick 0.3.1 (3f2c8ab 2026-03-18)
```

Useful for confirming which build is active in a pipeline without triggering any evaluation logic.

---

## Testing

Run the full test suite with:

```bash
cargo test
```

Run only the CLI integration harness with:

```bash
cargo test --test cli_integration
```

The integration test binary in `tests/cli_integration.rs` launches the compiled `mr-milchick` executable and talks to a stateful mock GitLab HTTP server, so it exercises:

- mode-specific CLI output
- GitLab snapshot fetches and mutations
- idempotency across repeated `refine` runs

Because that harness binds a local TCP port for the mock server, it should be run in an environment that allows local socket listeners.

---

## Execution Flags

Mr. Milchick behavior is controlled through runtime flags and environment context.

### Dry‑Run Mode

```
MR_MILCHICK_DRY_RUN=true
```

Forces `refine` into non‑mutating mode.

Used for:

- safe rollout
- CI experimentation
- policy validation

### CODEOWNERS Integration

```
MR_MILCHICK_CODEOWNERS_ENABLED=true
MR_MILCHICK_CODEOWNERS_PATH=.gitlab/CODEOWNERS
```

Enables:

- ownership‑aware reviewer routing
- per‑file ownership aggregation
- hybrid routing (ownership + reviewer capability env)

If not provided:

- CODEOWNERS defaults to enabled and Mr. Milchick looks for `CODEOWNERS`, `.github/CODEOWNERS`, `.gitlab/CODEOWNERS`, then `.CODEOWNERS`

### Reviewer Routing Configuration

```
MR_MILCHICK_REVIEWERS='[
  {"username":"milchick-duty","fallback":true},
  {"username":"principal-reviewer","mandatory":true},
  {"username":"alice","areas":["frontend","packages"]},
  {"username":"carol","areas":["backend"]},
  {"username":"grace","areas":["devops"]}
]'
MR_MILCHICK_MAX_REVIEWERS=2
```

Reviewers are supplied by the pipeline as JSON, not by a bundled repo config file.
Mr. Milchick's runtime configuration is CI env-driven: reviewer routing, CODEOWNERS toggles, and related execution settings are all loaded from environment variables at invocation time.

Each reviewer object can declare:

- `username`: GitLab username
- `areas`: list of review capabilities such as `frontend`, `backend`, `packages`, `devops`, `documentation`, `tests`
- `fallback`: optional boolean that makes the reviewer eligible when no area match can be selected
- `mandatory`: optional boolean that always includes the reviewer when they are eligible, even when area routing or CODEOWNERS would otherwise choose someone else

Mandatory reviewers are additive:

- they are selected before area-based or fallback routing
- they are also prepended to CODEOWNERS-driven assignment plans
- they do not consume the `MR_MILCHICK_MAX_REVIEWERS` cap used for area routing

---

## Tone System

Mr. Milchick communicates using structured tonal categories:

- Observation
- Refinement Opportunity
- Blocking Experience
- Pleasant Resolution
- Praise (future)

Tone is:

- deterministic per merge request
- architecture‑level, not cosmetic
- designed for institutional acceptance

Tone is not humor.  
Tone is operational ergonomics.

---

## Architecture Overview

```
CI Context
↓
GitLab Snapshot Intelligence
↓
Rule Engine
↓
Ownership Intelligence
↓
Reviewer Routing
↓
Action Planner
↓
Execution Strategy
↓
Structured Output
```

Key architectural domains:

### context/
CI parsing, normalization, execution mode inference.

### gitlab/
Snapshot client, DTO mapping, mutation API layer.

### rules/
Pure governance logic. No side effects.

### codeowners/
Ownership parsing and matching engine.

### routing/
Reviewer selection logic (topology + ownership).

### actions/
Action planning and execution abstraction.

### tone/
Deterministic narrative rendering.

### output/
Human‑readable CI reporting and MR comments.

---

## Why Rust

Mr. Milchick is written in Rust because:

- static binaries simplify CI distribution
- strong typing reduces governance risk
- async model suits API‑bound execution
- ownership model enforces architectural clarity
- long‑term maintainability is required

This is not a scripting utility.  
This is governance infrastructure.

---

## Current Capabilities

As of current development phase:

- strongly typed CI context model
- GitLab MR snapshot ingestion
- rule engine with severity classification
- deterministic reviewer routing
- hybrid CODEOWNERS + env routing
- action planning layer
- dry‑run execution strategy
- structured summary comment rendering
- explain mode parity with refine logic

---

## Example Local Execution

```
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

## Long‑Term Direction

Planned system evolution includes:

- merge request risk scoring engine
- reviewer load balancing
- policy DSL for organizational governance
- workflow analytics layer
- adaptive tone intensity
- merge readiness intelligence
- team topology awareness

The objective is not automation.

The objective is engineering civilization.

---

## Contributing

Contributions must preserve:

- deterministic system behavior
- clear architectural boundaries
- policy clarity over cleverness
- calm operational tone

Unstructured enthusiasm will be gently redirected.

---

## Disclaimer

Mr. Milchick is fictional.  
The governance he enforces is not.

Proceed deliberately.
