# Zod Agent Packet Contracts

Load when an agent handoff, tool result, or MCP-facing input/output needs a TypeScript/Zod contract.

**Validate at every trust boundary; keep the packet small.** A schema proves shape, not that the request is authorized, relevant, or safe.

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
const Result = Reply.extend({ kind: z.literal("result"), summary: z.string().min(1).max(800), artifactRef: z.string().url().optional(), next: z.string().max(240).optional() }).strict();
const Failure = Reply.extend({ kind: z.enum(["blocked", "rejected"]), summary: z.string().min(1).max(800), error: ErrorInfo, next: z.string().max(240).optional() }).strict();
export const AgentPacket = z.discriminatedUnion("kind", [Request, Result, Failure]);
```

## Contract states

| State | Required correlation and recovery |
|---|---|
| `request` | message `id`, goal, scope, expected result |
| `result` | new message `id`, `inReplyTo`, summary, optional artifact |
| `blocked` / `rejected` | new message `id`, `inReplyTo`, stable error code, retry action |

## Apply it

- Producer: `AgentPacket.parse(packet)` immediately before sending. Consumer: `safeParse` before routing, storage, or tool use; return a small structured rejection on failure.
- Use a discriminant (`kind`/`status`) for mutually exclusive packet states. `id` is every packet's message ID; every reply has a new `id` and uses `inReplyTo` for correlation.
- `result` carries a successful summary; `blocked` and `rejected` carry a stable error code plus one retry, approval, or terminal action.
- Bound strings and arrays to protect context. Use evidence references and artifact handles instead of inline corpora.
- Reject unknown fields at inter-agent boundaries so contract drift is visible; permit extensibility only through a versioned, documented extension field.
- Bump `v` for breaking changes and accept old versions only during an explicit migration window. Convert Zod with `z.toJSONSchema()` only when an external protocol needs JSON Schema.
- Pair schema validation with authorization, capability checks, semantic validation, and a clear retry/approval path; schemas alone cannot provide them.

## Sources
- Zod, [Defining schemas](https://zod.dev/api) — discriminated unions and type narrowing.
- Zod, [JSON Schema](https://zod.dev/json-schema) — stable `z.toJSONSchema()` conversion; `z.fromJSONSchema()` is experimental.
- A2A, [Protocol specification](https://github.com/a2aproject/A2A/blob/main/docs/specification.md) — required-field validation, schema validation, authorization, and injection protections.
