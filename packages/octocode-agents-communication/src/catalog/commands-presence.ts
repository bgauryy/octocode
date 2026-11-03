/** The presence commands: input and result schemas, help text and CLI flags. */
import { z } from 'zod';
import { ref, type CommandDefinition } from './registry.ts';
import { continuationSchema, identitySchema, statusSchema } from './defs.ts';

export const presenceCommands: readonly CommandDefinition[] = [
  {
    name: "join",
    description: "Register identity; reuse supplied ID. Presence lasts 60s: listen or heartbeat every 15s.",
    input: z.strictObject({
      name: z.string().min(1).max(256).describe("Unique live peer name; conflicts get a numeric suffix. Use the returned name."),
      vendor: z.string().min(1).max(256).describe("Host vendor owning this identity."),
      vendorSession: z.string().min(1).max(256).describe("Exact current host session ID; omit if unknown.").optional(),
      task: z.string().max(256).describe("Task shown to peers.").optional(),
      status: statusSchema.describe("Status shown to peers.").optional(),
      branch: z.string().max(256).nullable().describe("Git branch.").optional(),
    }),
    usage: "join [json] --workspace <path> [--database <file>]",
    cliFlags: ["workspace","database"],
    result: z.strictObject({
      id: z.string(),
      name: z.string(),
      vendor: z.string(),
      vendorSession: z.string().optional(),
      expiresAt: z.int(),
    }).describe("The new identity; name may carry a -2, -3 suffix."),
  },
  {
    name: "heartbeat",
    description: "Renew live presence (60s, or ttlMs up to 10 min); optionally update task/status. renewLeases extends live owned leases; expired identities need resume and fresh leases.",
    input: z.strictObject({
      vendorSession: z.string().min(1).max(256).describe("Native session ID; use attach to change an existing receiver.").optional(),
      task: z.string().max(256).describe("Current task visible to peers.").optional(),
      status: statusSchema.describe("Current presence status visible to peers.").optional(),
      ttlMs: z.int().min(60000).max(600000).describe("Presence lifetime in ms (default 60000, max 10 min, the lease cap). Raw CLI agents set it to cover their longest turn.").optional(),
      renewLeases: z.boolean().describe("Extend currently live owned leases to at least the presence lifetime; never revive expired leases. Raw CLI identities and host-managed owners both use it.").optional(),
      branch: z.string().max(256).nullable().describe("Git branch associated with this entry.").optional(),
      name: z.string().min(1).max(256).describe("Rename; rejected while a live peer uses it.").optional(),
    }),
    usage: "heartbeat [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      alive: z.literal(true),
      expiresAt: z.int(),
      renewedLeases: z.int().describe("Present with renewLeases.").optional(),
    }).describe("Presence renewed."),
  },
  {
    name: "set_status",
    description: "Update your advertised task or status; does not renew presence.",
    input: z.strictObject({
      task: z.string().max(256).describe("Task shown to peers.").optional(),
      status: statusSchema.describe("Status shown to peers.").optional(),
    }).meta({"minProperties":1}),
    usage: "set_status [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      id: z.string(),
      task: z.string(),
      status: statusSchema,
    }).describe("The advertised task and status."),
  },
  {
    name: "resume",
    description: "Resume expired same-vendor identity, deleting old leases; lock fresh before edits. Live sessions cannot be taken over.",
    input: z.strictObject({
      vendor: z.string().min(1).max(256).describe("Host vendor owning this identity."),
    }),
    usage: "resume [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: ref(identitySchema.describe("The revived identity with its latest checkpoint.")),
  },
  {
    name: "leave",
    description: "Expire your session and release its leases and delivery claims.",
    input: z.strictObject({}),
    usage: "leave [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      left: z.literal(true),
    }).describe("Identity ended; leases, subscriptions and claims released."),
  },
  {
    name: "prune",
    description: "Remove one batch of expired leases in this workspace; follow next. History is kept.",
    input: z.strictObject({}),
    usage: "prune [json] --workspace <path> [--database <file>]",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      removed: z.int(),
      next: continuationSchema.optional(),
    }).describe("Expired leases removed; next continues when the batch was full."),
  },
];
