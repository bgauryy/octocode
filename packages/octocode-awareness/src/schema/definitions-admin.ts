import { z } from 'zod';

const text = z.string().min(1);
const path = text.max(4096);
const empty = z.object({});
const hookHost = z.enum(['claude', 'codex', 'copilot', 'cursor', 'gemini', 'opencode']);
const hookProfile = z.enum(['guard', 'coordination', 'full']);
const hookOptions = {
  host: hookHost,
  project_dir: path.optional(),
  global: z.boolean().optional(),
  dry_run: z.boolean().optional(),
  profile: hookProfile.optional(),
};

/** Explicit contracts for administrative and schema-meta routes. */
export const adminSchemas = {
  skill_install: z.object({
    platform: text.describe('One or more comma-separated platforms, an alias, or all.'),
    global: z.boolean().optional(),
    project_dir: path.optional(),
    mode: z.enum(['symlink', 'copy', 'auto']).optional(),
    dry_run: z.boolean().optional(),
    force: z.boolean().optional(),
  }),
  maintenance_init: empty,
  maintenance_self_test: empty,
  hooks_install: z.object(hookOptions),
  hooks_check: z.object({ ...hookOptions, strict: z.boolean().optional() }),
  hooks_remove: z.object(hookOptions),
  hook_run: z.object({
    event: z.enum(['pre-edit', 'post-edit', 'stop-verify', 'notify-deliver', 'session-compact', 'session-end']),
    payload: z.record(z.string(), z.unknown()),
  }),
  schema_commands: z.object({ all: z.boolean().optional(), examples: z.boolean().optional() }),
  schema_command: z.object({ noun: text, subcommand: text.optional() }),
  schema_entities: z.object({ all: z.boolean().optional() }),
  schema_list: empty,
  schema_json_schema: z.object({ schema_name: text }),
  schema_example: z.object({ schema_name: text }),
  schema_validate: z.object({ schema_name: text, input: z.unknown().describe('JSON value or serialized JSON to validate. The CLI accepts a JSON file path or stdin marker instead.') }),
};

export const adminExamples = {
  skill_install: { platform: 'codex', project_dir: '.', mode: 'symlink', dry_run: true },
  maintenance_init: {},
  maintenance_self_test: {},
  hooks_install: { host: 'codex', project_dir: '.', dry_run: true },
  hooks_check: { host: 'codex', project_dir: '.', strict: true },
  hooks_remove: { host: 'codex', project_dir: '.', dry_run: true },
  hook_run: { event: 'pre-edit', payload: { cwd: '.', tool_name: 'file', tool_input: { path: 'src/a.ts' } } },
  schema_commands: { all: true },
  schema_command: { noun: 'signal', subcommand: 'list' },
  schema_entities: { all: true },
  schema_list: {},
  schema_json_schema: { schema_name: 'memory_recall' },
  schema_example: { schema_name: 'memory_recall' },
  schema_validate: { schema_name: 'memory_recall', input: { query: 'current task' } },
} as const;
