/** The system commands: input and result schemas, help text and CLI flags. */
import { z } from 'zod';
import { ref, type CommandDefinition } from './registry.ts';
import { databaseIdentitySchema, inboxPageSchema, toolSchema } from './defs.ts';

export const systemCommands: readonly CommandDefinition[] = [
  {
    name: "db info",
    description: "Inspect the resolved path and compatibility without creating a database.",
    input: z.strictObject({}),
    usage: "db info [--database <file>] [--workspace <path>]",
    cliFlags: ["workspace","database"],
    result: z.strictObject({
      path: z.string(),
      workspace: z.string(),
      exists: z.boolean(),
      expectedApplicationId: z.int(),
      expectedSchemaVersion: z.int(),
      expectedSchemaSha256: z.string(),
      compatible: z.boolean(),
      applicationId: z.int().optional(),
      schemaVersion: z.int().optional(),
      sqliteVersion: z.string().optional(),
      journalMode: z.string().optional(),
      schemaSha256: z.string().optional(),
    }).describe("Database compatibility; the applied-schema fields are present when the file exists."),
  },
  {
    name: "db protocol",
    description: "SQLite-only client protocol and DDL, for other adapters. Opens no database.",
    usage: "db protocol",
    cliFlags: ["workspace","database"],
    result: z.strictObject({
      protocol: z.string().describe("The DB.md protocol text."),
      database: databaseIdentitySchema,
    }).describe("The SQL protocol and schema identity."),
  },
  {
    name: "inbox wait",
    description: "Wait for inbox messages or timeout, without acknowledging. Explicit recovery only.",
    input: z.strictObject({
      timeoutMs: z.int().min(0).max(60000).default(30000).describe("Maximum wait in milliseconds.").optional(),
      after: z.int().min(0).default(0).describe("Exclusive continuation cursor copied from next.input; keep the same filters.").optional(),
    }),
    usage: "inbox wait [json] --session <id>",
    cliFlags: ["workspace","database","session"],
    result: ref(inboxPageSchema.describe("Pending deliveries once any exist or the timeout passed (then items is empty).")),
  },
  {
    name: "mcp",
    description: "Typed stdio capability tools. Unbound connections expose init; --managed owns presence/cleanup and --session borrows the host identity. --tools limits capabilities.",
    input: z.strictObject({}),
    usage: "mcp --workspace <path> [--database <file>] [--tools <names>] [--managed --vendor <host>] [--name <name> | --session <id>]",
    cliFlags: ["workspace","database","session","managed","vendor","name","tools"],
    cliSchema: {"oneOf":[{"properties":{"managed":{"const":false}},"not":{"anyOf":[{"required":["name"]},{"required":["vendor"]}]}},{"properties":{"managed":{"const":true}},"required":["managed","vendor"],"oneOf":[{"required":["name"],"not":{"required":["session"]}},{"required":["session"],"not":{"required":["name"]}}]}]},
    result: z.union([z.strictObject({
        jsonrpc: z.literal("2.0"),
        id: z.union([z.string(), z.int(), z.null()]),
        result: z.record(z.string(), z.unknown()),
      }).describe("A JSON-RPC success response."), z.strictObject({
        jsonrpc: z.literal("2.0"),
        id: z.union([z.string(), z.int(), z.null()]),
        error: z.strictObject({
          code: z.int(),
          message: z.string(),
        }),
      }).describe("A JSON-RPC error response.")]).describe("One JSON-RPC response frame per request on standard output."),
  },
  {
    name: "run",
    description: "Start managed Codex/Claude/Pi on request; host owns presence/delivery. --session resumes same-vendor expiry unless delivery is staged. Existing conversations: attach. Pi needs Node/model. --tools selects tools.",
    input: z.strictObject({
      vendor: z.enum(["codex","claude","pi"]).describe("Host vendor owning this identity."),
      model: z.string().min(1).max(256).describe("Host model identifier."),
      name: z.string().min(1).max(256).describe("Stable agent or document name.").optional(),
      prompt: z.string().min(1).describe("User task starting the managed worker."),
      durationMs: z.int().min(1000).max(86400000).describe("Maximum lifetime in milliseconds.").optional(),
      trace: z.boolean().describe("Emit protocol trace diagnostics.").optional(),
    }),
    usage: "run --vendor codex|claude|pi --model <model> --prompt <task> [--name <name>] [--duration-ms <ms>] [--trace] --workspace <path> [--database <file>] [--session <expired-id>] [--tools <comma-separated-tool-names>]",
    cliFlags: ["workspace","database","session","vendor","model","name","prompt","duration-ms","trace","tools"],
    result: z.union([z.strictObject({
        type: z.literal("ready"),
        session: z.string(),
        vendor: z.string(),
        pid: z.int(),
        leases: z.literal("managed"),
      }), z.strictObject({
        type: z.literal("text"),
        text: z.string(),
      }), z.strictObject({
        type: z.literal("turn-completed"),
        vendor: z.string(),
      }), z.strictObject({
        type: z.literal("protocol"),
        method: z.string().optional(),
        itemType: z.string().optional(),
        status: z.string().optional(),
      }), z.strictObject({
        type: z.literal("delivery"),
        messages: z.array(z.int()),
      }), z.strictObject({
        type: z.literal("usage"),
        vendor: z.string(),
        scope: z.string(),
        messageId: z.int().optional(),
        usage: z.record(z.string(), z.unknown()),
      })]).describe("One JSON line per worker event."),
  },
  {
    name: "skill",
    description: "Read the companion skill; --vendor <host> returns its body without install frontmatter.",
    usage: "skill",
    cliFlags: ["workspace","database","vendor"],
    result: z.strictObject({
      instructions: z.string(),
      packageRoot: z.string(),
      referenceRoot: z.string(),
    }).describe("The operating guide and where the package lives."),
  },
  {
    name: "schema",
    description: "Discover commands, tools or record types without opening a database; schema types --compact gives every field, schema type <name> gives its schema and example.",
    usage: "schema [<command> | types [--compact] | type <name> | tools]",
    cliFlags: ["workspace","database","compact","tools"],
    result: z.strictObject({
      commands: z.array(z.strictObject({
        name: z.string(),
        description: z.string(),
        usage: z.string(),
        cliFlags: z.array(z.string()),
        inputSchema: z.record(z.string(), z.unknown()).optional(),
        cliSchema: z.record(z.string(), z.unknown()).optional(),
        annotations: z.record(z.string(), z.unknown()).optional(),
      })),
      tools: z.array(toolSchema),
      recordTypes: z.array(z.strictObject({
        type: z.string(),
        writer: z.string(),
        description: z.string(),
        dataSchema: z.record(z.string(), z.unknown()),
        example: z.record(z.string(), z.unknown()),
        routing: z.string(),
        fetch: z.record(z.string(), z.unknown()),
      })),
      recordEnvelope: z.record(z.string(), z.unknown()),
      results: z.record(z.string(), z.unknown()).describe("The result schema of each command, keyed by command name."),
      database: databaseIdentitySchema,
      pagination: z.strictObject({
        maxItems: z.int(),
        targetBytes: z.int(),
        continuation: z.string(),
      }),
    }).describe("The whole catalog."),
  },
];
