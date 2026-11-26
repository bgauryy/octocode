Load when deciding how to decompose work or what to do after new evidence arrives.

# Choose and run a loop

Choose by dependencies and the next acceptance gap. A role is a duty, not a requirement for another agent.

| Situation | Execution shape | Deciding condition |
|---|---|---|
| One bounded question or tightly coupled change | Direct execution with tools | Additional coordination has no distinct evidence or capability to contribute. |
| Known dependent stages | Sequential workflow ([dependency graph](dependency-graph.md)) | The preceding artifact satisfies the next stage's input contract. |
| Independent evidence lanes or disjoint changes | Parallel assignments, then integration | Each lane has a useful output and clear ownership; verify their combined result. |
| Research discovers new questions | [Adaptive research](adaptive-research.md) | Assign the next material uncertainty after integrating current evidence. |
| A concrete acceptance check fails | [Implement → evaluate → repair](evaluate-repair.md) | Feedback identifies an actionable defect against stable acceptance. |
| Execution was interrupted | Reconcile → [resume](recovery.md#resume-from-current-state) | Current artifacts and operation state establish what can safely continue. |

## Execution graph

The feedback loop operates on observations, not a fixed number of rounds. Recheck cancellation and actual host/user limits at each decision boundary, including before dispatch or a wait.

```mermaid
flowchart TD
  O["Observe current state or resume"] --> G{"Cancellation or actual limit?"}
  G -- Yes --> H["Apply interruption rule; preserve known state and report"]
  G -- No --> V{"Integrated acceptance verified?"}
  V -- Yes --> Q["Close or hand over owned work"]
  Q --> S{"Accepted result still stable?"}
  S -- Yes --> D["Report verified completion"]
  S -- No --> O
  V -- No --> U{"Uncertain effect or conflicting writer?"}
  U -- Yes --> R["Reconcile operation, artifact and ownership"]
  R --> N{"New evidence enables a safe action?"}
  N -- Yes --> O
  N -- No --> I{"Independent useful work available?"}
  I -- Yes --> C["Choose the next acceptance gap"]
  I -- No --> W{"Relevant work is still active?"}
  U -- No --> A{"Useful authorized action available?"}
  A -- Yes --> C
  A -- No --> W
  C --> X["Execute or assign ready work"]
  X --> E["Collect and verify evidence; invalidate affected checks"]
  E --> O
  W -- Yes --> T["Wait for a relevant event or supported status result"]
  T --> O
  W -- No --> B["Checkpoint; report blocker or partial result"]
```

Cancellation stops owned work and reconciles effects using permitted capabilities. An actual limit preserves known state and unresolved work; neither path claims completion. If the host permits no further calls, provide the handoff in the response without claiming a persisted checkpoint or a stopped worker. A new observation can resume a blocked path, but elapsed time alone supplies no missing authority or evidence. A failed acceptance check returns a concrete defect to its owner; it does not release dependent work.
