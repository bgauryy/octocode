# Diagrams in RFCs

Load when an RFC or plan section explains a flow, structure, comparison, proportion, lifecycle, or schedule. Mermaid makes the shape explicit: humans see it, and agents read nodes and edges as facts. Prose carries the why and the evidence.

## Rules
- **One message per diagram.** If you cannot state the message, drop the diagram.
- **True data, drawn to scale.** Every number traces to a cited source. Mark computed or estimated figures.
- **Small:** ≤15 nodes or ≤8 points. Split big pictures into an overview plus details. Label every non-obvious edge.
- **Same names** as the code, schema, and tables (tool names, `S1`, `Q1`).
- **Beta types may not render.** When the argument rests on a `*-beta` chart, also give its numbers in a table.
- **No decoration:** no diagram that restates one sentence, no meaningless colors.

## Type per purpose
| Type (keyword) | Use for | Section |
|---|---|---|
| `flowchart` (`subgraph`) | decision trees, data/control flow, plan-step DAG, boundaries | Rationale, Reference, Plan |
| `sequenceDiagram` (`zenuml` if the team uses it) | protocols, retries, handoffs, continuation walks | Reference |
| `stateDiagram-v2` | lifecycles, legal vs illegal transitions | Reference |
| `classDiagram` / `erDiagram` | types and ownership / storage entities, keys, migrations | Reference, Migration |
| `C4Context` / `C4Container` / `C4Component` / `architecture-beta` | system, container, and service topology | Motivation, Reference |
| `block-beta` / `packet-beta` | layered layouts / wire and bit formats | Reference |
| `requirementDiagram` | requirement → design → test traceability | KPI |
| `xychart-beta` | before/after metrics, trends, a target line | Motivation, KPI, Results |
| `pie` / `sankey-beta` / `treemap-beta` | one total's composition / split-and-merge flows / hierarchical size | Motivation |
| `quadrantChart` / `radar-beta` | options on 2 criteria / on 4–8 criteria | Alternatives |
| `gantt` | only with committed dates or durations; otherwise a flowchart DAG | Plan |
| `timeline` / `gitGraph` | incident or decision history / branch, release, rollback strategy | Prior Art, Rollout |
| `journey` / `mindmap` | user or agent steps with friction scores / scope and non-goals | Motivation, Goals |
| `kanban` | status snapshot (rare; prefer the plan table) | Plan |

## Starter shapes
Replace every label and number with the RFC's own names and data. Plan steps as a DAG that mirrors each step's `Depends on:` field:

```mermaid
flowchart LR
    S1[S1 contract] --> S2[S2 consumer]
    S1 --> S3[S3 migration]
    S2 --> S4{S4 KPI met?}
    S3 --> S4
    S4 -- yes --> S5[S5 accept]
    S4 -- no --> S6[S6 roll back]
```

A measured series against a target: `xychart-beta` with `title "p95 latency (ms), lower is better"`, `x-axis [baseline, S2, S3, final]`, `y-axis "ms" 0 --> 400`, `bar [380, 260, 210, 190]`, `line [200, 200, 200, 200]`.

Validator checks never prove a diagram is true; review data and scales by hand.

Next: return to the section being drafted in `references/rfc-template.md` or `references/rfc-implementation.md`.
