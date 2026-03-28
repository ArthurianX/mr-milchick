

# Security Policy

## Scope

Mr Milchick is a CI-native governance tool that can:

- inspect merge request metadata
- evaluate repository structure and policy
- Assigned reviewers
- post comments
- influence pipeline outcomes

Because it operates inside CI pipelines and may use privileged project tokens, security issues should be treated seriously.

---

## Supported Versions

At this stage, security support applies to the latest maintained version on the default branch.

As the release process matures, this policy may expand to include specific tagged versions.

---

## Reporting a Vulnerability

Please do **not** report security vulnerabilities through public issues, public merge requests, or public discussion threads.

Instead, report them privately to the maintainers with:

- a clear description of the issue
- affected component or behavior
- reproduction steps, if available
- potential impact
- any suggested mitigation

If the issue involves token exposure, permission misuse, or unsafe GitLab mutation behavior, include that context explicitly.

---

## What to Report

Examples of security-relevant issues include:

- unsafe handling of GitLab tokens or credentials
- privilege escalation through CI execution paths
- unauthorized reviewer assignment or MR mutation behavior
- command or configuration injection risks
- insecure parsing of repository-controlled inputs
- leakage of sensitive metadata into logs or comments
- behavior that could be abused to block, manipulate, or misroute workflow execution

If you are unsure whether something is a security issue, report it anyway.

---

## Response Expectations

Maintainers will aim to:

- acknowledge receipt promptly
- assess severity and impact
- determine remediation priority
- coordinate a fix before public disclosure when appropriate

Response times may vary, but reports will be handled seriously and discreetly.

---

## Disclosure Policy

Please allow maintainers reasonable time to investigate and remediate reported vulnerabilities before any public disclosure.

Coordinated disclosure is strongly preferred.

---

## Operational Guidance

Users of Mr Milchick should follow these baseline practices:

- use the minimum GitLab token permissions required
- avoid exposing tokens in job logs
- restrict mutation-capable execution to trusted pipelines
- review configuration and routing changes carefully
- test new versions in dry-run mode before enabling mutation behavior broadly

---

## Security Philosophy

Mr Milchick should be deterministic not only in policy, but also in trust boundaries.

Security features and fixes should preserve:

- explicit permissions
- predictable behavior
- minimal privilege
- auditable outcomes