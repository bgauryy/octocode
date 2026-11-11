Load when agents must exchange tasks, results or events: choosing a protocol or pattern, defining typed messages and envelopes, cross-app adapters, delivery guarantees, streaming, schema versions, capability routing or contract tests.

# Agent communication

Delegation contracts and task ownership are in [models and delegation](models-and-delegation.md).

Principle: agents communicate through explicit, validated contracts, not unstructured conversation or shared reasoning transcripts. The layer lets agents discover capabilities, delegate work, exchange concise evidence, transfer artifacts, recover from errors and verify completion independently.

## Contents

- [Protocols](#protocols)
- [Patterns](#patterns)
- [A2A building blocks](#a2a-building-blocks)
- [Typed messages](#typed-messages)
- [One schema, many boundaries](#one-schema-many-boundaries)
- [Schema rules](#schema-rules)
- [Cross-app alignment](#cross-app-alignment)
- [Message envelope](#message-envelope)
- [Reliability and delivery](#reliability-and-delivery)
- [Streaming and artifacts](#streaming-and-artifacts)
- [Schema versioning](#schema-versioning)
- [Capability routing](#capability-routing)
- [Testing](#testing)
- [Choose a mechanism](#choose-a-mechanism)
- [Rules](#rules)
- [Sources](#sources)

## Protocols

| Protocol / technology | Responsibility | Best use |
|---|---|---|
| A2A (Agent2Agent) | Agent discovery, delegation, task lifecycle | Independent cooperating agents |
| MCP | Expose tools, resources and prompts | Connect agents to capabilities |
| JSON Schema | Language-independent data contract | Typed message boundaries |
| Zod | TypeScript runtime validation and schemas | Validating agent inputs and outputs |
| JSON-RPC | Structured remote procedure calls | Method-based communication |
| gRPC / Protobuf | Typed high-performance RPC | Internal services |
| HTTP REST | Resource-oriented communication | APIs and task services |
| SSE | Server-to-client event stream | Progress updates |
| WebSocket | Bidirectional persistent messaging | Interactive agents |
| Message broker | Asynchronous reliable delivery | Durable agent workflows |
| CloudEvents | Standard event envelope | Event-driven integration |
| OpenTelemetry | Distributed traces and correlation | Execution observability |

```text
Supervisor agent: routes work, validates results, owns overall completion
  ──A2A──→ specialist agent: owns its task lifecycle, returns artifacts
              ──MCP──→ tool server: search, read, execute, retrieve resources
Shared layer under all three: schema validation · authorization · resource references · tracing
```

A2A manages independent agent tasks, messages, artifacts and capability discovery; MCP connects an agent application to tools and resources. They complement each other; neither replaces the other. Check the current A2A specification for its version and states.

## Patterns

| Pattern | How it works | Use |
|---|---|---|
| Request–response | One request, one result | Small bounded tasks |
| Delegation–return | Parent delegates, worker returns | Specialist agents |
| Supervisor–worker | Supervisor assigns and aggregates | Coordinated workflows |
| Publish–subscribe | Agents receive subscribed events | Loose coupling |
| Request–reply messaging | Correlated asynchronous messages | Broker-based services |
| Fan-out / fan-in | Parallel workers, one aggregator | Independent analysis |
| Pipeline | Each agent handles a stage | Sequential transformations |
| Blackboard | Agents contribute to shared state | Collaborative reasoning |
| Actor model | Isolated agents exchange messages | Stateful concurrent systems |
| Negotiation | Agents exchange proposals and constraints | Resource or task allocation |
| Handoff | One agent transfers ownership | Specialized workflows |
| Orchestration | Central coordinator controls sequence | Reliable business processes |
| Choreography | Agents react to events | Decentralized workflows |
| Consensus / voting | Candidates combined or compared | Selected high-uncertainty decisions |

Start with one supervisor and bounded specialist workers.

## A2A building blocks

| Concept | Function |
|---|---|
| Agent Card | Advertises identity, skills, interfaces and security requirements |
| Agent Skill | Advertised capability of an A2A agent |
| Message | Instructions, clarification and status |
| Task | Trackable stateful work |
| Task state | Progress, interruption or completion |
| Part | Text, files or structured data |
| Artifact | Task outputs |
| Context ID | Groups related interactions |
| Task ID | Identifies one unit of work |
| Streaming | Incremental task updates |
| Push notification | Asynchronous progress reports |
| Extension | Negotiated extra functionality |

An A2A Agent Skill is advertised metadata, not an executable `SKILL.md` package. Messages communicate; artifacts deliver results.

## Typed messages

TypeScript types vanish at runtime, and a remote agent, tool, model, file or MCP server can return invalid data. Validate actual values at every boundary with Zod, and export JSON Schema when other languages need the contract. Examples use Zod 4 APIs.

```ts
import { z } from "zod";

// An application task request, not the A2A wire format.
const AgentRequest = z.strictObject({
  version: z.literal("1.0"),
  taskId: z.string().min(1),
  correlationId: z.string().min(1),
  from: z.string().min(1),
  to: z.string().min(1),
  intent: z.strictObject({
    goal: z.string().min(1),
    reason: z.string().min(1),
    expectedOutput: z.string().min(1),
  }),
  limits: z.strictObject({
    maxSteps: z.number().int().positive(),
    maxOutputTokens: z.number().int().positive(),
  }),
  contextRefs: z.array(z.string()),
  input: z.record(z.string(), z.unknown()),
});
type AgentRequest = z.infer<typeof AgentRequest>;
```

Use a discriminated union instead of one loose object with many optional fields:

```ts
const EvidenceRef = z.strictObject({ uri: z.string().min(1), revision: z.string().optional() });

const Result = z.discriminatedUnion("status", [
  z.strictObject({ status: z.literal("completed"), summary: z.string(), evidence: z.array(EvidenceRef) }),
  z.strictObject({
    status: z.literal("blocked"),
    reason: z.string(),
    retryable: z.boolean(),
    suggestedAction: z.string().optional(),
  }),
  z.strictObject({ status: z.literal("failed"), code: z.string(), message: z.string() }),
]);
type AgentResult = z.infer<typeof Result>;

function receiveAgentResult(raw: unknown): AgentResult {
  const parsed = Result.safeParse(raw);
  if (!parsed.success) throw new Error(`Invalid agent result: ${z.prettifyError(parsed.error)}`);
  return parsed.data;
}
```

Completed, blocked and failed work each get an explicit shape; TypeScript narrows on `status` and runtime validation rejects malformed results. `blocked` is an interruption requiring input or recovery, not a terminal outcome; preserve the callee's actual terminal state separately. A rejected request also needs a stable code and one retry, approval or terminal action.

Validate sends with `.parse()` and receives with `.safeParse()` before routing, storage or tool use; reject malformed packets with a small structured error. Use the request, result and envelope examples here as one application-owned set, not a wire standard. Only export with `z.toJSONSchema()` when a protocol needs JSON Schema.

| Technique | Purpose |
|---|---|
| `z.strictObject()` | Reject unexpected fields |
| `z.discriminatedUnion()` | Typed message variants |
| `.safeParse()` | Validate without throwing |
| `.refine()` / `.superRefine()` | Custom invariants / richer cross-field issues |
| `z.infer` | Derive TypeScript types |
| `z.toJSONSchema()` | Export interoperable contracts |
| `.parseAsync()` | Asynchronous refinements |
| `.optional()` / `.default()` | Optional data / controlled defaults |
| `.transform()` | Normalize after validation |
| `.brand()` | Distinguish identical-looking domain types |

Some Zod types and transforms have no faithful JSON Schema form. Zod-specific migration and API detail: the `zod` skill, if installed, or the Zod documentation.

## One schema, many boundaries

```text
Canonical contract (Zod / JSON Schema / Protobuf)
  → LLM outputs (constrain structure) · tool inputs (validate parameters) · tool outputs (validate observed data)
  → agent handoffs (validate delegated results) · MCP schemas · API documentation
```

One canonical contract reduces drift, but targets support different dialects: strict structured outputs accept only a JSON Schema subset. Export a provider-compatible schema at the model boundary, then validate the returned value and business invariants in application code.

## Schema rules

- Field and branch rules belong to [tools](tools-and-mcp.md#schema-and-result). Declare the dialect where the wire allows; project into each target's subset, no provider-only keywords.
- Validate ingress and egress; keep model-generated routing metadata apart from trusted state and credentials.
- Bound strings and arrays according to the owning workload. Large payloads use server-minted opaque handles with retention, expiry, dereference authorization and recovery; reauthorize every use.
- Schemas check shape, not authority or meaning. Reject unknown fields between agents; extend only via a versioned extension field.

## Cross-app alignment

- One app-owned semantic contract, one adapter per protocol; never rename native fields to match or build a universal envelope.
- Same shape but different meaning, trust, lifecycle or compatibility → separate contracts.
- Audit a crossing along `owner → producer → adapter → trust boundary → validation/authorization → consumer → result/error`; material rows only; decide each concern once, enforce it in the adapter:
  - Identity, tenancy (actor, agent, receiver, tenant): map to native auth; never trust model-supplied identity.
  - Authority (operation, scope, approval owner): reauthorize at the effect; discovery is not authorization.
  - Correlation: native IDs opaque; one ID per message.
  - Payload (media type, bounds, expected result): target dialect; reject lossy conversion.
  - Lifecycle (accepted, active, input-needed, terminal, cancelled): map states; keep retry, timeout, cancel and late-result behavior.
  - Effects (side-effect class, idempotency key): deduplicate before replaying an effect.
  - Result, error (artifact vs status, completeness): typed results, recovery handles.
  - Version: negotiate where defined; no silent downgrade.

## Message envelope

Separate routing, tracing, lifecycle and compatibility from the domain payload:

```ts
const Envelope = z.strictObject({
  schemaVersion: z.literal("1.0"),
  messageId: z.uuid(),
  taskId: z.string(),
  correlationId: z.string(),
  causationId: z.string().optional(),
  sender: z.string(),
  recipient: z.string(),
  kind: z.enum(["command", "event", "query", "result", "error", "cancel"]),
  createdAt: z.iso.datetime(),
  deadlineAt: z.iso.datetime().optional(),
  idempotencyKey: z.string().optional(),
  payloadSchema: z.string(),
  payload: z.unknown(),
});
```

| Field | Purpose |
|---|---|
| `messageId` | Unique message identity |
| `taskId` | Logical task identity |
| `correlationId` / `causationId` | Related messages / the event that caused this one |
| `sender` / `recipient` | Routing and audit |
| `kind` | Command, event, query, result, error or cancel |
| `schemaVersion` | Envelope version |
| `payloadSchema` | Selects the versioned payload validator, applied after the envelope |
| `deadlineAt` | Bounds useful execution time |
| `idempotencyKey` | Deduplicates logical operations |

Use envelope fields only when they change a decision; production envelopes may add trace context, tenant scope, content type, priority and artifact references. Replies mint a new `messageId` and use `causationId` for the replied-to message (the `inReplyTo` relationship), preserving task correlation. Identity and authorization come from authenticated transport or verified credentials, never from the `sender` field.

## Reliability and delivery

| Concern | Approach |
|---|---|
| Duplicated messages | Stable idempotency keys |
| Lost messages | Durable broker or outbox where required |
| Out-of-order delivery | Sequence numbers and version checks |
| Repeated commands | Idempotent handlers |
| Agent crash | Durable checkpoints, resumable tasks |
| Agent timeout | Query task state before repeating side effects |
| Partial completion | Persist completed steps and remaining work |
| Concurrent modification | Optimistic locking or transactional serialization |
| Long-running work | Task IDs with polling, streaming or callbacks |
| Cancellation | Explicit, acknowledged cancellation |
| Backpressure | Bounded queues and admission control |
| Dead letters | Quarantine failed messages with diagnostics |
| Schema mismatch | Reject, negotiate or migrate versions |
| Duplicate results | Deduplicate by task and artifact identity |

Exactly-once messaging is not exactly-once business effects. Prefer at-least-once delivery with idempotent processing and durable deduplication.

## Streaming and artifacts

| Event | Content |
|---|---|
| `task.started` | Accepted, ID assigned |
| `task.progress` | Meaningful stage transition |
| `task.blocked` | Missing input or permission |
| `artifact.available` | Reference to a produced artifact |
| `task.completed` | Final result and verification status |
| `task.failed` | Error, retryability, recovery options |
| `task.canceled` | Confirmed cancellation |

Stream semantic progress, not internal reasoning text. Where updates can be lost, keep authoritative task state queryable. In A2A, artifacts can stream incrementally while task status stays queryable.

## Schema versioning

| Change | Compatibility |
|---|---|
| Add optional field | Usually compatible for tolerant consumers |
| Add required field | May break existing producers |
| Rename or remove field | Breaking for consumers relying on it |
| Change field type | Usually breaking |
| Add enum value | Can break exhaustive consumers |
| Change meaning or units | Semantic break unless versioned |
| New message type | Needs capability negotiation or tolerant handling |

```text
contracts/
  agent-envelope/v1.schema.json
  architecture-findings/v1.schema.json
  architecture-findings/v2.schema.json
  task-result/v1.schema.json
```

Run compatibility checks in CI, keep fixtures for older producers and consumers, and use explicit upcasters when reading historic persisted events. A breaking shape bumps the version; accept old versions only in an explicit migration window.

### Change and removal

- Inventory producers, consumers, stored payloads, generated clients and deployed versions → classify per consumer as additive, behavioral or breaking (a default change can break without a shape change) → bridge with an adapter or version branch, never a second canonical schema or permanent alias → test valid, invalid, denied, partial, replay, timeout, cancel and version-skew cases through real seams.
- Remove only when nothing emits or needs the contract (stored payloads included), the migration window closed and rollback evidence exists; an empty repo search does not prove external absence.
- Report per concern: owner, mapping, validation boundary, compatibility and verdict (keep, align, adapt, remove).

## Capability routing

Select agents by declared capabilities, not names: stable ID, supported skills, input and output modalities, contract versions, execution mode (sync, async, streaming), payload and concurrency limits, authorization requirements, data classification, cost and latency profile, health and deployment version.

Route by capability fit → permission → contract compatibility → health → cost and latency. Never pick an untrusted external agent only because its description claims the capability.

## Testing

| Test | Validates |
|---|---|
| Contract | Message conforms to its schema |
| Consumer-driven contract | Producer meets consumer assumptions |
| Compatibility | Old and new versions interoperate |
| Property-based | Invariants across generated inputs |
| Fuzz | Malformed payload handling |
| Duplicate delivery | Idempotency |
| Reordering | Out-of-order behavior |
| Crash / restart | Durable recovery |
| Timeout | Deadline behavior |
| Cancellation | Cancellation and side effects |
| Authorization | Cross-agent permission enforcement |
| Streaming | Update order and artifact assembly |
| Load | Throughput, queueing, saturation |
| Chaos | Partial failures and partitions |

Validate on ingress and egress, and test real contract fixtures; TypeScript compilation alone proves nothing at runtime.

## Choose a mechanism

| Need | Use | Why |
|---|---|---|
| One agent runs a capability | MCP tool | Structured inputs and outputs |
| Two independent agents collaborate | A2A | Discovery, tasks, messages, artifacts |
| Agents share a process | Typed function calls or an internal queue | No network protocol overhead |
| Persistent asynchronous commands | Broker + durable task state | Decoupled, recoverable execution |
| TypeScript contract validation | Zod | Runtime parsing plus inferred types |
| Cross-language messages | JSON Schema or Protobuf | Portable schemas, generated clients |
| Asynchronous status updates | SSE or A2A streaming | Progress without excess polling |
| Large results | Artifact reference | Small agent context |
| Reliable repeated operations | Idempotency + transactional records | No repeated side effects |
| Schema changes across versions | Versioned registry + compatibility tests | No integration breakage |
| Many available agents | Capability registry + router | Choice by contract and permission |

## Rules

1. Schema first: every message boundary has a defined, validated contract.
2. A2A for agents, MCP for capabilities.
3. Separate messages from artifacts: messages carry intent and status; deliverables are stored independently.
4. Validate twice: outgoing and incoming.
5. Reference large results instead of copying them into every context.
6. Use correlation and causation IDs so distributed activity is traceable.
7. Design for duplicates and failure: retries must not duplicate logical effects.
8. Version contracts deliberately and test compatibility before deploying.
9. Bound delegation: explicit ownership, budgets, deadlines and completion criteria.
10. Trust verification, not declarations: an agent reporting success does not establish success.

Protocol security is in [security](security.md#between-agents).

## Sources

- [A2A protocol specification](https://a2a-protocol.org/latest/specification/)
- [Model Context Protocol specification](https://modelcontextprotocol.io/specification)
- [Zod documentation](https://zod.dev/) and [JSON Schema conversion](https://zod.dev/json-schema)
- [OpenAI — Structured outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
