/* v8 ignore file -- exercised through built CLI and isolated-package subprocess tests */
import { readFile } from 'node:fs/promises';
import { z } from 'zod';
import { memorySchemas } from './definitions-memory.js';
import { workSchemas } from './definitions-work.js';
import { operationSchemas } from './definitions-operations.js';
import { examples as coreExamples } from './examples.js';
import { integrationExamples } from './examples-integration.js';
import { awarenessEntityCatalog } from './entities.js';
import { commandIndex } from './command-catalog.js';
import { integrationSchemas } from './definitions-integration.js';
import { CLI_REQUIRED, projectCliProperties } from './cli-contract.js';

export const schemas = {
  ...memorySchemas,
  ...workSchemas,
  ...operationSchemas,
  ...integrationSchemas,
};
export const examples = { ...coreExamples, ...integrationExamples };
export type SchemaName = keyof typeof schemas;

const listableSchemas = [
  ...Object.keys(integrationSchemas),
  "memory_record", "memory_recall",
  "attend", "query",
  "workspace_status", "export_harness", "session_capture",
  "plan", "task", "work", "lock_acquire", "lock_wait", "lock_prune", "lock_release", "verify", "verify_audit",
  "forget_memory", "memory_lifecycle", "refinement", "refine_query", "refine_delete",
  "agent_registry", "agent_signal", "signal_prune",
  "mine_weakness", "developer_review", "doc_staleness", "docs_catalog", "digest", "reflect", "awareness_config",
];


const CORE_NOUNS = new Set(["verify", "attend", "plan", "task", "work", "memory", "signal", "query"]);
// Rare/expert/redundant commands stay fully available under `--all` and
// `<command> --help`, but are hidden from the default lobby catalog to keep the
// agent-facing surface small. Removing them here removes catalog verbosity, not
// capability.
const COMPACT_HIDE = new Set<string>([
  "reflect mine-weakness", "reflect export-harness", "reflect developer-review",
  "query developer-review", "docs staleness",
  "schema list", "schema json-schema", "schema example", "schema validate",
]);

function groupedCommandIndex() {
  const grouped: Record<"core" | "advanced", Record<string, string[]>> = { core: {}, advanced: {} };
  for (const row of commandIndex) {
    if (COMPACT_HIDE.has(row.command)) continue;
    const [noun, ...rest] = row.command.split(" ");
    const tier = CORE_NOUNS.has(noun!) ? "core" : "advanced";
    (grouped[tier][noun!] ??= []).push(rest.length > 0 ? rest.join(" ") : noun === "query" ? "<view>" : "<direct>");
  }
  return grouped;
}

function printJson(payload: unknown, compact = false): void {
  console.log(JSON.stringify(payload, null, compact ? 0 : 2));
}

function usage() {
  return `Usage:
  npx @octocodeai/octocode-awareness schema commands [--compact] [--all] [--examples]
  npx @octocodeai/octocode-awareness schema command <noun> [action] [--compact]
  npx @octocodeai/octocode-awareness schema entities [--compact] [--all]
  npx @octocodeai/octocode-awareness schema list
  npx @octocodeai/octocode-awareness schema json-schema <schema-name>
  npx @octocodeai/octocode-awareness schema example <schema-name>
  npx @octocodeai/octocode-awareness schema validate <schema-name> <json-file|->`;
}

function toJsonSchema(schema: z.ZodType) {
  if (typeof z.toJSONSchema === "function") {
    return z.toJSONSchema(schema);
  }
  throw new Error("This script requires Zod v4 with z.toJSONSchema().");
}

function cliCommandSchema(commandName: string): Record<string, unknown> | null {
  const row = commandIndex.find((candidate) => candidate.command === commandName);
  if (!row?.schema) return null;
  const schema = schemas[row.schema as SchemaName];
  if (!schema) return null;
  const output = structuredClone(toJsonSchema(schema)) as Record<string, unknown>;
  const properties = output.properties as Record<string, unknown> | undefined;
  const action = commandName.split(" ")[1];
  if (properties && action && properties.action) delete properties.action;
  let aliases: Record<string, string> = {};
  if (properties) {
    aliases = projectCliProperties(properties, commandName);
  }
  const existingRequired = Array.isArray(output.required)
    ? (output.required as string[])
      .filter((field) => field !== "action")
      .map((field) => aliases[field] ?? field)
      .filter((field) => properties?.[field] && !Object.hasOwn(properties[field] as object, "default"))
    : [];
  const required = [...new Set([...existingRequired, ...(CLI_REQUIRED[commandName] ?? [])])];
  if (required.length > 0) output.required = required;
  else delete output.required;
  output["x-cli-command"] = commandName;
  output["x-cli-example"] = row.example;
  output["x-cli-note"] = "CLI flags use kebab-case; repeat array flags. The router injects the action.";
  return output;
}

function parseJson(input: string): unknown {
  try {
    return JSON.parse(input);
  } catch (error) {
    throw new Error(`Invalid JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function formatZodError(error: z.ZodError) {
  return error.issues.map((issue) => ({
    path: issue.path.join(".") || "<root>",
    code: issue.code,
    message: issue.message,
  }));
}

function printJsonError(payload: Record<string, unknown>, code = 2, compact = false): number {
  console.log(JSON.stringify({ ok: false, ...payload }, null, compact ? 0 : 2));
  return code;
}

export async function runSchemaCli(argv: string[]): Promise<number> {
  const compact = argv.includes("--compact") || process.env.OCTOCODE_AWARENESS_COMPACT === "1";
  const includeExamples = argv.includes("--examples");
  const includeAll = argv.includes("--all");
  const filteredArgv = argv.filter((arg) => arg !== "--compact" && arg !== "--examples" && arg !== "--all");
  const [command, schemaName, file] = filteredArgv;

  if (!command || command === "--help" || command === "-h") {
    console.log(usage());
    return 0;
  }

  if (command === "commands") {
    const commands = includeAll
      ? (includeExamples ? commandIndex : commandIndex.map(({ command: cmd, schema }) => ({ command: cmd, schema })))
      : groupedCommandIndex();
    printJson({
      ok: true,
      hint: includeAll
        ? "Flat command detail. Use `<command> --help` or `schema command <noun> [action]` for one exact contract."
        : "Core first; minimum agent loop is attend -> work start -> work end -> verify mark -> verify audit. Follow attend.next; pass --all for the flat catalog.",
      commands,
    }, compact);
    return 0;
  }

  if (command === "command") {
    const requestedCommandName = [schemaName, file].filter(Boolean).join(" ");
    const commandName = requestedCommandName;
    const commandSchema = cliCommandSchema(commandName);
    if (!commandSchema) {
      return printJsonError({
        error_code: "UNKNOWN_CLI_COMMAND",
        error: `Unknown or schema-less CLI command: ${requestedCommandName || "<missing>"}`,
        hint: "Use `schema commands --all --compact` to list command names.",
      }, 1, compact);
    }
    printJson(commandSchema, compact);
    return 0;
  }

  if (command === "list") {
    printJson(listableSchemas, compact);
    return 0;
  }

  if (command === "entities") {
    const catalog = awarenessEntityCatalog();
    if (includeAll) {
      printJson({ ok: true, kind: "awareness.entities", ...catalog }, compact);
    } else {
      const families = new Map<string, string[]>();
      for (const entity of catalog.entities) {
        const names = families.get(entity.family) ?? [];
        names.push(entity.name);
        families.set(entity.family, names);
      }
      printJson({
        ok: true,
        kind: "awareness.entities",
        storage: catalog.storage,
        families: [...families.entries()].map(([family, entities]) => ({ family, entities })),
        hint: "Pass --all for owner and relation kind per entity.",
      }, compact);
    }
    return 0;
  }

  const knownSchemaName = listableSchemas.includes(schemaName as SchemaName)
    ? schemaName as SchemaName
    : undefined;
  const schema = knownSchemaName ? schemas[knownSchemaName] : undefined;
  if (!schema) {
    return printJsonError({
      error_code: "UNKNOWN_SCHEMA",
      error: `Unknown schema: ${schemaName || "<missing>"}`,
      hint: "Use one of the schemas returned by `schema list`.",
      ...(compact ? {} : { known_schemas: listableSchemas }),
    }, 1, compact);
  }

  if (command === "json-schema") {
    printJson(toJsonSchema(schema), compact);
    return 0;
  }

  if (command === "example") {
    printJson(examples[knownSchemaName!], compact);
    return 0;
  }

  if (command === "validate") {
    if (!file) {
      return printJsonError({
        error_code: "MISSING_INPUT",
        error: "Missing <json-file|->.",
        hint: "Use `schema validate <schema-name> <json-file|->`.",
      }, 1, compact);
    }
    const raw = file === "-" ? await readStdin() : await readFile(file, "utf8");
    let parsed;
    try {
      parsed = parseJson(raw);
    } catch (error) {
      return printJsonError({
        error_code: "INVALID_JSON",
        schema: schemaName,
        error: error instanceof Error ? error.message : String(error),
        hint: "Pass valid JSON matching the selected schema.",
      }, 1, compact);
    }
    const result = schema.safeParse(parsed);
    if (!result.success) {
      return printJsonError({
        schema: schemaName,
        issues: formatZodError(result.error),
      }, 1, compact);
    }
    printJson({ ok: true, schema: schemaName, data: result.data }, compact);
    return 0;
  }

  return printJsonError({
    error_code: "UNKNOWN_COMMAND",
    error: `Unknown command: ${command}`,
    hint: usage(),
  }, 1, compact);
}

async function readStdin() {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString("utf8");
}
