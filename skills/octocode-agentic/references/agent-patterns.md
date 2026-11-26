Load when choosing how several agents are arranged.

# Agent communication patterns

## Patterns

| Pattern | How it works | Use when |
|---|---|---|
| Request–response | One request, one result | The task is small and bounded |
| Request–reply messaging | Correlated asynchronous messages | Agents talk through a broker |
| Publish–subscribe | Agents receive the events they subscribed to | Senders must not know or wait for receivers (loose coupling) |
| Pipeline | Each agent handles a stage | Each stage transforms the previous stage's output |
| Orchestration | A central coordinator controls the sequence | Steps must complete in order with one owner of completion |
| Choreography | Agents react to events with no coordinator | No single owner can sequence the work; completion is then harder to trace, so keep task state queryable ([delivery](message-delivery.md#streaming-and-artifacts)) |
| Blackboard | Agents contribute to shared state | Findings from several agents build one result; one agent writes the result ([delegation](delegation.md#delegate-only-separable-work)) |
| Actor model | Isolated agents exchange messages | Each agent holds its own state and the work is concurrent |
| Negotiation | Agents exchange proposals and constraints | A resource or task must be allocated between agents |
| Consensus / voting | Candidates combined or compared | Only a number or label that agreement can check ([reasoning](reasoning.md)) |

The problem-first chooser is [choose a pattern](architecture.md#choose-a-pattern). When a measured gap needs more than one agent, follow [delegate only separable work](delegation.md#delegate-only-separable-work), which owns supervisor–worker, delegation–return and fan-out; an agent that takes over ownership follows [handoffs](handoffs.md#handoff-protocols).

Which mechanism carries a message is in [agent protocols](agent-protocols.md#choose-a-mechanism). Protocol security is in [security](security-risks.md#between-agents).
