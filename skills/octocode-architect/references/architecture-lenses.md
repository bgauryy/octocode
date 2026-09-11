# Architecture Lenses

Load when modeling a consequential design, boundary, interface, flow, or unfamiliar path. Why: one view hides dependencies and ownership that another exposes.

## Start with wiring

Trace the behavior as:

`source → parse/validate → transform/decide → boundary → sink/effect → observation`

At each hop, name the data shape, owner, invariant, failure mode, and interface. Static types document and constrain checked code; runtime schemas and validation are the executable boundary contracts. Make invalid states difficult or impossible to represent, then verify the runtime path. Keep core decisions separate from UI, storage, and transport edges.

Record `module/layer → responsibility → owned data/invariants → public interface → allowed dependencies`. Mark rules `declared`, `observed`, or `inferred`; only a declared rule can directly prove a violation.

## Cross-check four views

| View | Question |
|---|---|
| Static | Which exact symbols and files depend on each other? |
| Control | Who initiates, branches, retries, cancels, handles errors, and owns effects? |
| Data | Where is data created, validated, authorized, transformed, persisted, and exposed? |
| Ownership | Who owns each invariant, lifecycle, contract, and exception? |
| Runtime | What configuration, registry, process, service, generated artifact, or dynamic dispatch completes the wiring? |

For 1–3 representative scenarios, rejoin the lanes as `trigger → decision → transformation → side effect → result/error`. Record unresolved reflection, queues, framework registration, external systems, and trust or transaction boundaries.

Judge design by a named quality attribute: correctness, security, changeability, performance, testability, operability, or delivery speed. Similar syntax is not shared policy; a small API is not automatically safe; folder count is not modularity; high degree is not slowness.

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
