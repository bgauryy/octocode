Load when a less common style is named: cells, deployment stamps, control plane and data plane, vertical slices, functional core, data mesh, lakehouse, lambda or kappa, or multi-agent.

# Concepts: more architecture styles

## More architecture styles

More styles, with the cost each adds:

| Style | Purpose | Trade-off |
|---|---|---|
| Cell-based architecture | Partition workloads into independent failure domains | Operational complexity |
| Deployment stamps | Replicate independent application stacks | Provisioning and routing complexity |
| Control plane / data plane | Separate management from request processing | Coordination and consistency |
| Self-contained systems | Independently deliver business capabilities | Duplication and integration |
| Vertical slice architecture | Organize software by end-to-end behavior | Repeated cross-cutting infrastructure |
| Functional core, imperative shell | Isolate pure business logic from effects | Boundary design effort |
| Workflow-oriented architecture | Coordinate durable multistep processes | Workflow state management |
| Data mesh | Decentralize analytical data ownership | Governance and interoperability |
| Data fabric | Integrate data through shared metadata and automation | Tooling complexity |
| Lakehouse | Combine data-lake storage with warehouse capabilities | Table maintenance and governance |
| Lambda architecture | Combine batch and streaming processing | Duplicate pipelines |
| Kappa architecture | Use a streaming-centric processing model | Replay and retention requirements |
| Digital twin | Maintain representations of real-world entities | Synchronization complexity |
| Multi-agent architecture | Coordinate specialized autonomous components | Nondeterminism and control overhead |

Cells and deployment stamps isolate tenant workloads and limit an incident to one unit; prove the claim by finding what the units still share (database, queue, control plane, DNS). Sidecar, ambassador, service mesh and BFF are in [messaging patterns](concepts-messaging.md#messaging-and-distributed-system-patterns) and [components](concepts-roles.md#application-components-and-roles).
