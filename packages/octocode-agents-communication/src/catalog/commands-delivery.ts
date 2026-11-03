/** The delivery commands: input and result schemas, help text and CLI flags. */
import { z } from 'zod';
import { ref, type CommandDefinition } from './registry.ts';
import { claudeInboundSchema, dispatchOutcomeSchema, hookSpecificOutputSchema, transportSchema, usageDataSchema } from './defs.ts';

export const deliveryCommands: readonly CommandDefinition[] = [
  {
    name: "attach",
    description: "Bind existing receiver: raw; claude/grok same-user socket; codex owning loopback WebSocket/Unix socket; opencode idle workspace and loopback HTTP. OpenCode auth: listener env OPENCODE_SERVER_PASSWORD, OPENCODE_SERVER_USERNAME, OCTOCODE_OPENCODE_AUTH_ENDPOINT. Grok needs ACP bridge.",
    input: z.strictObject({
      transport: transportSchema.describe("Attached host transport."),
      endpoint: z.string().min(1).max(4096).describe("Host endpoint used for attached delivery.").optional(),
      vendorSession: z.string().min(1).max(256).describe("Exact host session ID bound to this identity.").optional(),
    }),
    usage: "attach [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      session: z.string(),
      transport: transportSchema,
      endpoint: z.string().optional(),
      updatedAt: z.int(),
      vendorSession: z.string().optional(),
      capabilities: z.strictObject({
        nativeInput: z.boolean(),
        passiveInjection: z.boolean().optional(),
        actionWake: z.boolean().optional(),
        acceptanceReceipt: z.string(),
        readReceipt: z.boolean(),
        wakePrerequisite: z.string().optional(),
        activeInjection: z.boolean().optional(),
      }),
      inbound: claudeInboundSchema.optional(),
    }).describe("The delivery binding of the identity."),
  },
  {
    name: "dispatch",
    description: "Submit one DB batch to the attached native receiver. Codex/OpenCode defer busy recipients. Submission is not handling; attempts never replay automatically.",
    input: z.strictObject({}),
    usage: "dispatch [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: ref(dispatchOutcomeSchema.describe("The final delivery pass: nothing is in flight.")),
  },
  {
    name: "hook",
    description: "Offer new host context; refresh presence. Formats: text (empty if idle), json (SDK), claude (UserPromptSubmit). Complete handled IDs; inspect failed delivery before retry.",
    input: z.strictObject({
      format: z.enum(["text","json","claude"]).describe("Host hook output format.").optional(),
      deferConfirm: z.boolean().describe("Acknowledge only after the host confirms submission.").optional(),
      consumer: z.string().min(1).max(256).describe("Stable host lifecycle owner for inspecting staged delivery receipts.").optional(),
      managed: z.boolean().describe("Host-managed lifecycle: renew presence and live leases without resuming expired identity.").optional(),
    // oxlint-disable-next-line unicorn/no-thenable -- "then" is the JSON Schema keyword of the conditional rule
    }).meta({"allOf":[{"if":{"properties":{"deferConfirm":{"const":true,"description":"Acknowledge only after the host confirms submission."}},"required":["deferConfirm"]},"then":{"properties":{"format":{"const":"json","description":"Host hook output format."}},"required":["format"]}}]}),
    usage: "hook [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      items: z.array(z.strictObject({
        id: z.int(),
        dispatchToken: z.string().optional(),
      })).optional(),
      context: z.string().optional(),
      action: z.literal(true).optional(),
      hookSpecificOutput: hookSpecificOutputSchema.optional(),
    }).describe("JSON of the raw hook: format json prints items, context and action; claude prints hookSpecificOutput; text prints plain text, not JSON."),
  },
  {
    name: "listen",
    description: "Maintain presence and dispatch native mail; raw uses hooks. No model calls or lease renewal.",
    input: z.strictObject({}),
    usage: "listen --session <id> --workspace <path> [--database <file>] [--duration-ms <ms>]",
    cliFlags: ["workspace","database","session","duration-ms"],
    result: z.union([z.strictObject({
        type: z.literal("listening"),
        session: z.string(),
        modelCalls: z.literal(0),
      }), dispatchOutcomeSchema]).describe("One JSON line per event: the start notice, then each delivery pass that submitted mail."),
  },
  {
    name: "retry_delivery",
    description: "Reoffer one of your unacknowledged messages after inspecting its staged or uncertain attempt; may duplicate an uncertain write. A submitted attempt was accepted by the host and is refused. reason is audited.",
    input: z.strictObject({
      message: z.int().min(1).describe("Exact message ID."),
      reason: z.string().min(1).max(256).describe("Reason for retrying delivery."),
    }),
    usage: "retry_delivery [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      ready: z.boolean(),
      retries: z.int(),
      exhausted: z.literal(true).optional(),
      retryAfterMs: z.int().optional(),
      guidance: z.string().optional(),
    }).describe("Whether the attempt was made ready again within the retry budget."),
  },
  {
    name: "record_usage",
    description: "Record host token counters once per key, without text. scope: request, turn or cumulative (never sum cumulative). Missing means unknown, not zero.",
    input: z.strictObject(usageDataSchema.shape),
    usage: "record_usage [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      recorded: z.boolean(),
    }).describe("Whether a new usage record was stored."),
  },
  {
    name: "confirm_delivery",
    description: "SDK context receipt: exact IDs and dispatch tokens required. Rejects stale tokens; work stays pending.",
    input: z.strictObject({
      items: z.array(z.strictObject({
        id: z.int().min(1).describe("Exact delivery item identifier."),
        dispatchToken: z.string().meta({"format":"uuid"}).describe("Exact dispatch token returned by the delivery operation."),
      })).min(1).max(16).describe("Exact delivery items and tokens returned by dispatch."),
    }),
    usage: "confirm_delivery [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: z.strictObject({
      submitted: z.literal(true),
    }).describe("Offered batch confirmed."),
  },
  {
    name: "host-hook",
    description: "Claude/Codex/Cursor/Grok stdin lifecycle JSON. Bind identity at session start; offer mail on supported post-tool events. Fails open; configure a timeout.",
    usage: "host-hook --vendor claude|codex|cursor|grok --workspace <path> [--database <file>]",
    cliFlags: ["workspace","database","vendor","session"],
    cliSchema: {"required":["vendor"],"properties":{"vendor":{"type":"string","enum":["claude","codex","cursor","grok"],"description":"Host receiving the hook configuration."}}},
    result: z.strictObject({
      continue: z.literal(true).optional(),
      additional_context: z.string().optional(),
      hookSpecificOutput: hookSpecificOutputSchema.optional(),
    }).describe("The hook output the host reads: empty when there is nothing to add."),
  },
  {
    name: "host-config",
    description: "Preview Claude/Codex/Cursor/Grok messaging hook config with 10s timeouts; writes no settings or DB. Merge into the documented host location; separate edit guards are optional.",
    usage: "host-config --vendor claude|codex|cursor|grok --workspace <path> [--database <file>]",
    cliFlags: ["workspace","database","vendor","session"],
    cliSchema: {"required":["vendor"],"properties":{"vendor":{"type":"string","enum":["claude","codex","cursor","grok"],"description":"Host receiving the hook configuration."}}},
    result: z.strictObject({
      version: z.literal(1).optional(),
      hooks: z.record(z.string(), z.array(z.record(z.string(), z.unknown()))),
    }).describe("Hook settings to merge into the host configuration."),
  },
];
