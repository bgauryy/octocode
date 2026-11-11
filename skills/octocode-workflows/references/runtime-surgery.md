Load when a running feature must be improved with separate code/architecture and runtime actors.

# Runtime surgery

The orchestrator selects one feature at a time, owns authority and the ledger, and assigns the code/architecture actor and runtime actor. No actor recursively spawns. If the runtime role cannot access the decisive process/interface, report the execution limitation.

| Actor | Owns | Returns |
|---|---|---|
| Code/architecture actor | Source research, proven boundary, authorized edit, focused checks, build request | Changed paths, expected identities, acceptance replay, source evidence |
| Runtime actor | Process lifecycle, scenario state, real-interface calls, activation, loaded identity, replay observation | Preconditions, actual result/error, loaded identity, state/reset needs, raw evidence |

## Process lifetime and handoff

| Trigger | Required action | Why |
|---|---|---|
| Start a process | Establish host/session lifetime before external replay; keep it alive through replay or use an owned durable supervisor. | Completion of a tool call does not keep a process alive. |
| Receive a live handoff | Check endpoint and artifact identity. | Delivery alone proves no process lifetime. |
| Process missing | Reconcile endpoint, process, artifact and durable state before acting. | Never assume prior state survives or stop a remembered/stale PID. |

```mermaid
flowchart LR
  F[Orchestrator frames feature and row] --> R[Runtime actor establishes baseline]
  R --> Q{Baseline meets acceptance?}
  Q -- yes --> M
  Q -- no --> C[Code actor proves boundary]
  C --> B[Code actor changes and builds]
  B --> A[Runtime actor activates and asserts identity]
  A -- identity matches --> P[Runtime actor restores state and replays]
  P --> M[Orchestrator merges acceptance]
  A -- unknown or mismatch --> D[Orchestrator diagnoses evidence]
  M -- failure --> D
  D -- activation or identity cause --> A
  D -- behavioral cause --> C
  D -- missing prerequisite --> H[Report unverified row]
```

1. Agree a Task contract for the feature: inputs (checkout, baseline including uncommitted work, endpoint, configuration, scenario, build-to-process route) and Definition of Done (the acceptance condition). Use `octocode-agentic` coordination for separate identities, leases, durable requests, and completion.
2. Runtime actor captures exact inputs, durable state, expected/actual output, process/build identity, and error/exit evidence through the real interface. If the baseline already meets the acceptance condition, it returns that observation for orchestrator acceptance without an edit. It returns a ledger-row observation, not a verdict.
3. Code actor owns source research, proven boundary, authorized edit, and build request. It uses Research's exact reads/AST/LSP semantics and Architect's Frame → Model → Prove → Change → Verify, including strongest rival and distinguishing check. It sends only ready build identities and replay instructions.
4. Runtime actor uses the [identity guard](verification.md#automatically-check-artifact-identity), establishes the actual loaded build, restores required state, and repeats the same acceptance and material edge checks.
5. Orchestrator merges the result and diagnoses failure before assigning another action. An identity mismatch or unknown identity repairs activation and identity proof first, never triggers a behavioral patch by itself. A timeout, expired request, or unavailable partner is not acceptance; follow [recovery](verification.md#diagnose-and-replan-from-evidence) and inspect durable effects before any repeated mutation.

Finish one feature's baseline → proof → change → activate → replay loop before another. Release owned leases and clean up only processes started for this workflow unless the user asks otherwise.
