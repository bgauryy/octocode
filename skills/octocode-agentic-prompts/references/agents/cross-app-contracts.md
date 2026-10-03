# Cross-app contracts and Zod packets

Load when a capability, handoff, message, artifact, or schema crosses agent apps, hosts, vendors, transports, or protocol versions; or when a handoff, tool result, or MCP input needs a TypeScript/Zod contract.

One application-owned semantic contract, one adapter per external protocol. Never rename native fields to make protocols look alike or build a universal envelope. Interaction choice, authority, and lifecycle: `agent-communication.md`.

## Alignment map

Trace `semantic owner → producer → adapter → trust boundary → validation/authorization → consumer → result/error`; record only material rows.

| Concern | Canonical decision | Adapter obligation |
|---|---|---|
| Identity and tenancy | actor, agent, receiver, tenant | map to native auth fields; never trust model-supplied identity |
| Capability and authority | operation, scope, approval owner | discovery is not authorization; reauthorize at the effect boundary |
| Correlation | request, message, task IDs, parent | preserve opaque native IDs; never reuse one ID for two messages |
| Payload | input, media type, bounds, expected result | target's schema dialect; reject lossy conversion |
| Lifecycle | accepted, active, input-needed, terminal, cancelled | map states explicitly; keep retry, timeout, cancel, late-result behavior |
| Delivery and effects | side-effect class, idempotency key, replay policy | deduplicate before repeating an effect |
| Result and error | artifact vs. status; completeness; stable error meaning | typed results and recovery handles, not prose |
| Version and extension | semantic version, compatibility policy | negotiate where the protocol says; no silent downgrade |

## Schema rules

- Declare the schema dialect when the wire permits; project into each target's subset, never copying provider-only keywords.
- Discriminant (`kind`/`status`) for exclusive states; define absent, `null`, empty, default, and unknown-field behavior.
- Each field: one name, unit, encoding, cardinality, sensitivity, owner. Same shape is not same meaning.
- Validate ingress and egress at every trust boundary; keep model-generated routing metadata apart from trusted state and credentials.
- Bound strings, arrays, and inline content; large payloads use handles with retention, expiry, dereference authorization, and recovery.
- A schema validates shape only, never capability, authorization, or semantics.
- Reject unknown fields at inter-agent boundaries; extensibility only through a versioned extension field.

## Zod packet

A minimal local pattern to adapt, not a standard or a reason to replace an existing schema.

```ts
import { z } from "zod";

const Evidence = z.object({ label: z.string().min(1).max(120), ref: z.string().min(1).max(500) }).strict();
const Request = z.object({
  v: z.literal(1), kind: z.literal("request"), id: z.string().min(1),
  goal: z.string().min(1).max(800),
  scope: z.array(z.string().min(1).max(120)).max(12).default([]),
  expects: z.enum(["answer", "findings", "artifact_ref"]),
  evidence: z.array(Evidence).max(8).default([]),
}).strict();
const Reply = z.object({ v: z.literal(1), id: z.string().min(1), inReplyTo: z.string().min(1) });
const ErrorInfo = z.object({ code: z.string().min(1).max(80), retry: z.enum(["retry", "ask_user", "do_not_retry"]) }).strict();
const Result = Reply.extend({ kind: z.literal("result"), summary: z.string().min(1).max(800), artifactRef: z.string().min(1).max(500).optional(), next: z.string().max(240).optional() }).strict();
const Failure = Reply.extend({ kind: z.enum(["blocked", "rejected"]), summary: z.string().min(1).max(800), error: ErrorInfo, next: z.string().max(240).optional() }).strict();
export const AgentPacket = z.discriminatedUnion("kind", [Request, Result, Failure]);
```

- Producer: `AgentPacket.parse(packet)` right before sending. Consumer: `safeParse` before routing, storage, or tool use; return a small structured rejection.
- Every reply has a new `id` and correlates with `inReplyTo`. `blocked` and `rejected` carry a stable code plus one retry, approval, or terminal action.
- Bump `v` for breaking changes; accept old versions only in an explicit migration window. `z.toJSONSchema()` only when a protocol needs JSON Schema.

## Change, merge, and removal gates

1. Inventory producers, consumers, persisted payloads, generated clients, deployed versions, caches, and queues.
2. Classify per consumer: additive, behavioral, or breaking. A default change can break without a shape change.
3. Differing native contracts get an adapter or version branch, never a second canonical schema or permanent alias.
4. Test valid, invalid, denied, partial, replay, timeout, cancel, and version-skew cases through real producer and consumer seams.
5. Remove an alias, field, schema, adapter, or version only when no producer emits it, no consumer or stored payload needs it, the migration window closed, and rollback evidence exists. Empty repo references do not prove external absence.

Keep contracts separate when meaning, trust, lifecycle, or compatibility differ, even with matching shapes.

Review output: `| Semantic concern | Owner | App/protocol mapping | Validation/authorization boundary | Compatibility | Keep/align/adapt/remove |`.

Source: [Zod JSON Schema](https://zod.dev/json-schema) (`z.toJSONSchema()` stable, `z.fromJSONSchema()` experimental).

Next: tool fields and MCP details `../tools/tool-contracts.md`.
