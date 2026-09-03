# Architecture Lenses

Load when Think reaches a consequential design or unfamiliar path. Why: one view hides dependencies that another exposes.

## Start with wiring

Trace the behavior as:

`source → parse/validate → transform/decide → boundary → sink/effect → observation`

At each hop, name the data shape, owner, invariant, failure mode, and interface. Types are executable contracts; make invalid states difficult or impossible to represent. Keep core decisions separate from UI, storage, and transport edges.

## Cross-check four views

| View | Question |
|---|---|
| Graph | Who depends on this, and what does it depend on? |
| Code | What does the exact implementation establish? |
| Stream | How do data and control move, branch, retry, or terminate? |
| Runtime | What configuration, process, package, service, or generated artifact wires it together? |

Search beyond the first match. Similar names can hide different contracts; verify before reuse. Prefer one source of truth, caller-shaped interfaces, high cohesion, and dependencies pointing toward stable policy.

## Decompose and compose

Break the task into the smallest problems that produce meaningful, verifiable outcomes. Map their dependency edges before ordering them. Run independent reads, checks, tools, or workers concurrently only when ownership is clear and parallelism reduces latency or provides independent evidence; dependent work stays sequential.

Give each part explicit inputs, outputs, and a verification point. Compose only after the parts pass, then test their shared interfaces and the end-to-end path. Never use more workers merely to imitate decomposition or hide unresolved failures.

## Affected scope and impact

Map both direct and second-order effects:

- public and internal interfaces, callers, consumers, and dependency edges;
- persisted or serialized data, schemas, defaults, migrations, and compatibility;
- runtime behavior, latency, memory, network hops, retries, concurrency, and failure containment;
- trust boundaries, sensitive data, authorization, and output from untrusted sources or models;
- tests, telemetry, rollout, rollback, generated outputs, and repository records.

Evaluate only material dimensions; mark a surprising omission N/A with a reason. After editing, inspect the diff, and retrace the affected wiring for stale copies, asymmetric branches, changed defaults, and widened impact.

Next: plan with `output-contracts.md` when the decision needs a written contract; otherwise return to the workflow in `SKILL.md`.
