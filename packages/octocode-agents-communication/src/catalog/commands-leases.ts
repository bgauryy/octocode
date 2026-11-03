/** The leases commands: input and result schemas, help text and CLI flags. */
import { z } from 'zod';
import { mutating, type CommandDefinition } from './registry.ts';
import { leaseIdSchema, leaseSchema, lockRefusedSchema, pathKindSchema, reasoningSchema } from './defs.ts';

export const leasesCommands: readonly CommandDefinition[] = [
  {
    name: "lock",
    description: "Reserve one file or tree in this worktree before editing. Only ok:true grants a lease. On conflict, next retries with wait:true to queue; a later inbox notice reports the grant. Use lock_many for an atomic set.",
    input: z.strictObject({
      path: z.string().min(1).max(4096).describe("Repository path."),
      kind: pathKindSchema.describe("file or whole tree.").optional(),
      ttlMs: z.int().min(1000).max(600000).describe("Lifetime in ms; renew before expiry.").optional(),
      reasoning: reasoningSchema,
      wait: z.boolean().describe("Queue behind conflicts instead of failing.").optional(),
    }),
    annotations: mutating,
    usage: "lock [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.xor([z.strictObject({
        ok: z.literal(true),
        lease: leaseSchema,
      }).describe("Lease acquired."), lockRefusedSchema]).describe("Lease acquired, or the conflict and how to queue."),
  },
  {
    name: "renew",
    description: "Extend a live owned lease; renewed:false requires reacquisition.",
    input: z.strictObject({
      leaseId: leaseIdSchema,
      ttlMs: z.int().min(1000).max(600000).describe("Lifetime in milliseconds.").optional(),
    }),
    annotations: mutating,
    usage: "renew [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.xor([z.strictObject({
        renewed: z.literal(true),
        refreshedAt: z.int(),
        expiresAt: z.int(),
      }), z.strictObject({
        renewed: z.literal(false),
        guidance: z.string(),
      })]).describe("Lease extended, or a refusal to reacquire."),
  },
  {
    name: "unlock",
    description: "Release a live owned lease; queued contenders may then acquire it.",
    input: z.strictObject({
      leaseId: leaseIdSchema,
    }),
    annotations: mutating,
    usage: "unlock [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      released: z.boolean(),
    }).describe("Whether an owned live lease was released."),
  },
];
