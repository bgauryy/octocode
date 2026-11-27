Load when defining a typed, validated message between agents.

# Typed messages

TypeScript types vanish at runtime, and a remote agent, tool, model, file or MCP server can return invalid data. Validate actual values at every boundary with Zod, and export JSON Schema when other languages need the contract. Examples use Zod 4 APIs.

Use a discriminated union ([schema](tool-descriptions.md#schema)) instead of one loose object with many optional fields:

```ts
import { z } from "zod";

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

Completed, blocked and failed work each get an explicit shape; TypeScript narrows on `status` and runtime validation rejects malformed results. `blocked` is an interruption requiring input or recovery, not a terminal outcome; preserve the callee's actual terminal state separately. A refused request also needs a stable code and one retry, approval or terminal action.

Validate sends with `.parse()` and receives with `.safeParse()` before routing, storage or tool use; reject malformed packets with a small structured error. The result example is an application contract, not a wire standard. Only export with `z.toJSONSchema()` when a protocol needs JSON Schema.

What validation buys is containment: a malformed or out-of-range message stops at the boundary and the failing field is named. Measured accuracy barely moves from typing alone, and forcing a format on the reasoning itself can lower accuracy on some models, so let the sender reason first and format the result as a separate step ([arXiv 2610.31280](https://arxiv.org/abs/2610.31280), [arXiv 2604.03616](https://arxiv.org/abs/2604.03616)).

Some Zod types and transforms have no faithful JSON Schema form. Zod-specific migration and API detail: the `zod` skill, if installed, or the Zod documentation.

## Sources

- [Zod documentation](https://zod.dev/) and [JSON Schema conversion](https://zod.dev/json-schema)
