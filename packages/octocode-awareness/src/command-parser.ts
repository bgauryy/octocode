// Tokenization for the canonical shell adapter.
import type { ParsedArgs } from './commands/args.js';

export const ARRAY_FLAGS = new Set(['section', 'tag', 'tags', 'reference', 'file', 'fix_file', 'target_file', 'supersedes', 'label', 'state', 'memory_id', 'signal_id', 'ref_id', 'run_id', 'regex', 'file_regex', 'to_agent', 'kind', 'path', 'depends_on', 'origin']);

export function parseArgs(argv: string[]): ParsedArgs {
  const result: ParsedArgs = { _: [] };
  let i = 0;
  while (i < argv.length) {
    const arg = argv[i]!;
    if (arg === '--') {
      result._.push(...argv.slice(i + 1));
      break;
    }
    if (arg.startsWith('--no-')) {
      result[arg.slice(5).replace(/-/g, '_')] = false;
      i++;
      continue;
    }
    if (arg.startsWith('--')) {
      const equalsIndex = arg.indexOf('=');
      const key = (equalsIndex >= 0 ? arg.slice(2, equalsIndex) : arg.slice(2)).replace(/-/g, '_');
      if (equalsIndex >= 0) {
        const value = arg.slice(equalsIndex + 1);
        if (ARRAY_FLAGS.has(key)) {
          const cur = result[key];
          result[key] = Array.isArray(cur) ? [...cur, value] : [value];
        } else result[key] = parseFlagValue(key, value || true);
        i++;
        continue;
      }
      const next = argv[i + 1];
      if (next === undefined || next.startsWith('--')) {
        result[key] = true;
        i++;
        continue;
      }
      i += 2;
      if (ARRAY_FLAGS.has(key)) {
        const cur = result[key];
        result[key] = Array.isArray(cur) ? [...cur, next] : [next];
      } else {
        result[key] = parseFlagValue(key, next);
      }
      continue;
    }
    result._.push(arg);
    i++;
  }
  return result;
}

// Normalize at the CLI boundary: consumers may use Boolean(value), so a
// recognized false token must never survive as a truthy string.
function parseFlagValue(key: string, value: string | boolean): string | boolean {
  if (!BOOLEAN_FLAGS.has(key) || typeof value !== 'string') return value;
  const normalized = value.trim().toLowerCase();
  if (['false', '0', 'no'].includes(normalized)) return false;
  if (['true', '1', 'yes'].includes(normalized)) return true;
  return value; // validateFlagValues rejects unknown Boolean tokens.
}

// Flags whose value must parse to an integer. Without this, `--limit abc` (NaN)
// or `--limit --smart` (boolean-coerced) silently fell back to a default and
// read as "it worked". Excludes flags that already have dedicated validation
// with their own messages/bounds (wait_seconds, retry_interval via
// parseBoundedSeconds; ttl_*; importance on memory record).
export const NUMERIC_FLAGS = new Set(['limit', 'min_importance', 'max_importance', 'older_than_days', 'priority', 'lease_minutes']);
export const RETENTION_DAY_FLAGS = new Set<string>();
// Only these flags may use the `--no-*` spelling. Treating every `--no-*`
// token as false let required scalar values such as `--agent-id` and
// `--task-context` evade validation.
export const BOOLEAN_FLAGS = new Set(['compact', 'details', 'changes', 'help', 'smart', 'global_only', 'strict_scope', 'all_workspaces', 'explain', 'semantic', 'full', 'dry_run', 'include_handoffs', 'strict_agent_id', 'verified', 'expired_only', 'all_pending', 'propose', 'fail_stale_active_runs', 'include_bodies', 'explain_organ', 'check', 'include_view', 'all', 'unread_only', 'mark_read', 'resolved', 'global', 'strict', 'remove', 'exclusive', 'next', 'duo', 'examples', 'allow_similar', 'prune_orphans', 'adopt_verification', 'force', 'capture_fingerprint', 'check_fingerprint']);
// Flags that must carry a value. Catches value-swallow like `--query --smart`,
// which parseArgs would otherwise read as query=true (searching the literal
// string "true"). Curated allowlist — unlisted flags are never falsely rejected.
export const VALUE_REQUIRED_FLAGS = new Set([
  'agent_name',
  'agent_vendor',
  'agent_host',
  'offset',
  'query',
  'observation',
  'lesson',
  'task',
  'task_context',
  'subject',
  'body',
  'rationale',
  'reasoning',
  'remember',
  'message',
  'fix_repo',
  'fix_harness',
  'fix_instructions',
  'in_reply_to',
  'thread_id',
  'name',
  'objective',
  'title',
  'acceptance',
  'blocked_reason',
  'path',
  'agent_id',
  'session_id',
  'workspace',
  'artifact',
  'repo',
  'ref',
  'run_id',
  'task_id',
  'plan_id',
  'test_plan',
  'context_ref',
  'target_file',
  'file',
  'status',
  'verified_note',
  'memory_id',
  'signal_id',
  'to_agent',
  'ref_id',
  'host',
  'platform',
  'project_dir',
  'out',
  'out_dir',
  'mode',
  'format',
  'view',
  'action',
  'kind',
  'label',
  'tag',
  'reference',
  'state',
  'sort',
  'as_of',
  'cwd',
  'depends_on',
  'failure_signature',
  'valid_from',
  'valid_to',
  'outcome',
  'quality',
  'reason',
  'targets_json',
  'origin',
  'supersedes',
  'regex',
  'file_regex',
  'tags',
  'db_scope',
  'agent_name',
  'context',
  'before',
  'importance',
  'check_receipt',
  'repository_scope',
  'memory_scope',
  'hook_profile',
  'profile',
]);
