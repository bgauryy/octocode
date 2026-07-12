/* v8 ignore file -- exercised through built CLI and isolated-package subprocess tests */
import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { z } from 'zod';
import { memorySchemas } from './definitions-memory.js';
import { workSchemas } from './definitions-work.js';
import { operationSchemas } from './definitions-operations.js';
import { examples } from './examples.js';

export const schemas = {
  ...memorySchemas,
  ...workSchemas,
  ...operationSchemas,
};
export { examples };
export type SchemaName = keyof typeof schemas;

const listableSchemas = [
  "memory_record", "memory_recall",
  "attend", "query", "wiki_sync",
  "workspace_status", "export_harness", "session_capture",
  "plan", "task", "work", "lock_acquire", "lock_wait", "lock_prune", "lock_release", "verify", "verify_audit",
  "forget_memory", "memory_lifecycle", "refinement", "refine_query", "refine_delete",
  "agent_registry", "agent_signal", "signal_prune",
  "mine_weakness", "developer_review", "doc_staleness", "docs_catalog", "digest", "reflect",
];
const commandIndex = [
  { command: "attend", schema: "attend", use: "Build one bounded lobby with actions, relevant evidence/gaps, and a next command.", example: 'octocode-awareness attend --query "current task" --workspace "$PWD" --compact' },
  { command: "workspace status", schema: "workspace_status", use: "Check DB health, locks, pending verification, memory counts.", example: 'octocode-awareness workspace status --workspace "$PWD" --compact' },
  { command: "plan create", schema: "plan", use: "Create a shared plan and its managed narrative document folder.", example: 'octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact' },
  { command: "plan list", schema: "plan", use: "List plans in the current workspace scope.", example: 'octocode-awareness plan list --workspace "$PWD" --compact' },
  { command: "plan show", schema: "plan", use: "Inspect one plan, its docs, and participating agents.", example: "octocode-awareness plan show --plan-id plan_123 --compact" },
  { command: "plan join", schema: "plan", use: "Join an agent to a shared plan.", example: "octocode-awareness plan join --plan-id plan_123 --agent-id agent --compact" },
  { command: "plan doc", schema: "plan", use: "Register a supporting document inside the managed plan folder.", example: "octocode-awareness plan doc --plan-id plan_123 --agent-id agent --path docs/DESIGN.md --title Design --compact" },
  { command: "plan status", schema: "plan", use: "Let the lead transition the plan lifecycle.", example: "octocode-awareness plan status --plan-id plan_123 --agent-id lead --status ACTIVE --compact" },
  { command: "task create", schema: "task", use: "Create dependency-aware plan work with reasoning and paths.", example: 'octocode-awareness task create --plan-id plan_123 --title "Schema" --reasoning "Consumers need it first" --acceptance "schema tests pass" --path src/db.ts --agent-id lead --compact' },
  { command: "task list", schema: "task", use: "List durable tasks and current claim state.", example: "octocode-awareness task list --plan-id plan_123 --compact" },
  { command: "task ready", schema: "task", use: "List unblocked, unclaimed tasks agents may choose.", example: "octocode-awareness task ready --plan-id plan_123 --compact" },
  { command: "task show", schema: "task", use: "Inspect task reasoning, paths, dependencies, and claim.", example: "octocode-awareness task show --task-id task_123 --compact" },
  { command: "task claim", schema: "task", use: "Atomically claim one task and create its execution run.", example: "octocode-awareness task claim --task-id task_123 --agent-id agent --compact" },
  { command: "task heartbeat", schema: "task", use: "Extend an active task claim lease.", example: "octocode-awareness task heartbeat --task-id task_123 --run-id run_123 --agent-id agent --compact" },
  { command: "task submit", schema: "task", use: "Submit claimed work to the verification lane.", example: 'octocode-awareness task submit --task-id task_123 --run-id run_123 --agent-id agent --message "ready for verification" --compact' },
  { command: "task release", schema: "task", use: "Release or block claimed work without declaring success.", example: "octocode-awareness task release --task-id task_123 --run-id run_123 --agent-id agent --compact" },
  { command: "task depend", schema: "task", use: "Add dependency edges within one plan.", example: "octocode-awareness task depend --task-id task_2 --depends-on task_1 --agent-id lead --compact" },
  { command: "work start", schema: "work", use: "Declare advisory file work; add --exclusive only for sensitive changes.", example: 'octocode-awareness work start --agent-id agent --workspace "$PWD" --file src/a.ts --rationale "edit parser" --test-plan "yarn test" --compact' },
  { command: "work touch", schema: "work", use: "Heartbeat active file presence without repeating its reasoning.", example: "octocode-awareness work touch --agent-id agent --run-id run_123 --compact" },
  { command: "work end", schema: "work", use: "End standalone WORK presence and move its run to verification.", example: "octocode-awareness work end --agent-id agent --run-id run_123 --compact" },
  { command: "work list", schema: "work", use: "List active file presence in the workspace.", example: 'octocode-awareness work list --workspace "$PWD" --compact' },
  { command: "work show", schema: "work", use: "Show all active agents and reasons for one file.", example: 'octocode-awareness work show --workspace "$PWD" --file src/a.ts --compact' },
  { command: "memory recall", schema: "memory_recall", use: "Recall repo lessons before planning or editing.", example: 'octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact' },
  { command: "memory record", schema: "memory_record", use: "Store durable lessons, decisions, gotchas, or observations.", example: 'octocode-awareness memory record --agent-id agent --task-context "task" --observation "lesson" --importance 7 --workspace "$PWD" --compact' },
  { command: "memory forget", schema: "forget_memory", use: "Delete selected stale memories; dry-run first.", example: "octocode-awareness memory forget --memory-id mem_123 --dry-run --compact" },
  { command: "memory archive", schema: "memory_lifecycle", use: "Preview or reversibly archive explicit active memories.", example: "octocode-awareness memory archive --memory-id mem_123 --dry-run --compact" },
  { command: "memory restore", schema: "memory_lifecycle", use: "Preview or restore explicitly archived memories; never revive replacement history.", example: "octocode-awareness memory restore --memory-id mem_123 --dry-run --compact" },
  { command: "refinement get", schema: "refine_query", use: "Read unfinished handoffs or follow-up work.", example: 'octocode-awareness refinement get --workspace "$PWD" --state open --limit 3 --compact' },
  { command: "refinement set", schema: "refinement", use: "Save handoff/work state for the next agent.", example: 'octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact' },
  { command: "refinement delete", schema: "refine_delete", use: "Delete stale refinement rows; dry-run first.", example: "octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact" },
  { command: "lock acquire", schema: "lock_acquire", use: "Acquire exclusive sensitive-file protection; exit 2 means conflict.", example: 'octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "sensitive edit" --test-plan "yarn test" --compact' },
  { command: "lock wait", schema: "lock_wait", use: "Wait for existing file locks without claiming.", example: "octocode-awareness lock wait --agent-id agent --target-file src/file.ts --wait-seconds 60 --compact" },
  { command: "lock release", schema: "lock_release", use: "Release exclusive protection to PENDING; verify success separately with evidence.", example: "octocode-awareness lock release --agent-id agent --run-id run_123 --status PENDING --compact" },
  { command: "lock prune", schema: "lock_prune", use: "Clean expired/stale locks; never marks success.", example: 'octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact' },
  { command: "verify audit", schema: "verify_audit", use: "Find pending or stale work before finishing.", example: 'octocode-awareness verify audit --agent-id agent --workspace "$PWD" --compact' },
  { command: "verify mark", schema: "verify", use: "Mark declared verification as run.", example: 'octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact' },
  { command: "signal list", schema: "agent_signal", use: "Read inbox/messages; add --mark-read only after acting. --format hook returns host briefing shape.", example: 'octocode-awareness signal list --agent-id agent --workspace "$PWD" --limit 3 --compact' },
  { command: "signal publish", schema: "agent_signal", use: "Send blocker/question/request/handoff/decision/fyi.", example: 'octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --workspace "$PWD" --compact' },
  { command: "signal reply", schema: "agent_signal", use: "Reply in an existing signal thread.", example: "octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject \"Re: File locked\" --body \"done\" --compact" },
  { command: "signal ack", schema: "agent_signal", use: "Mark specific signals read after handling.", example: "octocode-awareness signal ack --agent-id agent --signal-id ntf_123 --compact" },
  { command: "signal resolve", schema: "agent_signal", use: "Close handled signals or threads.", example: "octocode-awareness signal resolve --agent-id agent --thread-id ntf_123 --compact" },
  { command: "signal prune", schema: "signal_prune", use: "Preview/delete old resolved participant-owned signals.", example: 'octocode-awareness signal prune --agent-id agent --workspace "$PWD" --resolved --older-than-days 7 --dry-run --compact' },
  { command: "agent register", schema: "agent_registry", use: "Register/touch an agent identity.", example: 'octocode-awareness agent register --agent-id agent --agent-name "Codex" --workspace "$PWD" --compact' },
  { command: "agent list", schema: "agent_registry", use: "List known agents in scope.", example: 'octocode-awareness agent list --workspace "$PWD" --limit 5 --compact' },
  { command: "query", schema: "query", use: "Read DB views as json/table/csv/markdown/html, including file-reference health.", example: 'octocode-awareness query workboard --workspace "$PWD" --format json --limit 1 --compact' },
  { command: "query files", schema: "query", use: "Filter/sort tracked paths and stale file references; rows include file_exists and missing_file.", example: 'octocode-awareness query files --workspace "$PWD" --format table --limit 50' },
  { command: "query workboard", schema: "query", use: "Read the smart agent queue, including stale_file_refs memory-review items.", example: 'octocode-awareness query workboard --workspace "$PWD" --format json --limit 1 --compact' },
  { command: "query all", schema: "query", use: "Export all live views; use html for the sortable/filterable browser view.", example: 'octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html' },
  { command: "query developer-review", schema: "query", use: "Read instruction-feedback rows; request Markdown only for an explicit export.", example: 'octocode-awareness query developer-review --workspace "$PWD" --format markdown --compact' },
  { command: "wiki sync", schema: "wiki_sync", use: "Refresh the local .octocode wiki/projections from canonical SQLite state.", example: 'octocode-awareness wiki sync --workspace "$PWD" --mode local --compact' },
  { command: "session capture", schema: "session_capture", use: "Hook-driven handoff capture from locks + dirty git tree.", example: 'octocode-awareness session capture --agent-id agent --workspace "$PWD" --reason handoff --compact' },
  { command: "reflect record", schema: "reflect", use: "Record outcome and lessons after work.", example: 'octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "lesson" --compact' },
  { command: "reflect mine-weakness", schema: "mine_weakness", use: "Find recurring failure clusters.", example: 'octocode-awareness reflect mine-weakness --workspace "$PWD" --compact' },
  { command: "reflect export-harness", schema: "export_harness", use: "Preview harness guidance candidates from memories.", example: 'octocode-awareness reflect export-harness --workspace "$PWD" --compact' },
  { command: "reflect developer-review", schema: "developer_review", use: "Read agent feedback on the instructions themselves (from reflect record --fix-instructions).", example: 'octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact' },
  { command: "docs list", schema: "docs_catalog", use: "List skill reference docs (references/*.md).", example: "octocode-awareness docs list --compact" },
  { command: "docs show", schema: "docs_catalog", use: "Show one skill reference by name.", example: "octocode-awareness docs show architecture" },
  { command: "docs staleness", schema: "doc_staleness", use: "Find docs likely stale from edit activity.", example: 'octocode-awareness docs staleness --targets-json \'[{"docFile":"README.md","sourceDirs":["src"]}]\' --compact' },
  { command: "maintenance digest", schema: "digest", use: "Preview or run memory, expired-lock, terminal-refinement, and terminal-run cleanup; signal/reference pressure is report-only.", example: 'octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact' },
  { command: "maintenance init", schema: null, use: "Initialize the awareness DB.", example: "octocode-awareness maintenance init --compact" },
  { command: "maintenance self-test", schema: null, use: "Run in-memory DB smoke checks.", example: "octocode-awareness maintenance self-test --compact" },
  { command: "hooks install", schema: null, use: "Install hook config after preview/approval; use non-compact preview for settings detail.", example: "octocode-awareness hooks install --host codex --dry-run" },
  { command: "hooks check", schema: null, use: "Check installed hook config and detect drift; use non-compact output for runtime detail.", example: "octocode-awareness hooks check --host codex --strict" },
  { command: "hooks remove", schema: null, use: "Remove awareness-owned hook config after a detailed preview.", example: "octocode-awareness hooks remove --host codex --dry-run" },
  { command: "hook run", schema: null, use: "Internal hook dispatcher used by wrappers.", example: "octocode-awareness hook run pre-edit < hook-payload.json" },
  { command: "schema commands", schema: null, use: "Print this command-to-schema map.", example: "octocode-awareness schema commands --compact" },
  { command: "schema list", schema: null, use: "Print schema names only.", example: "octocode-awareness schema list --compact" },
  { command: "schema path", schema: null, use: "Expose one standalone JSON Schema file to an agent.", example: "octocode-awareness schema path memory_recall --compact" },
  { command: "schema json-schema", schema: null, use: "Print one JSON schema.", example: "octocode-awareness schema json-schema memory_recall --compact" },
  { command: "schema example", schema: null, use: "Print example JSON for one schema.", example: "octocode-awareness schema example memory_recall --compact" },
  { command: "schema validate", schema: null, use: "Validate JSON payload against one schema.", example: "octocode-awareness schema validate memory_recall payload.json --compact" },
];

const CORE_NOUNS = new Set(["attend", "plan", "task", "work", "verify", "memory", "signal", "wiki", "query"]);
const CLI_REQUIRED: Record<string, string[]> = {
  "plan create": ["name", "objective", "lead_agent_id", "workspace"],
  "plan show": ["plan_id"],
  "plan join": ["plan_id", "agent_id"],
  "plan doc": ["plan_id", "agent_id", "path", "title"],
  "plan status": ["plan_id", "agent_id", "status"],
  "task create": ["plan_id", "title", "reasoning", "acceptance", "path", "agent_id"],
  "task show": ["task_id"],
  "task claim": ["agent_id"],
  "task heartbeat": ["task_id", "run_id", "agent_id"],
  "task submit": ["task_id", "run_id", "agent_id"],
  "task release": ["task_id", "run_id", "agent_id"],
  "task depend": ["task_id", "depends_on", "agent_id"],
  "work start": ["agent_id", "file"],
  "work touch": ["agent_id", "run_id"],
  "work end": ["agent_id", "run_id"],
  "work show": ["workspace", "file"],
  "memory record": ["agent_id", "task_context", "observation", "importance"],
  "signal publish": ["agent_id", "kind", "subject"],
  "signal reply": ["agent_id", "in_reply_to", "subject"],
  "signal ack": ["agent_id", "signal_id"],
  "signal resolve": ["agent_id"],
};
const CLI_ALLOWED: Record<string, string[]> = {
  "plan create": ["name", "objective", "lead_agent_id", "workspace", "artifact"],
  "plan list": ["workspace", "artifact", "status", "limit", "full"],
  "plan show": ["plan_id", "full"],
  "plan join": ["plan_id", "agent_id"],
  "plan doc": ["plan_id", "agent_id", "path", "title"],
  "plan status": ["plan_id", "agent_id", "status"],
  "task create": ["plan_id", "title", "reasoning", "acceptance", "path", "depends_on", "agent_id", "priority", "lease_minutes", "test_plan"],
  "task list": ["plan_id", "workspace", "status", "limit", "full"],
  "task ready": ["plan_id", "workspace", "limit", "full"],
  "task show": ["task_id", "full"],
  "task claim": ["task_id", "plan_id", "agent_id", "next", "lease_minutes"],
  "task heartbeat": ["task_id", "run_id", "agent_id", "lease_minutes"],
  "task submit": ["task_id", "run_id", "agent_id", "message"],
  "task release": ["task_id", "run_id", "agent_id", "blocked_reason"],
  "task depend": ["task_id", "depends_on", "agent_id"],
  "work start": ["agent_id", "session_id", "workspace", "artifact", "run_id", "rationale", "test_plan", "context_ref", "file", "exclusive", "ttl_minutes", "ttl_seconds"],
  "work touch": ["agent_id", "run_id", "file", "ttl_minutes", "ttl_seconds"],
  "work end": ["agent_id", "run_id", "file"],
  "work list": ["agent_id", "workspace", "artifact", "run_id", "all", "full"],
  "work show": ["workspace", "artifact", "file", "all", "full"],
  "signal publish": ["agent_id", "workspace", "artifact", "repo", "ref", "kind", "subject", "body", "to_agent", "file", "ref_id", "importance"],
  "signal list": ["agent_id", "workspace", "artifact", "repo", "ref", "all", "unread_only", "mark_read", "limit", "include_bodies", "format"],
  "signal reply": ["agent_id", "in_reply_to", "subject", "body", "to_agent", "file", "ref_id", "importance"],
  "signal ack": ["agent_id", "signal_id"],
  "signal resolve": ["agent_id", "signal_id", "thread_id"],
};

function groupedCommandIndex() {
  const grouped: Record<"core" | "advanced", Record<string, string[]>> = { core: {}, advanced: {} };
  for (const row of commandIndex) {
    const [noun, ...rest] = row.command.split(" ");
    const tier = CORE_NOUNS.has(noun!) ? "core" : "advanced";
    (grouped[tier][noun!] ??= []).push(rest.length > 0 ? rest.join(" ") : noun === "query" ? "<view>" : "run");
  }
  return grouped;
}

function printJson(payload: unknown, compact = false): void {
  console.log(JSON.stringify(payload, null, compact ? 0 : 2));
}

function usage() {
  return `Usage:
  octocode-awareness schema commands [--compact] [--all] [--examples]
  octocode-awareness schema command <noun> [action] [--compact]
  octocode-awareness schema list
  octocode-awareness schema path <schema-name>
  octocode-awareness schema json-schema <schema-name>
  octocode-awareness schema example <schema-name>
  octocode-awareness schema validate <schema-name> <json-file|->`;
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
  const aliases: Record<string, string> = {
    workspace_path: "workspace",
    target_files: "file",
    tags: "tag",
    references: "reference",
  };
  if (properties) {
    for (const [from, to] of Object.entries(aliases)) {
      if (properties[from] && !properties[to]) properties[to] = properties[from];
      delete properties[from];
    }
    const allowed = CLI_ALLOWED[commandName];
    if (allowed) {
      for (const property of Object.keys(properties)) {
        if (!allowed.includes(property)) delete properties[property];
      }
    }
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

function schemaFilePath(schemaName: SchemaName): string {
  const argvDir = process.argv[1] ? dirname(resolve(process.argv[1])) : process.cwd();
  const candidates = [
    process.env.OCTOCODE_AWARENESS_SCHEMA_DIR,
    resolve(argvDir, 'schemas'),
  ].filter((candidate): candidate is string => Boolean(candidate));
  return resolve(candidates.find((candidate) => existsSync(resolve(candidate, `${schemaName}.schema.json`))) ?? candidates[0]!, `${schemaName}.schema.json`);
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
        : "Core first; advanced remains available. Pass --all for the flat catalog.",
      commands,
    }, compact);
    return 0;
  }

  if (command === "command") {
    const commandName = [schemaName, file].filter(Boolean).join(" ");
    const commandSchema = cliCommandSchema(commandName);
    if (!commandSchema) {
      return printJsonError({
        error_code: "UNKNOWN_CLI_COMMAND",
        error: `Unknown or schema-less CLI command: ${commandName || "<missing>"}`,
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

  if (command === "path") {
    printJson({ ok: true, schema: knownSchemaName, path: schemaFilePath(knownSchemaName!) }, compact);
    return 0;
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
