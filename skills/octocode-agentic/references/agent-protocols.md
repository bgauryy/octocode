Load when agents must exchange tasks, results or events and you are choosing the mechanism (MCP, A2A, ACP, a broker, a schema format) or routing by capability.

# Agent protocols and capability routing

Principle: agents communicate through explicit, validated contracts, not unstructured conversation or shared reasoning transcripts.

## Choose a mechanism

| Need | Use | Why |
|---|---|---|
| One agent runs a capability | MCP tool | Structured inputs and outputs |
| Two independent agents collaborate | A2A | Discovery, tasks, messages, artifacts |
| An editor or client drives a coding agent | Agent Client Protocol ([ACP](acp.md)) | One prompt, one reply |
| An agent front end shows the run to a user | AG-UI | Agent-to-user interaction events |
| Agents share a process | Typed function calls or an internal queue | No network protocol overhead |
| Persistent asynchronous commands | Broker + durable task state ([delivery](message-delivery.md#reliability-and-delivery)) | Decoupled, recoverable execution |
| TypeScript contract validation | Zod ([typed messages](typed-messages.md)) | Runtime parsing plus inferred types |
| Cross-language messages | JSON Schema or Protobuf ([schemas](message-schemas.md#one-schema-many-boundaries)) | Portable schemas, generated clients |
| Asynchronous status updates | SSE or A2A streaming ([streaming](message-delivery.md#streaming-and-artifacts)) | Progress without excess polling |
| Both sides send messages during one session | WebSocket; SSE only carries server-to-client events | Bidirectional persistent messaging for interactive agents |
| Typed, high-throughput calls between internal services | gRPC with Protobuf | Typed RPC without a public API surface |
| Events cross systems that already route events | CloudEvents envelope | A standard event envelope instead of a custom one |
| Follow one task across agents and tools | OpenTelemetry trace context | Distributed traces and correlation |
| Large results | Artifact reference ([reference-first](tool-results.md#reference-first-responses)) | Small agent context |
| Schema changes across versions | Versioned registry + compatibility tests ([versioning](schema-versioning.md)) | No integration breakage |
| Many available agents | Capability registry + router ([capability routing](#capability-routing)) | Choice by contract and permission |

```text
Supervisor agent: routes work, validates results, owns overall completion
  ──A2A──→ specialist agent: owns its task lifecycle, returns artifacts
              ──MCP──→ tool server: search, read, execute, retrieve resources
Shared layer under all three: schema validation · authorization · resource references · tracing
```

A2A manages independent agent tasks, messages, artifacts and capability discovery; MCP connects an agent application to tools and resources. They complement each other. A2A 1.0 has been stable since March 2026 and is hosted by the Agentic AI Foundation under the Linux Foundation; IBM's Agent Communication Protocol merged into it in 2025 and is archived. Check the [A2A protocol specification](https://a2a-protocol.org/latest/specification/) for versions and states. An A2A Agent Skill is advertised metadata, not an executable `SKILL.md` package. Messages communicate; artifacts deliver results.

A protocol carries the message; it does not make the message sufficient: in the [MAST traces](delegation.md#delegate-only-separable-work), coordination failures occurred even between agents sharing one format, because the sender did not model what the receiver needed. Fill the message from the [brief](guiding-agents.md#brief-a-subagent) and the [catalog](protocol-catalog.md#catalog) slots.

## Capability routing

Select agents by declared capabilities, not names: stable ID, supported skills, input and output modalities, contract versions, execution mode (sync, async, streaming), payload and concurrency limits, authorization requirements, data classification, cost and latency profile, health and deployment version.

Route by capability fit → permission → contract compatibility → health → cost and latency. A claimed capability is not trust ([between agents](security-risks.md#between-agents)).
