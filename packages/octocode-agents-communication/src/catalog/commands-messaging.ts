/** The messaging commands: input and result schemas, help text and CLI flags. */
import { z } from 'zod';
import { mutating, readOnly, ref, type CommandDefinition } from './registry.ts';
import { budgetSchema, contextIdSchema, continuationSchema, inboxPageSchema, keySchema, outcomeSchema, partsSchema, reasoningSchema, statusSchema, topicsSchema, ttlMsSchema, wakeSchema } from './defs.ts';

export const messagingCommands: readonly CommandDefinition[] = [
  {
    name: "peers",
    description: "Find live repository peers, including linked worktrees, with IDs, names, branches, tasks and status.",
    input: z.strictObject({
      after: z.string().describe("Cursor from next.input.").optional(),
    }),
    annotations: readOnly,
    usage: "peers [json] --workspace <path> [--database <file>]",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      items: z.array(z.strictObject({
        id: z.string(),
        workspace: z.string(),
        coordinationScope: z.string().optional(),
        name: z.string(),
        vendor: z.string(),
        vendorSession: z.string().optional(),
        branch: z.string().optional(),
        task: z.string(),
        status: statusSchema,
        expiresAt: z.int(),
      })),
      next: continuationSchema.optional(),
      budget: budgetSchema.optional(),
    }).describe("Live peers of the repository, by ID."),
  },
  {
    name: "send_message",
    description: "Send to fixed recipients. Returns a durable receipt; handling requires recipient completion.",
    input: z.strictObject({
      to: z.string().min(1).max(256).describe("Peer name/ID; \"*\" snapshots live repository peers except you.").optional(),
      topic: z.string().min(1).max(256).describe("Snapshot current subscribers; excludes to.").optional(),
      parts: partsSchema,
      replyRequired: z.boolean().default(false).describe("Require a final result from each recipient.").optional(),
      key: keySchema.optional(),
      ttlMs: ttlMsSchema.optional(),
      reasoning: reasoningSchema,
      wake: wakeSchema.optional(),
      contextId: contextIdSchema.optional(),
    }).meta({"oneOf":[{"required":["to"]},{"required":["topic"]}]}),
    annotations: mutating,
    usage: "send_message [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      id: z.int().describe("Message ID."),
      recipients: z.int(),
      recipientOffline: z.literal(true).optional(),
    }).describe("Receipt of the stored message."),
  },
  {
    name: "inbox",
    description: "Read pending mail without acknowledging it. For history, use records/read.",
    input: z.strictObject({
      after: z.int().min(0).describe("Cursor from next.input.").optional(),
      message: z.int().min(1).describe("Message ID.").optional(),
    }).meta({"not":{"required":["after","message"]}}),
    annotations: readOnly,
    usage: "inbox [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: ref(inboxPageSchema.describe("Pending deliveries; follow next.")),
  },
  {
    name: "complete",
    description: "Handle notices/results silently by message or messages. For requests, atomically reply with message, outcome and reply; retries must be unchanged.",
    input: z.strictObject({
      message: z.int().gt(0).describe("Message ID.").optional(),
      messages: z.array(z.int().gt(0)).min(1).max(100).meta({"uniqueItems":true}).describe("Handled FYI IDs; no reply.").optional(),
      reply: partsSchema.optional(),
      outcome: outcomeSchema.optional(),
      reasoning: reasoningSchema.describe("Reply purpose only; omit for silent completion.").optional(),
    }).meta({"dependentRequired":{"reply":["message","outcome"],"outcome":["message","reply"],"reasoning":["reply"]},"oneOf":[{"required":["message"]},{"required":["messages"]}]}),
    annotations: mutating,
    usage: "complete <json> --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.xor([z.strictObject({
        completed: z.literal(true),
        count: z.int(),
      }).describe("FYIs acknowledged."), z.strictObject({
        id: z.int().describe("Reply message ID."),
        recipients: z.int(),
        recipientOffline: z.literal(true).describe("The reply's recipient session had expired when it was sent.").optional(),
        completed: z.literal(true),
        count: z.literal(1),
      }).describe("Final result stored and this recipient acknowledged atomically; other recipients remain independent.")]).describe("Either an acknowledgement of read messages or the receipt of a stored final reply."),
  },
  {
    name: "subscribe",
    description: "Replace subscriptions for future mail; [] unsubscribes. Pending mail stays; no backfill.",
    input: z.strictObject({
      topics: topicsSchema,
    }),
    annotations: mutating,
    usage: "subscribe [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      subscribed: z.literal(true),
    }).describe("Topic subscriptions replaced."),
  },
];
