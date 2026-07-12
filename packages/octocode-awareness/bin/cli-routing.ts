/**
 * awareness.ts — CLI entry point for @octocodeai/octocode-awareness.
 *
 * Thin wrapper: parse args → call domain functions → emit JSON.
 * Compiled to out/octocode-awareness.js by build.mjs.
 */
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ArgValue, BOOLEAN_FLAGS, GLOBAL_FLAGS, NUMERIC_FLAGS, ParsedArgs, RETENTION_DAY_FLAGS, VALUE_REQUIRED_FLAGS } from './cli-model.js';
import { COMMAND_DISPLAY, COMMAND_EXAMPLE, COMMAND_TO_SCHEMA } from './cli-help-data.js';

export const KNOWN_FLAGS: Record<string, string[]> = {
  'tell-memory': ['agent_id', 'task_context', 'observation', 'importance', 'label', 'tag', 'reference', 'supersedes', 'failure_signature', 'valid_from', 'valid_to', 'workspace', 'artifact', 'repo', 'ref', 'file', 'file_tree_fingerprint', 'allow_similar'],
  'get-memory': ['query', 'limit', 'min_importance', 'label', 'tag', 'smart', 'workspace', 'artifact', 'repo', 'ref', 'state', 'sort', 'global_only', 'strict_scope', 'as_of', 'reference', 'regex', 'file_regex', 'file', 'explain', 'semantic', 'full'],
  'forget': ['memory_id', 'tag', 'tags', 'before', 'max_importance', 'workspace', 'artifact', 'repo', 'ref', 'dry_run'],
  'memory-archive': ['memory_id', 'workspace', 'artifact', 'repo', 'ref', 'dry_run'],
  'memory-restore': ['memory_id', 'workspace', 'artifact', 'repo', 'ref', 'dry_run'],
  'reflect': ['agent_id', 'task', 'outcome', 'lesson', 'worked', 'didnt_work', 'fix_repo', 'fix_file', 'fix_harness', 'fix_instructions', 'failure_signature', 'importance', 'judgment_note', 'duo', 'eval_failure_json', 'workspace', 'artifact', 'repo', 'ref', 'allow_similar'],
  'refine-set': ['agent_id', 'reasoning', 'remember', 'quality', 'state', 'workspace', 'artifact', 'repo', 'ref', 'file', 'refinement_id', 'check_receipt'],
  'refine-get': ['workspace', 'artifact', 'repo', 'ref', 'quality', 'include_handoffs', 'state', 'limit', 'full'],
  'refine-delete': ['refinement_id', 'workspace', 'artifact', 'dry_run'],
  'pre-flight-intent': ['agent_id', 'workspace', 'artifact', 'run_id', 'rationale', 'test_plan', 'context_ref', 'target_file', 'file', 'ttl_minutes', 'ttl_seconds', 'wait_seconds', 'retry_interval', 'strict_agent_id'],
  'release-file-lock': ['agent_id', 'run_id', 'lock_id', 'target_file', 'file', 'status', 'workspace', 'artifact'],
  'status': ['workspace', 'artifact', 'limit'],
  'init': [],
  'self-test': [],
  'prune-stale-locks': ['older_than_minutes', 'expired_only', 'agent_id', 'target_file', 'workspace', 'artifact', 'dry_run'],
  'audit-unverified': ['agent_id', 'workspace', 'artifact', 'older_than_days', 'origin', 'before'],
  'verify': ['run_id', 'all_pending', 'agent_id', 'status', 'message', 'workspace', 'artifact'],
  'mine-weakness': ['agent_id', 'workspace', 'artifact', 'min_count', 'limit', 'cwd'],
  'doc-staleness': ['agent_id', 'workspace', 'artifact', 'targets_json', 'min_edits', 'min_lines', 'propose', 'session_id'],
  'docs-catalog': ['action', 'name', 'full'],
  'export-harness': ['limit', 'min_importance', 'workspace', 'artifact'],
  'developer-review': ['workspace', 'artifact', 'repo', 'ref', 'state', 'limit', 'format', 'query'],
  'query': ['view', 'query', 'limit', 'format', 'out', 'workspace', 'artifact', 'repo', 'ref', 'agent_id', 'state', 'label', 'file', 'since', 'include_bodies'],
  'attend': ['agent_id', 'query', 'limit', 'workspace', 'artifact', 'repo', 'ref', 'file', 'include_bodies', 'explain_organ'],
  'repo-inject': ['query', 'limit', 'out', 'out_dir', 'workspace', 'artifact', 'repo', 'ref', 'mode', 'check', 'include_view', 'prune_orphans'],
  'agent-registry': ['action', 'agent_id', 'agent_name', 'workspace', 'artifact', 'context', 'limit'],
  'agent-signal': ['action', 'agent_id', 'workspace', 'artifact', 'repo', 'ref', 'kind', 'subject', 'body', 'to_agent', 'file', 'ref_id', 'importance', 'in_reply_to', 'thread_id', 'signal_id', 'all', 'unread_only', 'mark_read', 'limit', 'include_bodies', 'format'],
  'notify-prune': ['agent_id', 'signal_id', 'resolved', 'older_than_days', 'dry_run', 'workspace', 'artifact'],
  'session-capture': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'reason', 'cwd'],
  'wait-for-lock': ['agent_id', 'target_file', 'file', 'workspace', 'artifact', 'wait_seconds', 'retry_interval'],
  'digest': ['retention_days', 'refinement_handoff_retention_days', 'refinement_done_retention_days', 'operational_retention_days', 'pressure_age_days', 'dry_run', 'export_doc', 'workspace', 'artifact'],
  'hook-run': [],
  'hooks-install': ['host', 'project_dir', 'global', 'check', 'strict', 'dry_run', 'remove'],
  'schema': ['examples', 'all'],
  'plan-command': ['action', 'plan_id', 'name', 'objective', 'lead_agent_id', 'agent_id', 'workspace', 'artifact', 'status', 'path', 'title', 'limit', 'full'],
  'task-command': ['action', 'task_id', 'plan_id', 'workspace', 'title', 'reasoning', 'acceptance', 'path', 'created_by', 'agent_id', 'priority', 'depends_on', 'run_id', 'lease_minutes', 'message', 'blocked_reason', 'test_plan', 'status', 'next', 'limit', 'full'],
  'work-command': ['action', 'agent_id', 'session_id', 'workspace', 'artifact', 'run_id', 'rationale', 'test_plan', 'context_ref', 'target_file', 'file', 'exclusive', 'ttl_minutes', 'ttl_seconds', 'all', 'full', 'limit'],
};

export function validateFlags(command: string, args: ParsedArgs): string[] {
  const known = KNOWN_FLAGS[command];
  if (!known) return [];
  const allowed = new Set([...known, ...GLOBAL_FLAGS]);
  return Object.keys(args).filter((k) => k !== '_' && !allowed.has(k));
}

/**
 * Reject silently-coerced flag values: non-integer numeric flags and
 * value-required flags that got boolean-coerced (`--query --smart`). Runs for
 * every command so bad input fails loudly instead of falling back to a default.
 */
export function validateFlagValues(args: ParsedArgs): void {
  for (const key of Object.keys(args)) {
    if (key === '_') continue;
    const value = args[key];
    if (value === false && !BOOLEAN_FLAGS.has(key)) {
      die(`--no-${key.replace(/_/g, '-')} is invalid because --${key.replace(/_/g, '-')} expects a value`);
    } else if (NUMERIC_FLAGS.has(key)) {
      const n = typeof value === 'string' ? Number(value) : NaN;
      if (value === true || !Number.isInteger(n)) {
        die(`--${key.replace(/_/g, '-')} expects an integer`, { got: value === true ? 'flag with no value' : String(value) });
      }
      if (RETENTION_DAY_FLAGS.has(key) && (n < 1 || n > 3650)) {
        die(`--${key.replace(/_/g, '-')} must be in 1..3650`, { got: n });
      }
    } else if (VALUE_REQUIRED_FLAGS.has(key) && value === true) {
      die(`--${key.replace(/_/g, '-')} expects a value (it was followed by another flag)`);
    }
  }
}

export function parseBoundedSeconds(args: ParsedArgs, key: string, min: number, max: number): number | null {
  const raw = args[key];
  if (raw == null || raw === false) return null;
  const flag = `--${key.replace(/_/g, '-')}`;
  const value = Number(String(raw));
  if (!Number.isInteger(value)) die(`${flag} must be an integer`);
  if (value < min) die(`${flag} must be >= ${min}`);
  if (value > max) die(`${flag} must be <= ${max}`);
  return value;
}

export function listLimit(args: ParsedArgs, defaultLimit = 20): number {
  const value = Number(String(args['limit'] ?? defaultLimit));
  if (!Number.isInteger(value) || value < 1) die('--limit must be a positive integer');
  return Math.min(value, 200);
}

export function extractGlobalDb(argv: string[]): { dbPath: string | null; filtered: string[] } {
  let dbPath: string | null = null;
  const filtered: string[] = [];
  let i = 0;
  while (i < argv.length) {
    if (argv[i] === '--db') {
      const value = argv[i + 1];
      if (value === undefined || value.startsWith('--')) die('--db expects a path');
      dbPath = value; i += 2;
    } else {
      filtered.push(argv[i]!); i++;
    }
  }
  return { dbPath, filtered };
}

export interface CommandRoute {
  command: string;
  prepend?: string[];
}

export const COMMAND_ROUTES: Record<string, CommandRoute> = {
  'memory record': { command: 'tell-memory' },
  'memory recall': { command: 'get-memory' },
  'memory forget': { command: 'forget' },
  'memory archive': { command: 'memory-archive' },
  'memory restore': { command: 'memory-restore' },
  'workspace status': { command: 'status' },
  'lock acquire': { command: 'pre-flight-intent' },
  'lock release': { command: 'release-file-lock' },
  'lock wait': { command: 'wait-for-lock' },
  'lock prune': { command: 'prune-stale-locks' },
  'plan create': { command: 'plan-command', prepend: ['--action', 'create'] },
  'plan list': { command: 'plan-command', prepend: ['--action', 'list'] },
  'plan show': { command: 'plan-command', prepend: ['--action', 'show'] },
  'plan join': { command: 'plan-command', prepend: ['--action', 'join'] },
  'plan doc': { command: 'plan-command', prepend: ['--action', 'doc'] },
  'plan status': { command: 'plan-command', prepend: ['--action', 'status'] },
  'task create': { command: 'task-command', prepend: ['--action', 'create'] },
  'task list': { command: 'task-command', prepend: ['--action', 'list'] },
  'task ready': { command: 'task-command', prepend: ['--action', 'ready'] },
  'task show': { command: 'task-command', prepend: ['--action', 'show'] },
  'task claim': { command: 'task-command', prepend: ['--action', 'claim'] },
  'task heartbeat': { command: 'task-command', prepend: ['--action', 'heartbeat'] },
  'task submit': { command: 'task-command', prepend: ['--action', 'submit'] },
  'task release': { command: 'task-command', prepend: ['--action', 'release'] },
  'task depend': { command: 'task-command', prepend: ['--action', 'depend'] },
  'work start': { command: 'work-command', prepend: ['--action', 'start'] },
  'work touch': { command: 'work-command', prepend: ['--action', 'touch'] },
  'work end': { command: 'work-command', prepend: ['--action', 'end'] },
  'work list': { command: 'work-command', prepend: ['--action', 'list'] },
  'work show': { command: 'work-command', prepend: ['--action', 'show'] },
  'verify mark': { command: 'verify' },
  'verify audit': { command: 'audit-unverified' },
  'refinement set': { command: 'refine-set' },
  'refinement get': { command: 'refine-get' },
  'refinement delete': { command: 'refine-delete' },
  'signal publish': { command: 'agent-signal', prepend: ['--action', 'publish'] },
  'signal list': { command: 'agent-signal', prepend: ['--action', 'list'] },
  'signal reply': { command: 'agent-signal', prepend: ['--action', 'reply'] },
  'signal ack': { command: 'agent-signal', prepend: ['--action', 'ack'] },
  'signal resolve': { command: 'agent-signal', prepend: ['--action', 'resolve'] },
  'signal prune': { command: 'notify-prune' },
  'agent register': { command: 'agent-registry', prepend: ['--action', 'register'] },
  'agent list': { command: 'agent-registry', prepend: ['--action', 'list'] },
  'session capture': { command: 'session-capture' },
  'reflect record': { command: 'reflect' },
  'reflect mine-weakness': { command: 'mine-weakness' },
  'reflect export-harness': { command: 'export-harness' },
  'reflect developer-review': { command: 'developer-review' },
  'docs list': { command: 'docs-catalog', prepend: ['--action', 'list'] },
  'docs show': { command: 'docs-catalog', prepend: ['--action', 'show'] },
  'docs staleness': { command: 'doc-staleness' },
  'maintenance digest': { command: 'digest' },
  'maintenance init': { command: 'init' },
  'maintenance self-test': { command: 'self-test' },
  'wiki sync': { command: 'repo-inject' },
  'repo inject': { command: 'repo-inject' },
};

export const SINGLE_COMMANDS = new Set(['query', 'attend', 'schema']);
export const UNKNOWN_COMMAND = '__unknown__';

export function normalizeToken(value: string | undefined): string | undefined {
  return value?.replace(/_/g, '-');
}

export function selectCommand(argv: string[]): { command: string | undefined; rest: string[] } {
  const [firstRaw, secondRaw, thirdRaw, ...tail] = argv;
  const first = normalizeToken(firstRaw);
  if (!first) return { command: undefined, rest: [] };
  if (first.startsWith('-')) {
    // Tolerate a leading global flag (e.g. `--compact workspace status`): pull
    // it off, re-select on the remainder, and re-append it so parseArgs still
    // sees it. Without this the whole argv was mis-read as one unknown command.
    if (first === '--compact' && argv.length > 1) {
      const sel = selectCommand(argv.slice(1));
      if (sel.command && sel.command !== UNKNOWN_COMMAND) {
        return { command: sel.command, rest: [...sel.rest, '--compact'] };
      }
    }
    return argv.every((arg) => arg === '--compact')
      ? { command: undefined, rest: argv }
      : { command: UNKNOWN_COMMAND, rest: argv };
  }

  const second = normalizeToken(secondRaw);
  if (first === 'hook' && second === 'run') {
    return { command: 'hook-run', rest: thirdRaw ? [thirdRaw, ...tail] : tail };
  }
  if (first === 'hooks' && second) {
    if (second === 'install') return { command: 'hooks-install', rest: thirdRaw ? [thirdRaw, ...tail] : tail };
    if (second === 'check') return { command: 'hooks-install', rest: ['--check', ...(thirdRaw ? [thirdRaw, ...tail] : tail)] };
    if (second === 'remove') return { command: 'hooks-install', rest: ['--remove', ...(thirdRaw ? [thirdRaw, ...tail] : tail)] };
  }
  if (first === 'schema') {
    return { command: 'schema', rest: secondRaw ? [secondRaw, ...(thirdRaw ? [thirdRaw, ...tail] : tail)] : [] };
  }

  if (second) {
    const route = COMMAND_ROUTES[`${first} ${second}`];
    if (route) return { command: route.command, rest: [...(route.prepend ?? []), ...(thirdRaw ? [thirdRaw, ...tail] : tail)] };
  }

  if (SINGLE_COMMANDS.has(first)) {
    return { command: first, rest: secondRaw ? [secondRaw, ...(thirdRaw ? [thirdRaw, ...tail] : tail)] : [] };
  }

  return { command: UNKNOWN_COMMAND, rest: argv };
}

export function packageSkillScriptPath(...segments: string[]): string {
  const here = dirname(fileURLToPath(import.meta.url));
  const invokedDir = process.argv[1] ? dirname(resolve(process.argv[1])) : here;
  // Prefer the self-contained out/skills bundle. Code-split modules execute from
  // out/chunks, so resource discovery is anchored to the stable invoked entry.
  const candidates = [
    process.env.OCTOCODE_SKILL_ROOT ? join(process.env.OCTOCODE_SKILL_ROOT, 'scripts') : null,
    join(invokedDir, 'skills', 'octocode-awareness', 'scripts'),
    invokedDir,
    join(process.cwd(), 'skills', 'octocode-awareness', 'scripts'),
    join(here, '..', 'skills', 'octocode-awareness', 'scripts'),
  ].filter((candidate): candidate is string => Boolean(candidate));
  const scriptsDir = candidates.find((candidate) =>
    existsSync(join(candidate, 'schema.mjs')) || existsSync(join(candidate, 'hooks')),
  ) ?? candidates[0]!;
  return join(scriptsDir, ...segments);
}

export function valuesFor(args: ParsedArgs, key: string): string[] {
  const value = args[key];
  if (value === undefined || value === false) return [];
  return Array.isArray(value) ? value.map(String) : [String(value)];
}

export function firstValue(args: ParsedArgs, key: string): string | undefined {
  return valuesFor(args, key)[0];
}

export function flagBool(value: ArgValue | undefined, fallback?: boolean): boolean | undefined {
  if (value === undefined) return fallback;
  if (value === false) return false;
  if (value === true) return true;
  const normalized = String(value).trim().toLowerCase();
  if (normalized === 'false' || normalized === '0' || normalized === 'no') return false;
  if (normalized === 'true' || normalized === '1' || normalized === 'yes') return true;
  return Boolean(value);
}

// ─── Output ───────────────────────────────────────────────────────────────────

export interface EmitOptions { compact?: boolean }

function compactValue(value: unknown, key?: string): unknown {
  if (key === 'db_path' || value === null || value === undefined) return undefined;
  if (Array.isArray(value)) return value.map((item) => compactValue(item)).filter((item) => item !== undefined);
  if (typeof value !== 'object') return value;
  const out: Record<string, unknown> = {};
  for (const [childKey, childValue] of Object.entries(value as Record<string, unknown>)) {
    const compacted = compactValue(childValue, childKey);
    if (compacted !== undefined) out[childKey] = compacted;
  }
  if (key === 'filters') {
    delete out['limit'];
    for (const [filterKey, filterValue] of Object.entries(out)) {
      if (Array.isArray(filterValue) && filterValue.length === 0) delete out[filterKey];
    }
    if (Object.keys(out).length === 0) return undefined;
  }
  if (typeof out['count'] === 'number' && out['total'] === out['count']) delete out['total'];
  if (out['omitted_count'] === 0) delete out['omitted_count'];
  if (out['is_partial'] === false) delete out['is_partial'];
  if (typeof out['workspace_path'] === 'string' && Array.isArray(out['rows'])) {
    out['rows'] = out['rows'].map((row) => {
      if (!row || typeof row !== 'object' || Array.isArray(row)) return row;
      const projected = { ...(row as Record<string, unknown>) };
      delete projected['workspace_path'];
      return projected;
    });
  }
  return out;
}

// Set once command routing resolves (see selectCommand call at the bottom);
// lets every error path — flag parsing, domain validation, thrown domain
// errors — carry the same {command,schema,example} recovery context instead
// of a bare {error}. Empty until routing runs (e.g. --db parse errors).
export let activeCommand = '';

export function setActiveCommand(command: string): void {
  activeCommand = command;
}

export function errorContext(): Record<string, unknown> {
  const context: Record<string, unknown> = {};
  const display = COMMAND_DISPLAY[activeCommand];
  if (display) context['command'] = display;
  const schema = COMMAND_TO_SCHEMA[activeCommand];
  if (schema) context['schema'] = `octocode-awareness schema json-schema ${schema} --compact`;
  const example = COMMAND_EXAMPLE[activeCommand];
  if (example) context['example'] = example;
  return context;
}

export function emit(payload: Record<string, unknown>, exitCode = 0, opts: EmitOptions = {}): number {
  payload['ok'] = payload['ok'] ?? (exitCode === 0);
  if (exitCode !== 0 && typeof payload['error'] === 'string' && payload['command'] === undefined) {
    Object.assign(payload, { ...errorContext(), ...payload });
  }
  const compact = opts.compact === true || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
  const output = compact ? compactValue(payload) : payload;
  process.stdout.write((compact ? JSON.stringify(output) : JSON.stringify(output, null, 2)) + '\n');
  return exitCode;
}

export function die(message: string, extras: Record<string, unknown> = {}): never {
  const compact = process.argv.includes('--compact') || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
  process.stdout.write(JSON.stringify({ ok: false, error: message, ...errorContext(), ...extras }, null, compact ? 0 : 2) + '\n');
  process.exit(1);
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/**
 * Resolve the acting agent id with a stable precedence: explicit --agent-id,
 * then the OCTOCODE_AGENT_ID env (exported by hosts such as the Pi extension so
 * hooks and CLI calls share one identity), then the literal 'agent' fallback.
 * This lets a harness declare work via hooks and later verify/reflect via the
 * CLI under the same id without passing --agent-id on every call.
 */
export function resolveAgentId(args: ParsedArgs): string {
  return String(args['agent_id'] ?? process.env.OCTOCODE_AGENT_ID ?? 'agent');
}
