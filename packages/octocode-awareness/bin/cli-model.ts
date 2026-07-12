/**
 * awareness.ts — CLI entry point for @octocodeai/octocode-awareness.
 *
 * Thin wrapper: parse args → call domain functions → emit JSON.
 * Compiled to out/octocode-awareness.js by build.mjs.
 */
import { existsSync, readdirSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// ─── Resolved paths ─────────────────────────────────────────────────────────
// Computed once at startup so help text shows real, copy-pasteable paths.

export const __bin = dirname(fileURLToPath(import.meta.url));
const invokedDir = process.argv[1] ? dirname(resolve(process.argv[1])) : __bin;
// out/octocode-awareness.js -> out/skills/; standalone skill scripts/awareness.mjs
// -> the sibling skills/ directory that contains both packaged skills.
export const BUNDLED_SKILLS_DIR = process.env.OCTOCODE_SKILL_ROOT
  ? resolve(process.env.OCTOCODE_SKILL_ROOT, '..')
  : basename(invokedDir) === 'scripts' && basename(dirname(invokedDir)) === 'octocode-awareness'
    ? resolve(invokedDir, '..', '..')
    : resolve(invokedDir, 'skills');

// Awareness is the only required operating skill. Skill lifecycle support and
// every other bundled skill are optional and installed only when needed.
export const REQUIRED_BUNDLED_SKILLS = new Set(['octocode-awareness']);

export interface BundledSkill {
  name: string;
  path: string;
  required: boolean;
}

// Discovered at runtime (not hardcoded) so this list can never silently drift
// from whatever build.mjs actually bundled next to this CLI.
export function discoverBundledSkills(skillsDir: string): BundledSkill[] {
  if (!existsSync(skillsDir)) return [];
  return readdirSync(skillsDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && existsSync(join(skillsDir, entry.name, 'SKILL.md')))
    .map((entry) => ({
      name: entry.name,
      path: join(skillsDir, entry.name),
      required: REQUIRED_BUNDLED_SKILLS.has(entry.name),
    }))
    .sort((a, b) => (a.required === b.required ? a.name.localeCompare(b.name) : a.required ? -1 : 1));
}

export const BUNDLED_SKILLS = discoverBundledSkills(BUNDLED_SKILLS_DIR);
export const BUNDLED_SKILLS_NAME_WIDTH = Math.max(0, ...BUNDLED_SKILLS.map((s) => s.name.length));
export const BUNDLED_SKILLS_BLOCK = BUNDLED_SKILLS
  .map((s) => `  ${s.name.padEnd(BUNDLED_SKILLS_NAME_WIDTH)} : ${s.path}  (${s.required ? 'required' : 'optional'})`)
  .join('\n');

// ─── Arg parser ───────────────────────────────────────────────────────────────

export type ArgValue = string | boolean | string[];
export type ParsedArgs = Record<string, ArgValue> & { _: string[] };

export const MAX_CLI_TTL_SECONDS = 10 * 60;
export const MAX_CLI_WAIT_SECONDS = 60 * 60;
export const MAX_CLI_RETRY_INTERVAL_SECONDS = 5 * 60;
export const MEMORY_SORTS = new Set(['smart', 'score', 'importance', 'recent', 'accessed']);

export const ARRAY_FLAGS = new Set([
  'tag', 'tags', 'reference', 'file', 'fix_file', 'target_file', 'supersedes', 'label', 'state',
  'memory_id', 'refinement_id', 'signal_id', 'ref_id', 'run_id', 'regex', 'file_regex',
  'to_agent', 'kind', 'path', 'depends_on',
  'origin',
]);

export function parseArgs(argv: string[]): ParsedArgs {
  const result: ParsedArgs = { _: [] };
  let i = 0;
  while (i < argv.length) {
    const arg = argv[i]!;
    if (arg === '--') { result._.push(...argv.slice(i + 1)); break; }
    if (arg.startsWith('--no-')) {
      result[arg.slice(5).replace(/-/g, '_')] = false; i++; continue;
    }
    if (arg.startsWith('--')) {
      const key = arg.slice(2).replace(/-/g, '_');
      const next = argv[i + 1];
      if (next === undefined || next.startsWith('--')) {
        result[key] = true; i++; continue;
      }
      i += 2;
      if (ARRAY_FLAGS.has(key)) {
        const cur = result[key];
        result[key] = Array.isArray(cur) ? [...cur, next] : [next];
      } else {
        result[key] = next;
      }
      continue;
    }
    result._.push(arg); i++;
  }
  return result;
}

// Per-command flag allowlist. Documented flags that the runtime silently
// ignored were the #1 source of doc drift — unknown flags are now hard errors.
export const GLOBAL_FLAGS = ['db', 'compact', 'help'];

// Flags whose value must parse to an integer. Without this, `--limit abc` (NaN)
// or `--limit --smart` (boolean-coerced) silently fell back to a default and
// read as "it worked". Excludes flags that already have dedicated validation
// with their own messages/bounds (wait_seconds, retry_interval via
// parseBoundedSeconds; ttl_*; importance on memory record).
export const NUMERIC_FLAGS = new Set([
  'limit', 'min_importance', 'max_importance', 'min_count', 'min_edits',
  'min_lines', 'older_than_days', 'retention_days',
  'refinement_handoff_retention_days', 'refinement_done_retention_days',
  'operational_retention_days',
  'pressure_age_days',
  'priority', 'lease_minutes',
]);
export const RETENTION_DAY_FLAGS = new Set([
  'retention_days', 'refinement_handoff_retention_days',
  'refinement_done_retention_days', 'operational_retention_days',
  'pressure_age_days',
]);
// Only these flags may use the `--no-*` spelling. Treating every `--no-*`
// token as false let required scalar values such as `--agent-id` and
// `--task-context` evade validation.
export const BOOLEAN_FLAGS = new Set([
  'compact', 'help', 'smart', 'global_only', 'strict_scope', 'explain',
  'semantic', 'full', 'dry_run', 'include_handoffs', 'strict_agent_id',
  'verified', 'expired_only', 'all_pending', 'propose',
  'include_bodies', 'explain_organ', 'check', 'include_view', 'all',
  'unread_only', 'mark_read', 'resolved', 'global', 'strict', 'remove',
  'exclusive', 'next', 'duo', 'examples',
  'allow_similar', 'prune_orphans',
]);
// Flags that must carry a value. Catches value-swallow like `--query --smart`,
// which parseArgs would otherwise read as query=true (searching the literal
// string "true"). Curated allowlist — unlisted flags are never falsely rejected.
export const VALUE_REQUIRED_FLAGS = new Set([
  'query', 'observation', 'lesson', 'task', 'task_context', 'subject', 'body',
  'rationale', 'reasoning', 'remember', 'message', 'fix_repo', 'fix_harness',
  'fix_instructions', 'in_reply_to', 'thread_id',
  'name', 'objective', 'title', 'acceptance', 'blocked_reason', 'path',
  'agent_id', 'session_id', 'workspace', 'artifact', 'repo', 'ref', 'run_id',
  'task_id', 'plan_id', 'test_plan', 'context_ref', 'target_file', 'file',
  'status', 'verified_note', 'memory_id', 'refinement_id', 'signal_id',
  'to_agent', 'ref_id', 'host', 'project_dir', 'out', 'out_dir', 'mode',
  'format', 'view', 'action', 'kind', 'label', 'tag', 'reference', 'state',
  'sort', 'as_of', 'cwd', 'created_by', 'depends_on', 'failure_signature',
  'valid_from', 'valid_to', 'outcome', 'quality', 'reason', 'targets_json',
  'origin', 'supersedes', 'regex', 'file_regex', 'tags',
  'agent_name', 'context', 'before', 'importance', 'check_receipt',
]);
