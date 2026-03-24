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

OUTPUT > mr-milchick 1.0.0 (2076d86 2026-03-23)
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
- send Slack review notifications when configured
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
→ mr-milchick 1.0.0 (3f2c8ab 2026-03-23)
```

Useful for confirming which build is active in a pipeline without triggering any evaluation logic.

---

## Testing

Run the full test suite with:

```bash
cargo test
```

Run the thin app-level CLI smoke harness with:

```bash
cargo test -p mr-milchick --test cli_integration
```

Run connector-owned integration tests with:

```bash
cargo test -p milchick-connectors
```

The CLI integration test binary in `apps/mr-milchick/tests/cli_integration.rs` launches the compiled `mr-milchick` executable and talks to a stateful mock GitLab HTTP server, so it exercises:

- mode-specific CLI output
- workspace/runtime wiring
- one end-to-end refine path through the compiled connectors

Connector-specific HTTP behavior, idempotency, and Slack sink payload assertions now live with the connector crate rather than the app crate.

Because that harness binds a local TCP port for the mock server, it should be run in an environment that allows local socket listeners.

Additional operational docs:

- [`docs/local-testing.md`](/Users/arthur.kovacs/Work/mr-milchick/docs/local-testing.md)
- [`docs/connector-compilation-guidelines.md`](/Users/arthur.kovacs/Work/mr-milchick/docs/connector-compilation-guidelines.md)
- [`docs/build-pipeline-examples.md`](/Users/arthur.kovacs/Work/mr-milchick/docs/build-pipeline-examples.md)

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

### Slack Review Notifications

```bash
MR_MILCHICK_SLACK_BOT_TOKEN=xoxb-...
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X
MR_MILCHICK_SLACK_ENABLED=true
```

Or, for the webhook sink:

```bash
MR_MILCHICK_SLACK_WEBHOOK_URL=https://hooks.slack.com/triggers/...
MR_MILCHICK_SLACK_CHANNEL=C0ALY38CW3X
MR_MILCHICK_SLACK_ENABLED=true
```

When Slack is configured, `refine` posts a review notification only when:

- execution is real, not dry-run
- the merge request is not being blocked or failed
- reviewers were actually assigned during that run

Mr. Milchick supports two Slack sink variants, both centered on a light parent message plus a fuller follow-up:

Current message shape:

- channel line: `:gitlab: Reviews Needed for <MR-link|MR #iid>, by author :pepe-review:`
- thread body: bold tone line, MR title link, and `_Assign reviewers_` followed by bold reviewer mentions

The Slack app sink uses Slack's Web API and keeps the second message threaded directly from Milchick.

The Slack workflow sink is intended for Slack Workflow input webhooks, not Slack apps or generic incoming webhooks. It sends one workflow trigger payload with three workflow variables:

- `mr_milchick_talks_to`
- `mr_milchick_says`
- `mr_milchick_says_thread`

That lets the Slack workflow itself post a light top-level message and a fuller thread reply inside the workspace, while Milchick only needs access to the workflow trigger URL. The workflow sink also downgrades the detailed message to simple plain text without Slack markdown formatting.

Reviewer names in the Slack thread are rendered as `@username` based on the GitLab reviewer usernames chosen during routing.

Slack app setup notes:

- the bot token must have `chat:write`
- the app must be a member of the target channel, or have `chat:write.public` for public channels

For local testing or CLI integration tests, `MR_MILCHICK_SLACK_BASE_URL` can override the default Slack API base URL (`https://slack.com/api`).

Slack workflow webhook notes:

- this variant is designed for lower-permission environments where creating a Slack app may require admin approval
- the webhook URL must be a Slack Workflow input webhook URL
- the workflow must accept the three `mr_milchick_*` variables above
- the workflow is responsible for posting the lightweight parent message and the fuller threaded follow-up

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

## Connector Architecture

```
Review Connector
  -> ReviewSnapshot
  -> Rule Engine
  -> Decision Model
  -> Action Plan
  -> Execute Review Actions via same Review Connector
  -> Fan out Notifications via Notification Sinks
```

Mr. Milchick binaries are built from:

- exactly 1 review connector
- zero or more notification sinks

That means:

- review reads and writes always go through the same connector
- sinks never influence core planning logic
- the planner emits neutral intents, not platform API payloads

Current first-party connectors:

- review connector: GitLab
- notification sinks:
  - Slack app
  - Slack webhook

Workspace layout:

```text
apps/
  mr-milchick/

crates/
  milchick-core/
  milchick-runtime/
  milchick-connectors/
```

Responsibilities:

### `apps/mr-milchick`

- CLI parsing
- flavor loading
- runtime bootstrap
- capability reporting

### `crates/milchick-core`

- platform-neutral domain types
- rules
- reviewer planning
- CODEOWNERS analysis
- rendered message model

### `crates/milchick-runtime`

- connector traits
- execution wiring
- capability model
- dry-run vs real execution behavior

### `crates/milchick-connectors`

- GitLab review connector
- Slack app sink
- Slack workflow sink
- connector integration tests

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
