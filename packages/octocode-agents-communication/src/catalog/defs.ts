/** Shared catalog definitions: each is emitted once under `$defs` and referenced by `$ref` everywhere it is used. */
import { z } from 'zod';
import { define } from './registry.ts';

export const statusSchema = define("status", z.enum(["available","busy","blocked","unknown"]));

export const pathKindSchema = define("pathKind", z.enum(["file","tree"]));

export const transportSchema = define("transport", z.enum(["raw","claude","codex","opencode","grok"]));

export const usageScopeSchema = define("usageScope", z.enum(["request","turn","cumulative"]));

export const activityViewSchema = define("activityView", z.enum(["files","commits","reflog"]));

export const presenceFilterSchema = define("presenceFilter", z.enum(["active","expired","all"]));

export const topicDataSchema = define("topicData", z.strictObject({
  topic: z.string(),
}));

export const leaseExpirySchema = define("leaseExpiry", z.strictObject({
  id: z.int(),
  expiresAt: z.int(),
}));

export const usageDataSchema = define("usageData", z.strictObject({
  key: z.string().min(1).max(256).describe("Stable idempotency key; reuse only for unchanged input."),
  scope: usageScopeSchema.describe("Usage accounting scope."),
  model: z.string().min(1).max(256).describe("Host model identifier.").optional(),
  inputTokens: z.int().min(0).describe("Consumed input tokens.").optional(),
  outputTokens: z.int().min(0).describe("Produced output tokens.").optional(),
  cachedInputTokens: z.int().min(0).describe("Input tokens served from cache.").optional(),
  contextTokens: z.int().min(0).describe("Tokens currently in model context.").optional(),
  cacheWriteTokens: z.int().min(0).describe("Tokens written to cache.").optional(),
}));

export const keySchema = define("key", z.string().min(1).max(256).describe("Idempotency key; reuse only for unchanged input."));

export const ttlMsSchema = define("ttlMs", z.int().min(1000).max(86400000).describe("Lifetime in milliseconds."));

export const reasoningSchema = define("reasoning", z.string().min(1).max(512).regex(/\S/u).describe("Purpose; ≤512 UTF-8 bytes."));

export const wakeSchema = define("wake", z.enum(["action","passive"]).describe("action may wake an idle recipient; passive waits for a host boundary. Default: action for requests, passive for notices."));

export const contextIdSchema = define("contextId", z.string().min(1).max(128).regex(/^[A-Za-z0-9._:-]+$/u).describe("Related-work ID; final replies inherit it."));

export const topicsSchema = define("topics", z.array(z.string().min(1).max(256)).max(32).describe("Subscribed topics."));

export const leaseIdSchema = define("leaseId", z.int().gt(0).max(Number.MAX_SAFE_INTEGER).describe("Exact owned lease ID."));

export const presenceSchema = define("presence", presenceFilterSchema.default("active").describe("Filter active, expired, or all presence."));

export const pathTargetSchema = define("pathTarget", z.strictObject({
  path: z.string().min(1).max(4096).describe("Workspace-relative path."),
  kind: pathKindSchema.default("file").describe("file or whole tree.").optional(),
}));

export const coordinateDataSchema = define("coordinateData", z.strictObject({
  name: z.string().min(1).max(256).describe("Stable agent or document name."),
  vendor: z.string().min(1).max(256).describe("Host vendor owning this identity."),
  vendorSession: z.string().min(1).max(256).describe("Optional host session ID; must match the bound identity.").optional(),
  task: z.string().max(256).describe("Current task visible to peers."),
  status: statusSchema.describe("Current presence status visible to peers."),
  expiresAt: z.int().min(0).describe("Presence deadline at this event; current liveness comes from peers."),
}));

export const leaseDataSchema = define("leaseData", z.strictObject({
  path: z.string().min(1).max(4096).describe("Workspace-relative file/tree; default workspace root."),
  kind: pathKindSchema.describe("File or whole directory tree scope."),
  reasoning: reasoningSchema.describe("Why, briefly; ≤512 UTF-8 bytes."),
  leaseId: z.int().min(1).describe("Exact owned lease ID."),
  expiresAt: z.int().min(0).describe("Expiry timestamp in epoch milliseconds."),
}));

export const attachmentDataSchema = define("attachmentData", z.strictObject({
  transport: z.string().describe("Attached host transport."),
  endpoint: z.string().nullable().describe("Host endpoint used for attached delivery."),
}));

export const dispatchDataSchema = define("dispatchData", z.strictObject({
  messageId: z.int().min(1).describe("Exact durable message ID."),
  token: z.string().describe("Exact delivery attempt token."),
  transport: z.string().describe("Attached host transport."),
  error: z.string().nullable().describe("Delivery failure diagnostic.").optional(),
}));

export const deliveryDataSchema = define("deliveryData", z.strictObject({
  messageId: z.int().min(1).describe("Exact durable message ID."),
}));

export const continuationSchema = define("continuation", z.strictObject({
  command: z.string().describe("Command to run next."),
  input: z.record(z.string(), z.unknown()).describe("Input to pass unchanged."),
}).describe("Executable continuation: run command with input unchanged."));

export const budgetSchema = define("budget", z.strictObject({
  targetBytes: z.int(),
  reason: z.string(),
}).describe("A single oversized row was returned intact; the target size was exceeded on purpose."));

export const recordEnvelopeSchema = define("recordEnvelope", z.strictObject({
  recordId: z.int().min(1),
  path: z.string().min(1),
  from: z.string().min(1).max(256),
  to: z.string().nullable(),
  type: z.string().min(1),
  timestamp: z.int().min(0),
  branch: z.string().optional(),
  data: z.record(z.string(), z.unknown()),
}));

export const documentContextSchema = define("documentContext", z.strictObject({
  summary: z.string().min(1).max(320).describe("One useful fact and why it matters."),
  path: z.string().min(1).max(4096).default(".").describe("Workspace-relative file/tree; default workspace root.").optional(),
  kind: pathKindSchema.default("tree").optional(),
  branch: z.string().min(1).max(255).describe("Exact branch; omit for branch-independent facts.").optional(),
  ttlMs: z.int().min(1000).max(604800000).default(86400000).describe("Discovery lifetime; default 1 day, max 7. Retries do not extend it.").optional(),
  expiresAt: z.int().min(0).optional(),
}));

export const documentDataSchema = define("documentData", z.strictObject({
  name: z.string(),
  path: z.string(),
  author: z.string(),
  reasoning: z.string(),
  bytes: z.int().min(0),
  sha256: z.string(),
  context: documentContextSchema.optional(),
  supersedes: z.string().optional(),
}));

export const checkpointSchema = define("checkpoint", z.strictObject({
  recordId: z.int(),
  timestamp: z.int(),
  data: z.record(z.string(), z.unknown()),
}).describe("The latest checkpoint record of the identity."));

export const identitySchema = define("identity", z.strictObject({
  id: z.string(),
  workspace: z.string(),
  name: z.string(),
  vendor: z.string(),
  vendorSession: z.string().optional(),
  branch: z.string().optional(),
  expiresAt: z.int(),
  task: z.string(),
  status: statusSchema,
  coordinationScope: z.string().describe("Present on binding.").optional(),
  active: z.boolean().describe("Present on binding.").optional(),
  checkpoint: checkpointSchema.optional(),
}).describe("A session identity row."));

export const leaseSchema = define("lease", z.strictObject({
  id: z.int(),
  path: z.string(),
  kind: pathKindSchema,
  owner: z.string(),
  reasoning: z.string(),
  acquiredAt: z.int(),
  refreshedAt: z.int(),
  expiresAt: z.int().optional(),
}).describe("A lease; expiresAt is on the response of a multi-path lock."));

export const lockRefusedSchema = define("lockRefused", z.strictObject({
  ok: z.literal(false),
  conflict: z.strictObject({
    id: z.int().describe("Lease ID; absent when the blocker is a queued wait.").optional(),
    waitId: z.int().optional(),
    kind: pathKindSchema,
    acquiredAt: z.int(),
    refreshedAt: z.int(),
    expiresAt: z.int(),
    reasoning: z.string(),
    path: z.string(),
  }),
  owner: z.strictObject({
    id: z.string(),
    name: z.string(),
    vendor: z.string(),
    expiresAt: z.int(),
  }),
  retryAfterMs: z.int(),
  heldLeaseIds: z.array(z.int()),
  guidance: z.string().optional(),
  next: continuationSchema.optional(),
  queued: z.literal(true).optional(),
  wait: leaseExpirySchema.optional(),
}).describe("Nothing acquired: the blocking lease and what to do next."));

export const partsSchema = define("parts", z.array(z.strictObject({
  text: z.string().min(1).regex(/\S/u).optional(),
  data: z.any().meta({}).optional(),
  url: z.string().min(1).regex(/\S/u).describe("Reference only; never fetched automatically.").optional(),
  mediaType: z.string().min(1).optional(),
  filename: z.string().min(1).optional(),
}).meta({"oneOf":[{"required":["text"]},{"required":["data"]},{"required":["url"]}]})).min(1).describe("Ordered content. Data is opaque; no automatic actions."));

export const outcomeSchema = define("outcome", z.enum(["completed","failed","rejected","canceled"]).describe("Final work outcome."));

export const inboxPageSchema = define("inboxPage", z.strictObject({
  items: z.array(z.strictObject({
    id: z.int(),
    sender: z.string(),
    senderName: z.string(),
    reasoning: z.string(),
    topic: z.string().optional(),
    expiresAt: z.int(),
    wake: wakeSchema,
    contextId: z.string().optional(),
    replyTo: z.int().optional(),
    replyRequired: z.boolean(),
    parts: partsSchema,
    outcome: outcomeSchema.optional(),
  })),
  next: continuationSchema.optional(),
  budget: budgetSchema.optional(),
}).describe("Pending deliveries in message ID order."));

export const storageSchema = define("storage", z.strictObject({
  pageSize: z.int(),
  pageCount: z.int(),
  freePages: z.int(),
  logicalBytes: z.int(),
  reusableBytes: z.int(),
  mainFileBytes: z.int(),
  walBytes: z.int(),
}).describe("SQLite storage figures."));

export const claudeInboundSchema = define("claudeInbound", z.strictObject({
  outcome: z.string(),
  reason: z.string(),
  ownChild: z.boolean(),
}).describe("Predicted Claude Code inbound decision."));

export const dispatchOutcomeSchema = define("dispatchOutcome", z.strictObject({
  submitted: z.int(),
  transport: z.string().optional(),
  messages: z.array(z.int()).optional(),
  modelCalls: z.int().optional(),
  receipt: z.string().optional(),
  recipientTurnRequested: z.boolean().optional(),
  stopReason: z.string().optional(),
  inbound: claudeInboundSchema.optional(),
  pending: z.int().optional(),
  deferred: z.string().optional(),
  next: continuationSchema.optional(),
}).describe("One delivery pass."));

export const hookSpecificOutputSchema = define("hookSpecificOutput", z.strictObject({
  hookEventName: z.enum(["SessionStart","PostToolUse","PostToolUseFailure","UserPromptSubmit"]),
  additionalContext: z.string(),
}).describe("Host hook context envelope."));

export const toolSchema = define("tool", z.strictObject({
  name: z.string(),
  annotations: z.strictObject({
    readOnlyHint: z.boolean(),
    destructiveHint: z.boolean(),
    openWorldHint: z.boolean(),
  }),
  description: z.string(),
  inputSchema: z.record(z.string(), z.unknown()),
}).describe("An MCP tool definition."));

export const databaseIdentitySchema = define("databaseIdentity", z.strictObject({
  applicationId: z.int(),
  schemaVersion: z.int(),
  schemaSha256: z.string(),
  relationships: z.array(z.strictObject({
    table: z.string(),
    columns: z.array(z.string()),
    references: z.strictObject({
      table: z.string(),
      columns: z.array(z.string()),
    }),
    onDelete: z.string(),
  })),
  sql: z.string(),
  leasePathComparison: z.strictObject({
    algorithm: z.string(),
    unicodeVersion: z.array(z.int()),
    normalizationUnicodeVersion: z.array(z.int()),
    policy: z.string(),
  }),
}).describe("The database schema identity."));
