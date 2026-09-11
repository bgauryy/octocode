import type { ApprovalClass } from '@octocodeai/agent-contracts/protocols';
import { schemas, type SchemaName } from './registry.js';
import { projectCommandInput } from './command-input.js';
import { commandSchemaProperties } from './command-properties.js';
import { HISTORY_ROUTE_DESCRIPTORS } from './definitions-history.js';

export type AwarenessCommandEffect =
  | 'read'
  | 'coordination-write'
  | 'workspace-write'
  | 'host-config-write'
  | 'destructive-admin';
export type AwarenessCommandPiMode = 'normal' | 'recovery' | 'external-host-only';
export type AwarenessInjectedField = 'database' | 'workspace' | 'agent-id' | 'compact';

export interface AwarenessCommandCatalogEntry {
  command: string;
  schema: string;
  use: string;
  example: string;
  effect: AwarenessCommandEffect;
  piMode: AwarenessCommandPiMode;
  approvalClass?: ApprovalClass;
  injected: readonly AwarenessInjectedField[];
  /** Process exit codes that carry a valid report rather than command failure. */
  resultExitCodes?: readonly number[];
  positionals?: readonly string[];
  stdinField?: string;
}

const READ_COMMANDS = new Set([
  'attend', 'status', 'plan list', 'plan show', 'task list', 'task ready', 'task show',
  'work list', 'work show', 'memory recall', 'refinement get', 'lock wait', 'verify audit',
  'signal list', 'agent list', 'query', 'query files', 'query workboard', 'query all',
  'query developer-review', 'reflect mine-weakness', 'reflect developer-review', 'docs list',
  'docs show', 'docs staleness', 'maintenance self-test', 'config show', 'config validate',
  'hooks check', 'schema commands', 'schema command', 'schema entities', 'schema list',
  'schema json-schema', 'schema example', 'schema validate', 'memory recall-verified',
  'memory evaluate', 'handoff list', 'guide', 'instructions export', 'history status',
  'history timeline', 'history read', 'history inspect', 'history restore-preview', 'history retention-preview',
  'skill list', 'skill check',
]);
const COORDINATION_WRITE_COMMANDS = new Set([
  'plan create', 'plan join', 'plan doc', 'plan status', 'task create', 'task claim',
  'task heartbeat', 'task submit', 'task release', 'task retry', 'task depend', 'work start',
  'work touch', 'work end', 'memory record', 'memory restore', 'refinement set', 'lock acquire',
  'lock release', 'verify mark', 'signal publish', 'signal reply', 'signal ack', 'signal resolve',
  'agent register', 'session capture', 'reflect record', 'maintenance init', 'hook run',
  'agent touch', 'agent leave', 'memory store-verified', 'handoff add', 'handoff clear',
  'hooks pre-edit', 'history capture', 'history checkpoint',
]);
const WORKSPACE_WRITE_COMMANDS = new Set(['reflect export-harness', 'history restore-apply']);
const HOST_CONFIG_WRITE_COMMANDS = new Set(['skill install', 'skill remove', 'config init', 'hooks install', 'hooks remove']);
const DESTRUCTIVE_ADMIN_COMMANDS = new Set([
  'memory forget', 'memory archive', 'memory reindex', 'memory prune', 'refinement delete',
  'lock prune', 'signal prune', 'maintenance digest', 'database consolidate',
  'history retention-prune', 'history recovery', 'history evidence',
]);
const EXTERNAL_HOST_ONLY = new Set([
  // Lifecycle callbacks accept host event payloads; the host invokes them through
  // the API. Setup and instruction export are ordinary native commands.
  'hook run', 'hooks pre-edit',
]);
const RECOVERY_COMMANDS = new Set([
  'plan create', 'plan list', 'plan show', 'plan join', 'plan doc', 'plan status',
  'task create', 'task list', 'task ready', 'task show', 'task claim', 'task heartbeat',
  'task submit', 'task release', 'task retry', 'task depend', 'work start', 'work touch',
  'work end', 'work list', 'work show', 'verify audit', 'verify mark', 'memory forget',
  'memory archive', 'memory restore', 'memory reindex', 'memory prune', 'refinement delete',
  'lock prune', 'signal prune', 'maintenance digest', 'maintenance init', 'database consolidate',
  'history status', 'history capture', 'history checkpoint', 'history timeline', 'history read',
  'history restore-preview', 'history restore-apply', 'history retention-preview',
  'history retention-prune', 'history recovery', 'history evidence',
]);
const NO_DATABASE_INJECTION = new Set([
  'docs list', 'docs show', 'docs staleness', 'skill install', 'skill list', 'skill check',
  'skill remove', 'maintenance self-test',
  'config show', 'config init', 'config validate', 'hooks install', 'hooks check',
  'hooks remove', 'hook run', 'schema commands', 'schema command', 'schema entities',
  'schema list', 'schema json-schema', 'schema example', 'schema validate', 'guide',
  'instructions export', 'database consolidate',
]);
const POSITIONALS: Readonly<Record<string, readonly string[]>> = Object.freeze({
  'docs show': ['name'],
  'hook run': ['event'],
  'schema command': ['noun', 'subcommand'],
  'schema json-schema': ['schema_name'],
  'schema example': ['schema_name'],
  'schema validate': ['schema_name', 'input'],
});

function injectedFor(command: string, schemaName: string): readonly AwarenessInjectedField[] {
  const flags = new Set(Object.keys(commandSchemaProperties(projectCommandInput(command, schemas[schemaName as SchemaName]))));
  const fields: AwarenessInjectedField[] = [];
  if (!NO_DATABASE_INJECTION.has(command)) fields.push('database');
  if (flags.has('workspace')) fields.push('workspace');
  // Work reads use agent_id as an optional selector, not the acting identity.
  if ((flags.has('agent_id') || flags.has('lead_agent_id')) && command !== 'work list' && command !== 'work show') fields.push('agent-id');
  fields.push('compact');
  return Object.freeze(fields);
}

function effectFor(command: string): AwarenessCommandEffect {
  if (READ_COMMANDS.has(command)) return 'read';
  if (COORDINATION_WRITE_COMMANDS.has(command)) return 'coordination-write';
  if (WORKSPACE_WRITE_COMMANDS.has(command)) return 'workspace-write';
  if (HOST_CONFIG_WRITE_COMMANDS.has(command)) return 'host-config-write';
  if (DESTRUCTIVE_ADMIN_COMMANDS.has(command)) return 'destructive-admin';
  throw new Error(`Awareness command is missing an explicit effect classification: ${command}`);
}

function approvalFor(command: string, effect: AwarenessCommandEffect): ApprovalClass | undefined {
  if (command === 'skill install' || command === 'hooks install') return 'install';
  if (command === 'skill remove') return 'fs-delete';
  if (effect === 'host-config-write') return 'system';
  if (effect === 'workspace-write') return 'fs-delete';
  if (effect === 'destructive-admin') return 'infra';
  return undefined;
}

const rawCommandIndex = [
  { command: "attend", schema: "attend", use: "Read registered workspace peers once; use --details or query/file filters for deeper coordination.", example: 'npx @octocodeai/octocode-awareness attend --workspace "$PWD" --compact' },
  { command: "status", schema: "workspace_status", use: "Check DB health, locks, pending verification, memory counts.", example: 'npx @octocodeai/octocode-awareness status --workspace "$PWD" --compact' },
  { command: "plan create", schema: "plan", use: "Create a shared plan and its managed narrative document folder.", example: 'npx @octocodeai/octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact' },
  { command: "plan list", schema: "plan", use: "List plans in the current workspace scope.", example: 'npx @octocodeai/octocode-awareness plan list --workspace "$PWD" --compact' },
  { command: "plan show", schema: "plan", use: "Inspect one plan, its docs, and participating agents.", example: "npx @octocodeai/octocode-awareness plan show --plan-id plan_123 --compact" },
  { command: "plan join", schema: "plan", use: "Join an agent to a shared plan.", example: "npx @octocodeai/octocode-awareness plan join --plan-id plan_123 --agent-id agent --compact" },
  { command: "plan doc", schema: "plan", use: "Register a supporting document inside the managed plan folder.", example: "npx @octocodeai/octocode-awareness plan doc --plan-id plan_123 --agent-id agent --path docs/DESIGN.md --title Design --compact" },
  { command: "plan status", schema: "plan", use: "Let the lead transition the plan lifecycle.", example: "npx @octocodeai/octocode-awareness plan status --plan-id plan_123 --agent-id lead --status ACTIVE --compact" },
  { command: "task create", schema: "task", use: "Create dependency-aware plan work with reasoning and paths.", example: 'npx @octocodeai/octocode-awareness task create --plan-id plan_123 --title "Schema" --reasoning "Consumers need it first" --acceptance "schema tests pass" --path src/db.ts --agent-id lead --compact' },
  { command: "task list", schema: "task", use: "List durable tasks and current claim state.", example: "npx @octocodeai/octocode-awareness task list --plan-id plan_123 --compact" },
  { command: "task ready", schema: "task", use: "List unblocked, unclaimed tasks agents may choose.", example: "npx @octocodeai/octocode-awareness task ready --plan-id plan_123 --compact" },
  { command: "task show", schema: "task", use: "Inspect task reasoning, paths, dependencies, and claim.", example: "npx @octocodeai/octocode-awareness task show --task-id task_123 --compact" },
  { command: "task claim", schema: "task", use: "Atomically claim one task and create its execution run.", example: "npx @octocodeai/octocode-awareness task claim --task-id task_123 --agent-id agent --compact" },
  { command: "task heartbeat", schema: "task", use: "Extend an active task claim lease.", example: "npx @octocodeai/octocode-awareness task heartbeat --task-id task_123 --run-id run_123 --agent-id agent --compact" },
  { command: "task submit", schema: "task", use: "Submit claimed work to the verification lane.", example: 'npx @octocodeai/octocode-awareness task submit --task-id task_123 --run-id run_123 --agent-id agent --message "ready for verification" --compact' },
  { command: "task release", schema: "task", use: "Release or block claimed work without declaring success.", example: "npx @octocodeai/octocode-awareness task release --task-id task_123 --run-id run_123 --agent-id agent --compact" },
  { command: "task retry", schema: "task", use: "Lead explicitly reopens a FAILED or BLOCKED task; dependencies still gate claims.", example: "npx @octocodeai/octocode-awareness task retry --task-id task_123 --agent-id lead --compact" },
  { command: "task depend", schema: "task", use: "Add dependency edges within one plan.", example: "npx @octocodeai/octocode-awareness task depend --task-id task_2 --depends-on task_1 --agent-id lead --compact" },
  { command: "work start", schema: "work", use: "Declare advisory file work; add --exclusive only for sensitive changes.", example: 'npx @octocodeai/octocode-awareness work start --agent-id agent --workspace "$PWD" --file src/a.ts --rationale "edit parser" --test-plan "yarn test" --compact' },
  { command: "work touch", schema: "work", use: "Refresh already-declared active run files; use work start --run-id to add files.", example: "npx @octocodeai/octocode-awareness work touch --agent-id agent --run-id run_123 --compact" },
  { command: "work end", schema: "work", use: "End standalone WORK presence and move its run to verification.", example: "npx @octocodeai/octocode-awareness work end --agent-id agent --run-id run_123 --compact" },
  { command: "work list", schema: "work", use: "List active file presence in the workspace.", example: 'npx @octocodeai/octocode-awareness work list --workspace "$PWD" --compact' },
  { command: "work show", schema: "work", use: "Show all active agents and reasons for one file.", example: 'npx @octocodeai/octocode-awareness work show --workspace "$PWD" --file src/a.ts --compact' },
  { command: "memory recall", schema: "memory_recall", use: "Recall scoped lessons when prior learning could change the next action.", example: 'npx @octocodeai/octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact' },
  { command: "memory record", schema: "memory_record", use: "Save verified reusable learning after substantial work or a meaningful event.", example: 'npx @octocodeai/octocode-awareness memory record --agent-id agent --task-context "task" --observation "lesson" --importance 7 --workspace "$PWD" --compact' },
  { command: "memory forget", schema: "forget_memory", use: "Delete selected stale memories; dry-run first.", example: "npx @octocodeai/octocode-awareness memory forget --memory-id mem_123 --dry-run --compact" },
  { command: "memory archive", schema: "memory_lifecycle", use: "Preview or reversibly archive explicit active memories.", example: "npx @octocodeai/octocode-awareness memory archive --memory-id mem_123 --dry-run --compact" },
  { command: "memory restore", schema: "memory_lifecycle", use: "Preview or restore explicitly archived memories; never revive replacement history.", example: "npx @octocodeai/octocode-awareness memory restore --memory-id mem_123 --dry-run --compact" },
  { command: "refinement get", schema: "refine_query", use: "Read unfinished handoffs or follow-up work.", example: 'npx @octocodeai/octocode-awareness refinement get --workspace "$PWD" --state open --limit 3 --compact' },
  { command: "refinement set", schema: "refinement", use: "Save handoff/work state for the next agent.", example: 'npx @octocodeai/octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact' },
  { command: "refinement delete", schema: "refine_delete", use: "Delete stale refinement rows; dry-run first.", example: "npx @octocodeai/octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact" },
  { command: "lock acquire", schema: "lock_acquire", use: "Acquire exclusive sensitive-file protection; exit 2 means conflict.", example: 'npx @octocodeai/octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "sensitive edit" --test-plan "yarn test" --compact' },
  { command: "lock wait", schema: "lock_wait", use: "Wait for existing file locks without claiming.", example: "npx @octocodeai/octocode-awareness lock wait --agent-id agent --target-file src/file.ts --wait-seconds 60 --compact" },
  { command: "lock release", schema: "lock_release", use: "Release exclusive protection to PENDING; verify success separately with evidence.", example: "npx @octocodeai/octocode-awareness lock release --agent-id agent --run-id run_123 --status PENDING --compact" },
  { command: "lock prune", schema: "lock_prune", use: "Clean expired/stale locks; never marks success.", example: 'npx @octocodeai/octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact' },
  { command: "verify audit", schema: "verify_audit", use: "Audit pending or stale tracked work; unnecessary for untracked solo tasks.", example: 'npx @octocodeai/octocode-awareness verify audit --agent-id agent --workspace "$PWD" --compact' },
  { command: "verify mark", schema: "verify", use: "Record observed SUCCESS or FAILED for selected runs; unrun checks stay pending.", example: 'npx @octocodeai/octocode-awareness verify mark --agent-id agent --run-id run_123 --status SUCCESS --message "yarn test passed" --workspace "$PWD" --compact' },
  { command: "signal list", schema: "agent_signal", use: "Read inbox/messages; add --mark-read only after acting. --format hook returns host briefing shape.", example: 'npx @octocodeai/octocode-awareness signal list --agent-id agent --workspace "$PWD" --limit 3 --compact' },
  { command: "signal publish", schema: "agent_signal", use: "Send blocker/question/request/handoff/decision/fyi.", example: 'npx @octocodeai/octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --workspace "$PWD" --compact' },
  { command: "signal reply", schema: "agent_signal", use: "Reply in an existing signal thread.", example: "npx @octocodeai/octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject \"Re: File locked\" --body \"done\" --compact" },
  { command: "signal ack", schema: "agent_signal", use: "Mark specific signals read after handling.", example: "npx @octocodeai/octocode-awareness signal ack --agent-id agent --signal-id ntf_123 --compact" },
  { command: "signal resolve", schema: "agent_signal", use: "Close handled signals or threads.", example: "npx @octocodeai/octocode-awareness signal resolve --agent-id agent --thread-id ntf_123 --compact" },
  { command: "signal prune", schema: "signal_prune", use: "Preview/delete old resolved participant-owned signals.", example: 'npx @octocodeai/octocode-awareness signal prune --agent-id agent --workspace "$PWD" --resolved --older-than-days 7 --dry-run --compact' },
  { command: "agent register", schema: "agent_registry", use: "Register/touch a unique stable agent ID with optional self-reported name, model vendor and host application; reuse host identity or set OCTOCODE_AGENT_ID once.", example: 'npx @octocodeai/octocode-awareness agent register --agent-id "$OCTOCODE_AGENT_ID" --agent-name "Parser reviewer" --agent-vendor "openai" --agent-host "codex" --workspace "$PWD" --compact' },
  { command: "agent list", schema: "agent_registry", use: "Discover scoped peer IDs, names, vendors and hosts; follow executable continuations and route signals by exact agent_id, not labels.", example: 'npx @octocodeai/octocode-awareness agent list --workspace "$PWD" --limit 5 --compact' },
  { command: "query", schema: "query", use: "Read DB views as json/table/csv/markdown/html, including file-reference health.", example: 'npx @octocodeai/octocode-awareness query workboard --workspace "$PWD" --format json --limit 1 --compact' },
  { command: "query files", schema: "query", use: "Filter/sort tracked paths and stale file references; rows include file_exists and missing_file.", example: 'npx @octocodeai/octocode-awareness query files --workspace "$PWD" --format table --limit 50' },
  { command: "query workboard", schema: "query", use: "Read the smart agent queue, including stale_file_refs memory-review items.", example: 'npx @octocodeai/octocode-awareness query workboard --workspace "$PWD" --format json --limit 1 --compact' },
  { command: "query all", schema: "query", use: "Export all live views; use html for the sortable/filterable browser view.", example: 'npx @octocodeai/octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html' },
  { command: "query developer-review", schema: "query", use: "Read instruction-feedback rows; request Markdown only for an explicit export.", example: 'npx @octocodeai/octocode-awareness query developer-review --workspace "$PWD" --format markdown --compact' },
  { command: "session capture", schema: "session_capture", use: "Hook-driven handoff capture from awareness_locks + dirty git tree.", example: 'npx @octocodeai/octocode-awareness session capture --agent-id agent --workspace "$PWD" --reason handoff --compact' },
  { command: "reflect record", schema: "reflect", use: "Save one verified reusable lesson after substantial work or a meaningful event.", example: 'npx @octocodeai/octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "lesson" --compact' },
  { command: "reflect mine-weakness", schema: "mine_weakness", use: "Find recurring failure clusters.", example: 'npx @octocodeai/octocode-awareness reflect mine-weakness --workspace "$PWD" --compact' },
  { command: "reflect export-harness", schema: "export_harness", use: "Preview harness guidance candidates from awareness_memories.", example: 'npx @octocodeai/octocode-awareness reflect export-harness --workspace "$PWD" --compact' },
  { command: "reflect developer-review", schema: "developer_review", use: "Read agent feedback on the instructions themselves (from reflect record --fix-instructions).", example: 'npx @octocodeai/octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact' },
  { command: "docs list", schema: "docs_catalog", use: "List skill reference docs (references/*.md).", example: "npx @octocodeai/octocode-awareness docs list --compact" },
  { command: "docs show", schema: "docs_catalog", use: "Show one skill reference by name.", example: "npx @octocodeai/octocode-awareness docs show architecture" },
  { command: "docs staleness", schema: "doc_staleness", use: "Find docs likely stale from edit activity.", example: 'npx @octocodeai/octocode-awareness docs staleness --targets-json \'[{"docFile":"README.md","sourceDirs":["src"]}]\' --compact' },
  { command: "skill install", schema: "skill_install", use: "Preview or link the bundled octocode-awareness skill into an explicit agent platform and scope.", example: 'npx @octocodeai/octocode-awareness skill install --platform shared --project-dir "$PWD" --dry-run' },
  { command: "skill list", schema: "skill_list", use: "List the bundled Awareness skill and its canonical install state.", example: "npx @octocodeai/octocode-awareness skill list --compact" },
  { command: "skill check", schema: "skill_check", use: "Check the canonical Awareness skill and optional explicit platform destination.", example: 'npx @octocodeai/octocode-awareness skill check --platform shared --project-dir "$PWD" --compact' },
  { command: "skill remove", schema: "skill_remove", use: "Preview or remove an explicit platform destination or canonical copy.", example: 'npx @octocodeai/octocode-awareness skill remove --platform shared --project-dir "$PWD" --compact' },
  { command: "maintenance digest", schema: "digest", use: "Preview or run memory, expired-lock, terminal-refinement, and terminal-run cleanup; signal/reference pressure is report-only.", example: 'npx @octocodeai/octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact' },
  { command: "maintenance init", schema: "maintenance_init", use: "Initialize the Awareness workflow store deterministically; safe to repeat.", example: "npx @octocodeai/octocode-awareness maintenance init --compact" },
  { command: "maintenance self-test", schema: "maintenance_self_test", use: "Run in-memory DB smoke checks.", example: "npx @octocodeai/octocode-awareness maintenance self-test --compact" },
  { command: "config show", schema: "awareness_config", use: "Inspect effective lean defaults or explicit feature settings; configuration is optional.", example: "npx @octocodeai/octocode-awareness config show --compact" },
  { command: "config init", schema: "awareness_config", use: "Create awareness.json from lean defaults plus explicit feature overrides; refuses overwrite.", example: "npx @octocodeai/octocode-awareness config init --compact" },
  { command: "config validate", schema: "awareness_config", use: "Validate the existing awareness.json and report unsupported fields.", example: "npx @octocodeai/octocode-awareness config validate --compact" },
  { command: "hooks install", schema: "hooks_install", use: "Preview the exact hook config, then install within the user-authorized host and scope. Existing authorization remains valid for that scope.", example: "npx @octocodeai/octocode-awareness hooks install --host codex --dry-run" },
  { command: "hooks check", schema: "hooks_check", use: "Check installed hook config and detect drift; use non-compact output for runtime detail.", example: "npx @octocodeai/octocode-awareness hooks check --host codex --strict" },
  { command: "hooks remove", schema: "hooks_remove", use: "Remove awareness-owned hook config after a detailed preview.", example: "npx @octocodeai/octocode-awareness hooks remove --host codex --dry-run" },
  { command: "hook run", schema: "hook_run", use: "Internal hook dispatcher used by wrappers.", example: "octocode-awareness hook run pre-edit < hook-payload.json" },
  { command: "schema commands", schema: "schema_commands", use: "Print this command-to-schema map.", example: "npx @octocodeai/octocode-awareness schema commands --compact" },
  { command: "schema command", schema: "schema_command", use: "Print exact CLI flags and requirements for one concept operation or operator route.", example: "npx @octocodeai/octocode-awareness schema command work protect --compact" },
  { command: "schema entities", schema: "schema_entities", use: "Print the read-only Awareness entity catalog from canonical DDL.", example: "npx @octocodeai/octocode-awareness schema entities --compact" },
  { command: "schema list", schema: "schema_list", use: "Print schema names only.", example: "npx @octocodeai/octocode-awareness schema list --compact" },
  { command: "schema json-schema", schema: "schema_json_schema", use: "Print one JSON schema.", example: "npx @octocodeai/octocode-awareness schema json-schema memory_recall --compact" },
  { command: "schema example", schema: "schema_example", use: "Print example JSON for one schema.", example: "npx @octocodeai/octocode-awareness schema example memory_recall --compact" },
  { command: "schema validate", schema: "schema_validate", use: "Validate JSON payload against one schema.", example: "npx @octocodeai/octocode-awareness schema validate memory_recall payload.json --compact" },
  {"command": "agent touch", "schema": "agent_presence", "use": "Refresh a registered agent presence.", "example": "npx @octocodeai/octocode-awareness agent touch --help"},
  {"command": "agent leave", "schema": "agent_presence", "use": "End agent presence.", "example": "npx @octocodeai/octocode-awareness agent leave --help"},
  {"command": "memory store-verified", "schema": "verified_memory", "use": "Store scoped evidence with source digest and expiry.", "example": "npx @octocodeai/octocode-awareness memory store-verified --help"},
  {"command": "memory recall-verified", "schema": "verified_recall", "use": "Recall evidence valid for a source and time.", "example": "npx @octocodeai/octocode-awareness memory recall-verified --help"},
  {"command": "memory evaluate", "schema": "memory_evaluate", "use": "Evaluate recall against an explicit corpus.", "example": "npx @octocodeai/octocode-awareness memory evaluate --help"},
  {"command": "memory reindex", "schema": "memory_reindex", "use": "Rebuild derived memory embeddings.", "example": "npx @octocodeai/octocode-awareness memory reindex --help"},
  {"command": "memory prune", "schema": "memory_prune", "use": "Preview or remove old memories.", "example": "npx @octocodeai/octocode-awareness memory prune --help"},
  {"command": "handoff add", "schema": "handoff_add", "use": "Record a continuity note.", "example": "npx @octocodeai/octocode-awareness handoff add --help"},
  {"command": "handoff list", "schema": "handoff_list", "use": "Read continuity notes.", "example": "npx @octocodeai/octocode-awareness handoff list --help"},
  {"command": "handoff clear", "schema": "handoff_clear", "use": "Clear a continuity note.", "example": "npx @octocodeai/octocode-awareness handoff clear --help"},
  {"command": "guide", "schema": "guide", "use": "Read the external agent workflow.", "example": "npx @octocodeai/octocode-awareness guide --help"},
  {"command": "instructions export", "schema": "instructions_export", "use": "Export host instructions.", "example": "npx @octocodeai/octocode-awareness instructions export --help"},
  {"command": "hooks pre-edit", "schema": "pre_edit", "use": "Check the edit gate.", "example": "npx @octocodeai/octocode-awareness hooks pre-edit --help"},
  {"command": "database consolidate", "schema": "database_consolidate", "use": "Convert a supported historical database into a new canonical file.", "example": "npx @octocodeai/octocode-awareness database consolidate --help"},
  ...HISTORY_ROUTE_DESCRIPTORS.map(({ required: _required, allowed: _allowed, ...route }) => route),
];

export const commandIndex: readonly AwarenessCommandCatalogEntry[] = Object.freeze(
  rawCommandIndex.map((row) => {
    const effect = effectFor(row.command);
    let injected: readonly AwarenessInjectedField[] | undefined;
    return Object.freeze({
      ...row,
      effect,
      piMode: EXTERNAL_HOST_ONLY.has(row.command)
        ? 'external-host-only' as const
        : RECOVERY_COMMANDS.has(row.command)
          ? 'recovery' as const
          : 'normal' as const,
      ...(approvalFor(row.command, effect) ? { approvalClass: approvalFor(row.command, effect) } : {}),
      get injected() { return injected ??= injectedFor(row.command, row.schema); },
      ...(row.command === 'verify audit' ? { resultExitCodes: Object.freeze([0, 1]) } : {}),
      ...(POSITIONALS[row.command] ? { positionals: POSITIONALS[row.command] } : {}),
      ...(row.command === 'hook run' ? { stdinField: 'payload' } : {}),
    });
  }),
);
