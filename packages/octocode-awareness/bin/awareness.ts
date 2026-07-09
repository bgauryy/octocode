/**
 * awareness.ts — CLI entry point for @octocodeai/octocode-awareness.
 *
 * Thin wrapper: parse args → call domain functions → emit JSON.
 * Compiled to dist/bin/awareness.js by build.mjs.
 */

import { writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

import {
  connectDb, initDb, hasFts, resolveDbPath,
} from '../src/db.js';
import { insertMemory, getMemory, mineWeakness, forgetMemory, storeEmbedding, searchByEmbedding, lexicalSearch, bumpAccess } from '../src/memory.js';
import { resolveEmbedCommand, runHostEmbedder } from '../src/embed-host.js';
import { mineDocStaleness, proposeDocRefresh } from '../src/docs.js';
import { listSkillDocs, showSkillDoc } from '../src/docs-catalog.js';
import { insertRefinement, getRefinements, updateRefinement, deleteRefinement } from '../src/refinements.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { createPlan, getPlan, joinPlan, listPlans, registerPlanDocument, updatePlanStatus, type PlanStatus } from '../src/plans.js';
import {
  addTaskDependency, claimTask, createTask, getTask, heartbeatTaskClaim,
  listReadyTasks, listTasks, releaseTaskClaim, submitTask, type PlanTaskStatus,
} from '../src/tasks.js';
import { reflect } from '../src/reflect.js';
import type { EvalFailure, RefinementQuality } from '../src/types.js';
import { pruneStale, notifyGet, sessionCapture, waitForLock, digest, exportMemoryDoc, exportHarness, getWorkspaceStatus } from '../src/maintenance.js';
import { pruneNotifications, agentSignal } from '../src/notifications.js';
import { auditUnverified, markVerified } from '../src/verify.js';
import { registerAgent, listAgents } from '../src/agents.js';
import { hooksInstallUsage, runHooksInstall } from '../src/hooks-install.js';
import { attendAwareness } from '../src/attend.js';
import { developerReviewDoc, formatAwarenessQueryResult, injectRepoContext, queryAwareness } from '../src/repo-context.js';
import {
  normalizeNotificationKind,
  normalizeFilePath,
} from '../src/helpers.js';
import { normalizeWorkspacePath } from '../src/git.js';
import { runHookCommand } from './hook-runner.js';

// ─── Resolved paths ─────────────────────────────────────────────────────────
// Computed once at startup so help text shows real, copy-pasteable paths.

const __bin = dirname(fileURLToPath(import.meta.url));
// dist/bin/awareness.js → dist/skills/
const BUNDLED_SKILLS_DIR = join(__bin, '..', 'skills');

// ─── Arg parser ───────────────────────────────────────────────────────────────

type ArgValue = string | boolean | string[];
type ParsedArgs = Record<string, ArgValue> & { _: string[] };

const MAX_CLI_TTL_SECONDS = 10 * 60;
const MAX_CLI_WAIT_SECONDS = 60 * 60;
const MAX_CLI_RETRY_INTERVAL_SECONDS = 5 * 60;
const MEMORY_SORTS = new Set(['smart', 'score', 'importance', 'recent', 'accessed']);

const ARRAY_FLAGS = new Set([
  'tag', 'tags', 'reference', 'file', 'fix_file', 'target_file', 'supersedes', 'label', 'state',
  'memory_id', 'refinement_id', 'signal_id', 'ref_id', 'run_id', 'regex', 'file_regex',
  'to_agent', 'kind', 'path', 'depends_on',
]);

function parseArgs(argv: string[]): ParsedArgs {
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
const GLOBAL_FLAGS = ['db', 'compact', 'help'];

// Flags whose value must parse to an integer. Without this, `--limit abc` (NaN)
// or `--limit --smart` (boolean-coerced) silently fell back to a default and
// read as "it worked". Excludes flags that already have dedicated validation
// with their own messages/bounds (wait_seconds, retry_interval via
// parseBoundedSeconds; ttl_*; importance on memory record).
const NUMERIC_FLAGS = new Set([
  'limit', 'min_importance', 'max_importance', 'min_count', 'min_edits',
  'min_lines', 'older_than_days', 'retention_days',
  'refinement_handoff_retention_days', 'refinement_done_retention_days',
  'priority', 'lease_minutes',
]);
// Flags that must carry a value. Catches value-swallow like `--query --smart`,
// which parseArgs would otherwise read as query=true (searching the literal
// string "true"). Curated allowlist — unlisted flags are never falsely rejected.
const VALUE_REQUIRED_FLAGS = new Set([
  'query', 'observation', 'lesson', 'task', 'task_context', 'subject', 'body',
  'rationale', 'reasoning', 'remember', 'message', 'fix_repo', 'fix_harness',
  'fix_instructions', 'in_reply_to', 'thread_id',
  'name', 'objective', 'title', 'acceptance', 'blocked_reason', 'path',
]);
const KNOWN_FLAGS: Record<string, string[]> = {
  'tell-memory': ['agent_id', 'task_context', 'observation', 'importance', 'label', 'tag', 'reference', 'supersedes', 'failure_signature', 'valid_from', 'valid_to', 'workspace', 'artifact', 'repo', 'ref', 'file', 'file_tree_fingerprint', 'compat_coerce'],
  'get-memory': ['query', 'limit', 'min_importance', 'label', 'tag', 'smart', 'workspace', 'artifact', 'repo', 'ref', 'state', 'sort', 'global_only', 'strict_scope', 'as_of', 'reference', 'regex', 'file_regex', 'file', 'explain', 'semantic'],
  'forget': ['memory_id', 'tag', 'tags', 'before', 'max_importance', 'workspace', 'artifact', 'repo', 'ref', 'dry_run'],
  'reflect': ['agent_id', 'task', 'outcome', 'lesson', 'worked', 'didnt_work', 'fix_repo', 'fix_file', 'fix_harness', 'fix_instructions', 'failure_signature', 'importance', 'judgment_note', 'duo', 'eval_failure_json', 'workspace', 'artifact', 'repo', 'ref', 'compat_coerce'],
  'refine-set': ['agent_id', 'reasoning', 'remember', 'quality', 'state', 'workspace', 'artifact', 'repo', 'ref', 'file', 'refinement_id'],
  'refine-get': ['workspace', 'artifact', 'repo', 'ref', 'quality', 'include_handoffs', 'state', 'limit'],
  'refine-delete': ['refinement_id', 'workspace', 'artifact', 'dry_run'],
  'pre-flight-intent': ['agent_id', 'workspace', 'artifact', 'run_id', 'rationale', 'test_plan', 'context_ref', 'target_file', 'file', 'lock_type', 'ttl_minutes', 'ttl_seconds', 'wait_seconds', 'retry_interval', 'strict_agent_id'],
  'release-file-lock': ['agent_id', 'run_id', 'target_file', 'file', 'status', 'verified', 'verified_note', 'workspace', 'artifact'],
  'status': ['workspace', 'artifact', 'limit'],
  'init': [],
  'self-test': [],
  'prune-stale-locks': ['older_than_minutes', 'expired_only', 'agent_id', 'target_file', 'workspace', 'artifact', 'dry_run'],
  'audit-unverified': ['agent_id', 'workspace', 'artifact', 'abandon'],
  'verify': ['run_id', 'all_pending', 'agent_id', 'status', 'message', 'workspace', 'artifact'],
  'mine-weakness': ['agent_id', 'workspace', 'artifact', 'min_count', 'limit', 'cwd'],
  'doc-staleness': ['agent_id', 'workspace', 'artifact', 'targets_json', 'min_edits', 'min_lines', 'propose', 'session_id'],
  'docs-catalog': ['action', 'name'],
  'export-harness': ['limit', 'min_importance', 'workspace', 'artifact'],
  'developer-review': ['workspace', 'artifact', 'repo', 'ref', 'state', 'limit', 'format', 'query'],
  'query': ['view', 'query', 'limit', 'format', 'out', 'workspace', 'artifact', 'repo', 'ref', 'agent_id', 'state', 'label', 'file', 'since', 'include_bodies'],
  'attend': ['query', 'limit', 'workspace', 'artifact', 'repo', 'ref', 'file', 'include_bodies', 'explain_organ'],
  'repo-inject': ['query', 'limit', 'out', 'out_dir', 'workspace', 'artifact', 'repo', 'ref', 'mode', 'check', 'include_view', 'include_bodies'],
  'agent-registry': ['action', 'agent_id', 'agent_name', 'workspace', 'artifact', 'context', 'limit'],
  'agent-signal': ['action', 'agent_id', 'workspace', 'artifact', 'repo', 'ref', 'kind', 'subject', 'body', 'to_agent', 'file', 'ref_id', 'importance', 'in_reply_to', 'thread_id', 'signal_id', 'all', 'unread_only', 'mark_read', 'limit', 'format', 'compat_coerce'],
  'notify-prune': ['signal_id', 'resolved', 'older_than_days', 'dry_run', 'workspace', 'artifact'],
  'session-capture': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'reason', 'cwd'],
  'wait-for-lock': ['agent_id', 'target_file', 'file', 'workspace', 'artifact', 'lock_type', 'wait_seconds', 'retry_interval'],
  'digest': ['retention_days', 'refinement_handoff_retention_days', 'refinement_done_retention_days', 'dry_run', 'export_doc', 'workspace', 'artifact'],
  'hook-run': [],
  'hooks-install': ['host', 'project_dir', 'global', 'check', 'strict', 'dry_run', 'remove'],
  'schema': [],
  'plan-command': ['action', 'plan_id', 'name', 'objective', 'lead_agent_id', 'agent_id', 'workspace', 'artifact', 'status', 'path', 'title'],
  'task-command': ['action', 'task_id', 'plan_id', 'title', 'reasoning', 'acceptance', 'path', 'created_by', 'agent_id', 'priority', 'depends_on', 'run_id', 'lease_minutes', 'message', 'blocked_reason', 'test_plan', 'status', 'next'],
};

function validateFlags(command: string, args: ParsedArgs): string[] {
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
function validateFlagValues(args: ParsedArgs): void {
  for (const key of Object.keys(args)) {
    if (key === '_') continue;
    const value = args[key];
    if (NUMERIC_FLAGS.has(key)) {
      const n = typeof value === 'string' ? Number(value) : NaN;
      if (value === true || !Number.isInteger(n)) {
        die(`--${key.replace(/_/g, '-')} expects an integer`, { got: value === true ? 'flag with no value' : String(value) });
      }
    } else if (VALUE_REQUIRED_FLAGS.has(key) && value === true) {
      die(`--${key.replace(/_/g, '-')} expects a value (it was followed by another flag)`);
    }
  }
}

function parseBoundedSeconds(args: ParsedArgs, key: string, min: number, max: number): number | null {
  const raw = args[key];
  if (raw == null || raw === false) return null;
  const flag = `--${key.replace(/_/g, '-')}`;
  const value = Number(String(raw));
  if (!Number.isInteger(value)) die(`${flag} must be an integer`);
  if (value < min) die(`${flag} must be >= ${min}`);
  if (value > max) die(`${flag} must be <= ${max}`);
  return value;
}

function extractGlobalDb(argv: string[]): { dbPath: string | null; filtered: string[] } {
  let dbPath: string | null = null;
  const filtered: string[] = [];
  let i = 0;
  while (i < argv.length) {
    if (argv[i] === '--db' && i + 1 < argv.length) {
      dbPath = argv[i + 1]!; i += 2;
    } else {
      filtered.push(argv[i]!); i++;
    }
  }
  return { dbPath, filtered };
}

interface CommandRoute {
  command: string;
  prepend?: string[];
}

const COMMAND_ROUTES: Record<string, CommandRoute> = {
  'memory record': { command: 'tell-memory' },
  'memory recall': { command: 'get-memory' },
  'memory forget': { command: 'forget' },
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
  'repo inject': { command: 'repo-inject' },
};

const SINGLE_COMMANDS = new Set(['query', 'attend', 'schema']);
const UNKNOWN_COMMAND = '__unknown__';

function normalizeToken(value: string | undefined): string | undefined {
  return value?.replace(/_/g, '-');
}

function selectCommand(argv: string[]): { command: string | undefined; rest: string[] } {
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

function packageSkillScriptPath(...segments: string[]): string {
  const here = dirname(fileURLToPath(import.meta.url));
  // Prefer dist/skills/ (self-contained bundle) over root skills/ (source tree).
  // dist/skills/ is populated by build.mjs so dist/ is fully self-contained for
  // CLI installs and npm consumers that only ship dist/.
  const candidates = [
    join(here, '..', 'skills', 'octocode-awareness', 'scripts'),   // dist/skills/ — bundled, preferred
    join(here, '..', '..', 'skills', 'octocode-awareness', 'scripts'), // <packageRoot>/skills/ — source fallback
    here, // dist/bin/ — last resort
  ];
  const scriptsDir = candidates.find((candidate) =>
    existsSync(join(candidate, 'schema.mjs')) || existsSync(join(candidate, 'hooks')),
  ) ?? candidates[0]!;
  return join(scriptsDir, ...segments);
}

function valuesFor(args: ParsedArgs, key: string): string[] {
  const value = args[key];
  if (value === undefined || value === false) return [];
  return Array.isArray(value) ? value.map(String) : [String(value)];
}

function firstValue(args: ParsedArgs, key: string): string | undefined {
  return valuesFor(args, key)[0];
}

function flagBool(value: ArgValue | undefined, fallback?: boolean): boolean | undefined {
  if (value === undefined) return fallback;
  if (value === false) return false;
  if (value === true) return true;
  const normalized = String(value).trim().toLowerCase();
  if (normalized === 'false' || normalized === '0' || normalized === 'no') return false;
  if (normalized === 'true' || normalized === '1' || normalized === 'yes') return true;
  return Boolean(value);
}

// ─── Output ───────────────────────────────────────────────────────────────────

interface EmitOptions { compact?: boolean }

function emit(payload: Record<string, unknown>, exitCode = 0, opts: EmitOptions = {}): number {
  payload['ok'] = payload['ok'] ?? (exitCode === 0);
  const compact = opts.compact === true || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
  process.stdout.write((compact ? JSON.stringify(payload) : JSON.stringify(payload, null, 2)) + '\n');
  return exitCode;
}

function die(message: string, extras: Record<string, unknown> = {}): never {
  const compact = process.argv.includes('--compact') || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
  process.stdout.write(JSON.stringify({ ok: false, error: message, ...extras }, null, compact ? 0 : 2) + '\n');
  process.exit(1);
}

// ─── Commands ─────────────────────────────────────────────────────────────────

function cmdTellMemory(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const agentId = String(args['agent_id'] ?? 'agent');
  const taskContext = String(args['task_context'] ?? '');
  const observation = String(args['observation'] ?? '');
  const importanceLevel = args['importance'];

  if (!taskContext) die('--task-context is required');
  if (!observation) die('--observation is required');
  const imp = parseInt(String(importanceLevel), 10);
  if (isNaN(imp) || imp < 1 || imp > 10) die('--importance must be 1–10');

  const rawTag = args['tag'];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawRef = args['reference'];
  const references = Array.isArray(rawRef) ? rawRef : rawRef ? [String(rawRef)] : [];
  const rawFile = args['file'];
  const files = Array.isArray(rawFile) ? rawFile : rawFile ? [String(rawFile)] : [];
  const workspaceForFiles = args['workspace'] ? String(args['workspace']) : undefined;
  const fileReferences = files
    .map((file) => {
      const trimmed = file.trim();
      if (!trimmed) return null;
      if (trimmed.startsWith('file:')) return trimmed;
      const normalized = normalizeFilePath(trimmed, workspaceForFiles);
      return normalized ? `file:${normalized}` : null;
    })
    .filter((file): file is string => Boolean(file));
  const rawSup = args['supersedes'];
  const supersedes = Array.isArray(rawSup) ? rawSup : rawSup ? [String(rawSup)] : [];
  const rawLabel = args['label'];
  const label = Array.isArray(rawLabel) ? rawLabel[0] : String(rawLabel ?? '');
  const compatCoerce = Boolean(args['compat_coerce']);

  const { memory, superseded, noveltyScore, similarMemoryIds } = insertMemory(db, {
    agentId, taskContext, observation, importance: imp,
    label,
    tags, references: [...references, ...fileReferences], supersedes,
    failureSignature: args['failure_signature'] ? String(args['failure_signature']) : null,
    validFrom: args['valid_from'] ? String(args['valid_from']) : null,
    validTo: args['valid_to'] ? String(args['valid_to']) : null,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    fileTreeFingerprint: args['file_tree_fingerprint'] ? String(args['file_tree_fingerprint']) : null,
    compatCoerce,
  });

  // Consolidation surface (mem0 ADD/UPDATE/NOOP contract, LLM-free): when the
  // new memory overlaps existing ones, hand the calling agent the candidates
  // and let IT decide to supersede or forget — the store never guesses.
  const payload: Record<string, unknown> = { db_path: dbPath, memory, superseded };
  if (supersedes.length === 0 && noveltyScore < 0.5 && similarMemoryIds.length > 0) {
    payload['consolidation'] = {
      novelty_score: noveltyScore,
      similar_memory_ids: similarMemoryIds,
      hint: 'low novelty — review the similar memories; re-record with --supersedes <id> to replace one, or forget this one if redundant',
    };
  }
  const embedCmd = resolveEmbedCommand();
  if (embedCmd) {
    try {
      const text = `${taskContext}\n${observation}`.trim();
      const { embedding, model } = runHostEmbedder(text, { command: embedCmd });
      storeEmbedding(db, memory.memory_id, embedding, model);
      payload['embedding'] = { stored: true, model, dims: embedding.length };
    } catch (err) {
      payload['embedding'] = {
        stored: false,
        warning: err instanceof Error ? err.message : String(err),
      };
    }
  }
  return emit(payload, 0, opts);
}

function cmdGetMemory(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawLabel = args['label'];
  const labelArr = Array.isArray(rawLabel) ? rawLabel : rawLabel ? [String(rawLabel)] : undefined;
  const rawTag = args['tag'];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawState = args['state'];
  const states = rawState ? (Array.isArray(rawState) ? rawState : [String(rawState)]) : undefined;

  const rawReference = args['reference'];
  const references = Array.isArray(rawReference) ? rawReference : rawReference ? [String(rawReference)] : [];
  const rawRegex = args['regex'];
  const regex = Array.isArray(rawRegex) ? rawRegex : rawRegex ? [String(rawRegex)] : [];
  const rawFileRegex = args['file_regex'];
  const fileRegex = Array.isArray(rawFileRegex) ? rawFileRegex : rawFileRegex ? [String(rawFileRegex)] : [];
  const rawGetFiles = args['file'];
  const getFiles = Array.isArray(rawGetFiles) ? rawGetFiles : rawGetFiles ? [String(rawGetFiles)] : [];
  const sort = String(args['sort'] ?? 'smart');
  if (!MEMORY_SORTS.has(sort)) {
    die(`--sort must be one of: ${[...MEMORY_SORTS].join(', ')}`);
  }

  const result = getMemory(db, {
    query: String(args['query'] ?? ''),
    limit: parseInt(String(args['limit'] ?? '3'), 10),
    minImportance: parseInt(String(args['min_importance'] ?? '1'), 10),
    label: labelArr,
    tags,
    smart: args['smart'] === true || args['smart'] === 'true',
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    states,
    sort,
    globalOnly: Boolean(args['global_only']),
    strictScope: Boolean(args['strict_scope']),
    asOf: args['as_of'] ? String(args['as_of']) : null,
    references,
    regex,
    fileRegex,
    files: getFiles,
    explain: Boolean(args['explain']),
  });

  const payload: Record<string, unknown> = { db_path: dbPath, ...result };
  if (args['semantic']) {
    const embedCmd = resolveEmbedCommand();
    const queryText = String(args['query'] ?? '').trim();
    if (!embedCmd) {
      payload['warnings'] = [
        'semantic ranking is unavailable in the CLI (set OCTOCODE_EMBED_CMD or use library storeEmbedding()/searchByEmbedding()); results use lexical FTS + decay.',
      ];
    } else if (!queryText) {
      payload['warnings'] = [
        'semantic ranking skipped: --query is required when OCTOCODE_EMBED_CMD is set; results use lexical FTS + decay.',
      ];
    } else {
      try {
        const { embedding, model } = runHostEmbedder(queryText, { command: embedCmd });
        const limit = parseInt(String(args['limit'] ?? '3'), 10);
        const hits = searchByEmbedding(db, embedding, Math.max(limit, 1), 0.0, model);
        if (hits.length === 0) {
          payload['warnings'] = [
            `OCTOCODE_EMBED_CMD ran (model=${model}) but no stored embeddings matched; results use lexical FTS + decay. Record memories while OCTOCODE_EMBED_CMD is set to populate vectors.`,
          ];
        } else {
          // Re-apply the SAME scope/label/importance/state/tag/asOf filters the
          // lexical getMemory() call honored: searchByEmbedding only filters on
          // state='ACTIVE', so without this the semantic path would leak
          // cross-workspace, cross-label, low-importance memories. Feed the
          // cosine hits back through lexicalSearch as candidateMemoryIds with an
          // empty query (no FTS constraint) so only in-scope candidates survive,
          // then re-rank by cosine similarity.
          const simById = new Map(hits.map(hit => [hit.memory_id, hit.similarity]));
          const scoped = lexicalSearch(
            db,
            '',
            hits.length,
            parseInt(String(args['min_importance'] ?? '1'), 10),
            tags,
            labelArr ?? [],
            states ?? ['ACTIVE'],
            {
              workspacePath: args['workspace'] ? String(args['workspace']) : null,
              artifact: args['artifact'] ? String(args['artifact']) : null,
              repo: args['repo'] ? String(args['repo']) : null,
              ref: args['ref'] ? String(args['ref']) : null,
              strictScope: Boolean(args['strict_scope']),
              globalOnly: Boolean(args['global_only']),
              asOf: args['as_of'] ? String(args['as_of']) : null,
              candidateMemoryIds: hits.map(hit => hit.memory_id),
            },
          );
          const ranked = scoped
            .map(memory => {
              const similarity = simById.get(memory.memory_id) ?? 0;
              memory.score = similarity;
              memory.lexical = similarity;
              return memory;
            })
            .sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
          if (ranked.length === 0) {
            payload['warnings'] = [
              `OCTOCODE_EMBED_CMD ran (model=${model}) and matched embeddings, but none passed the scope/label/importance filters; results use lexical FTS + decay.`,
            ];
          } else {
            bumpAccess(db, ranked.map(memory => memory.memory_id));
            // Switching to semantic mode: drop the lexical-run judgment fields so
            // they don't misdescribe the semantic result set.
            delete payload['judgment_required'];
            delete payload['judgment_reason'];
            payload['memories'] = ranked.slice(0, limit);
            payload['count'] = Math.min(ranked.length, limit);
            payload['mode'] = 'semantic';
            payload['embedding_model'] = model;
          }
        }
      } catch (err) {
        payload['warnings'] = [
          `semantic ranking failed (${err instanceof Error ? err.message : String(err)}); results use lexical FTS + decay.`,
        ];
      }
    }
  }
  return emit(payload, 0, opts);
}

function cmdRefineSet(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawState = args['state'];
  const stateVal = Array.isArray(rawState) ? rawState[0] : String(rawState ?? 'open');
  const rawFile = args['file'];
  const files = Array.isArray(rawFile) ? rawFile : rawFile ? [String(rawFile)] : [];

  // Update path: --refinement-id changes only the passed fields
  // (open → ongoing → done lifecycle).
  const rawRefId = args['refinement_id'];
  const refinementId = Array.isArray(rawRefId) ? rawRefId[0] : rawRefId ? String(rawRefId) : null;
  if (refinementId && refinementId !== 'true') {
    const update = updateRefinement(db, {
      refinementId,
      ...(args['state'] !== undefined ? { state: stateVal as 'open' | 'ongoing' | 'done' } : {}),
      ...(args['quality'] !== undefined ? { quality: String(args['quality']) as RefinementQuality } : {}),
      ...(args['reasoning'] !== undefined ? { reasoning: String(args['reasoning']) } : {}),
      ...(args['remember'] !== undefined ? { remember: String(args['remember']) } : {}),
      ...(rawFile !== undefined ? { files } : {}),
    });
    if (!update.updated) die(`refinement not found: ${refinementId}`);
    return emit({ db_path: dbPath, updated: true, refinement: update.refinement }, 0, opts);
  }

  const reasoning = String(args['reasoning'] ?? '');
  const remember = String(args['remember'] ?? '');
  if (!reasoning) die('--reasoning is required');
  if (!remember) die('--remember is required');

  const { refinement } = insertRefinement(db, {
    agentId: String(args['agent_id'] ?? 'agent'),
    reasoning, remember,
    quality: (String(args['quality'] ?? 'good')) as RefinementQuality,
    state: (stateVal ?? 'open') as 'open' | 'ongoing' | 'done',
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    files,
  });

  return emit({ db_path: dbPath, refinement }, 0, opts);
}

function cmdRefineGet(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawState = args['state'];
  const states = rawState ? (Array.isArray(rawState) ? rawState : [String(rawState)]) : undefined;

  const result = getRefinements(db, {
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    quality: args['quality'] ? String(args['quality']) as RefinementQuality : undefined,
    includeHandoffs: Boolean(args['include_handoffs']),
    states,
    limit: parseInt(String(args['limit'] ?? '10'), 10),
  });

  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdReflect(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  if (!args['task']) die('--task is required');

  let evalFailures: EvalFailure[] = [];
  if (args['eval_failure_json']) {
    try {
      const parsed: unknown = JSON.parse(String(args['eval_failure_json']));
      if (!Array.isArray(parsed)) throw new Error('expected a JSON array');
      evalFailures = parsed as EvalFailure[];
    } catch (err) {
      die(`--eval-failure-json must be a JSON array of {id, dimension?, failure_signature?, suggested_lesson?}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  const compatCoerce = Boolean(args['compat_coerce']);
  const result = reflect(db, {
    agentId: String(args['agent_id'] ?? 'agent'),
    task: String(args['task']),
    outcome: args['outcome'] != null ? String(args['outcome']) : 'partial',
    lesson: args['lesson'] ? String(args['lesson']) : null,
    worked: args['worked'] ? String(args['worked']) : null,
    didntWork: args['didnt_work'] ? String(args['didnt_work']) : null,
    fixRepo: args['fix_repo'] ? String(args['fix_repo']) : null,
    fixHarness: args['fix_harness'] ? String(args['fix_harness']) : null,
    fixInstructions: args['fix_instructions'] ? String(args['fix_instructions']) : null,
    failureSignature: args['failure_signature'] ? String(args['failure_signature']) : null,
    importance: args['importance'] ? parseInt(String(args['importance']), 10) : null,
    judgmentNote: args['judgment_note'] ? String(args['judgment_note']) : null,
    duo: Boolean(args['duo']),
    evalFailures,
    files: Array.isArray(args['fix_file']) ? (args['fix_file'] as string[]) : [],
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    compatCoerce,
  });

  return emit({ ...result, db_path: dbPath }, 0, opts);
}

function cmdPreFlightIntent(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const envAgentId = process.env.OCTOCODE_AGENT_ID?.trim() || '';
  const argAgentId = args['agent_id'] ? String(args['agent_id']).trim() : '';
  const strictAgentId = Boolean(args['strict_agent_id']) || process.env.OCTOCODE_STRICT_AGENT_ID === '1';
  if (!argAgentId && !envAgentId) {
    const msg = 'lock acquire: set --agent-id or OCTOCODE_AGENT_ID so CLI and hooks share one identity';
    if (strictAgentId) die(msg);
    console.error(`octocode-awareness: warning: ${msg}`);
  }
  const rawTarget = args['target_file'] ?? args['file'];
  const targetFiles = Array.isArray(rawTarget) ? rawTarget : rawTarget ? [String(rawTarget)] : [];
  // Reject empty target — otherwise an ACTIVE run is created that locks
  // nothing (phantom lock) and pollutes the workboard. --target-file is required.
  if (targetFiles.length === 0) die('lock acquire requires at least one --target-file');
  const ttlMinutes = args['ttl_minutes'] ? parseInt(String(args['ttl_minutes']), 10) : null;
  const ttlSeconds = args['ttl_seconds'] ? parseInt(String(args['ttl_seconds']), 10) : null;
  if (ttlMinutes != null && (!Number.isInteger(ttlMinutes) || ttlMinutes < 1)) die('--ttl-minutes must be >= 1');
  if (ttlSeconds != null && (!Number.isInteger(ttlSeconds) || ttlSeconds < 1)) die('--ttl-seconds must be >= 1');
  if (ttlMinutes != null && ttlMinutes > 10) die('--ttl-minutes must be <= 10');
  if (ttlSeconds != null && ttlSeconds > MAX_CLI_TTL_SECONDS) die('--ttl-seconds must be <= 600');
  const ttlMs = ttlSeconds != null ? ttlSeconds * 1000 : ttlMinutes != null ? ttlMinutes * 60000 : null;

  const claimParams = {
    agentId: argAgentId || envAgentId || 'agent',
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    runId: firstValue(args, 'run_id') ?? null,
    rationale: String(args['rationale'] ?? 'agent write operation'),
    testPlan: String(args['test_plan'] ?? 'post-edit verification'),
    contextRef: args['context_ref'] ? String(args['context_ref']) : null,
    targetFiles,
    lockType: (String(args['lock_type'] ?? 'EXCLUSIVE')) as 'EXCLUSIVE' | 'SHARED',
    ttlMs,
  };
  let result = preFlightIntent(db, claimParams);

  // --wait-seconds: bounded wait for the current holder, then claim.
  // waitForLock sleeps outside SQLite transactions; a small window between
  // "clear" and the claim is inherent — the re-claim below closes it or conflicts again.
  const waitSeconds = parseBoundedSeconds(args, 'wait_seconds', 0, MAX_CLI_WAIT_SECONDS);
  const retrySeconds = parseBoundedSeconds(args, 'retry_interval', 1, MAX_CLI_RETRY_INTERVAL_SECONDS);
  if (!result.ok && waitSeconds != null && waitSeconds > 0) {
    const wait = waitForLock(db, {
      agent_id: claimParams.agentId,
      target_files: targetFiles,
      workspace: claimParams.workspacePath ?? undefined,
      artifact: claimParams.artifact ?? undefined,
      lock_type: claimParams.lockType,
      wait_ms: waitSeconds * 1000,
      retry_interval_ms: retrySeconds != null ? retrySeconds * 1000 : undefined,
    });
    if (wait.lock_free) result = preFlightIntent(db, claimParams);
  }

  if (!result.ok) return emit({ db_path: dbPath, ...result }, 2, opts);
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdAuditUnverified(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  // D1 fix: normalize the workspace filter to the git-root key (same as write
  // paths) so `verify audit` run from a package/subdir does not miss pending
  // work and report a false "0 unverified".
  const rawAuditWs = args['workspace'] ? String(args['workspace']) : null;
  const result = auditUnverified(db, {
    agentId: args['agent_id'] ? String(args['agent_id']) : null,
    workspacePath: rawAuditWs ? normalizeWorkspacePath(rawAuditWs, rawAuditWs) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    abandon: Boolean(args['abandon']),
  });
  return emit({ db_path: dbPath, ...result }, result.count > 0 ? 1 : 0, opts);
}

function cmdVerify(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const allPending = Boolean(args['all_pending']);
  const runIds = valuesFor(args, 'run_id');
  if (!allPending && runIds.length === 0) {
    return emit({ error: '--run-id is required (or use --all-pending)' }, 1, opts);
  }
  const statusArg = args['status'] ? String(args['status']) : 'SUCCESS';
  if (statusArg !== 'SUCCESS' && statusArg !== 'FAILED') {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts);
  }
  if (!allPending && runIds.length > 1) {
    const results = runIds.map((runId) => markVerified(db, {
      runId,
      agentId: String(args['agent_id'] ?? 'agent'),
      workspacePath: args['workspace'] ? String(args['workspace']) : null,
      artifact: args['artifact'] ? String(args['artifact']) : null,
      message: args['message'] ? String(args['message']) : undefined,
      status: statusArg as 'SUCCESS' | 'FAILED',
    }));
    const failed = results.find((result) => !result.ok);
    if (failed && !failed.ok) {
      return emit({ db_path: dbPath, ok: false, error: failed.error, run_id: null, run_ids: runIds, results }, 1, opts);
    }
    return emit({
      db_path: dbPath,
      run_id: null,
      run_ids: runIds,
      count: results.length,
      status: statusArg,
      results,
    }, 0, opts);
  }
  const result = markVerified(db, {
    runId: runIds[0],
    agentId: String(args['agent_id'] ?? 'agent'),
    allPending,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    message: args['message'] ? String(args['message']) : undefined,
    status: statusArg as 'SUCCESS' | 'FAILED',
  });
  return emit({ db_path: dbPath, ...result }, result.ok ? 0 : 1, opts);
}

function cmdReleaseFileLock(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawTarget = args['target_file'] ?? args['file'];
  const targetFiles = rawTarget
    ? (Array.isArray(rawTarget) ? rawTarget : [String(rawTarget)])
    : [];

  const runId = firstValue(args, 'run_id');
  if (!runId && targetFiles.length === 0) {
    return emit({ error: 'lock release requires --run-id or --target-file' }, 1, opts);
  }

  const result = releaseFileLock(db, {
    agentId: String(args['agent_id'] ?? 'agent'),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    runId: runId ?? null,
    targetFiles,
    status: (String(args['status'] ?? 'SUCCESS')) as 'PENDING' | 'SUCCESS' | 'FAILED',
    verified: Boolean(args['verified']),
    verifiedNote: args['verified_note'] ? String(args['verified_note']) : undefined,
  });

  // When release succeeded but verification is still pending, signal this clearly:
  // ok:false + exit 2 so agents don't interpret the release as fully complete and
  // then get unexpectedly blocked by stop-verify at session end.
  if (!result.released) {
    return emit({ db_path: dbPath, ...result, ok: false }, result.ambiguousRelease ? 2 : 1, opts);
  }
  if ('unverifiedConclusion' in result) {
    return emit({ db_path: dbPath, ...result, ok: false }, 2, opts);
  }
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function requiredArg(args: ParsedArgs, key: string): string {
  const value = args[key];
  if (value == null || value === true || !String(value).trim()) {
    die(`--${key.replace(/_/g, '-')} is required`);
  }
  return String(value).trim();
}

function cmdPlan(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const action = requiredArg(args, 'action');
  if (action === 'create') {
    const result = createPlan(db, {
      name: requiredArg(args, 'name'),
      objective: requiredArg(args, 'objective'),
      leadAgentId: String(args['lead_agent_id'] ?? args['agent_id'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim(),
      workspacePath: String(args['workspace'] ?? process.cwd()),
      artifact: args['artifact'] ? String(args['artifact']) : null,
    });
    return emit({ db_path: dbPath, ...result }, 0, opts);
  }
  if (action === 'list') {
    const plans = listPlans(db, {
      workspacePath: args['workspace'] ? String(args['workspace']) : null,
      artifact: args['artifact'] ? String(args['artifact']) : null,
      status: args['status'] ? String(args['status']).toUpperCase() as PlanStatus : null,
    });
    return emit({ db_path: dbPath, count: plans.length, plans }, 0, opts);
  }
  const planId = requiredArg(args, 'plan_id');
  if (action === 'show') {
    const plan = getPlan(db, planId);
    return plan
      ? emit({ db_path: dbPath, plan }, 0, opts)
      : emit({ db_path: dbPath, error: `plan not found: ${planId}` }, 1, opts);
  }
  if (action === 'join') {
    const member = joinPlan(db, { planId, agentId: String(args['agent_id'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim() });
    return emit({ db_path: dbPath, plan_id: planId, member }, 0, opts);
  }
  if (action === 'doc') {
    const document = registerPlanDocument(db, {
      planId,
      agentId: String(args['agent_id'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim(),
      relativePath: valuesFor(args, 'path')[0] ?? '',
      title: requiredArg(args, 'title'),
    });
    return emit({ db_path: dbPath, plan_id: planId, document }, 0, opts);
  }
  if (action === 'status') {
    const status = requiredArg(args, 'status').toUpperCase() as PlanStatus;
    if (!['DRAFT', 'ACTIVE', 'PAUSED', 'COMPLETED', 'CANCELLED'].includes(status)) {
      die('--status must be DRAFT, ACTIVE, PAUSED, COMPLETED, or CANCELLED');
    }
    const plan = updatePlanStatus(db, {
      planId,
      status,
      agentId: String(args['agent_id'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim(),
    });
    return emit({ db_path: dbPath, plan }, 0, opts);
  }
  return emit({ db_path: dbPath, error: `unknown plan action: ${action}` }, 1, opts);
}

function cmdTask(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const action = requiredArg(args, 'action');
  const agentId = String(args['agent_id'] ?? args['created_by'] ?? process.env.OCTOCODE_AGENT_ID ?? '').trim();
  if (action === 'create') {
    const result = createTask(db, {
      planId: requiredArg(args, 'plan_id'),
      title: requiredArg(args, 'title'),
      reasoning: requiredArg(args, 'reasoning'),
      acceptanceCriteria: args['acceptance'] ? String(args['acceptance']) : undefined,
      paths: valuesFor(args, 'path'),
      createdBy: agentId,
      priority: args['priority'] == null ? undefined : Number(args['priority']),
      dependsOn: valuesFor(args, 'depends_on'),
    });
    return emit({ db_path: dbPath, ...result }, 0, opts);
  }
  if (action === 'list' || action === 'ready') {
    const tasks = action === 'ready'
      ? listReadyTasks(db, { planId: args['plan_id'] ? String(args['plan_id']) : null })
      : listTasks(db, {
        planId: args['plan_id'] ? String(args['plan_id']) : null,
        status: args['status'] ? String(args['status']).toUpperCase() as PlanTaskStatus : null,
        agentId: args['agent_id'] ? agentId : null,
      });
    return emit({ db_path: dbPath, count: tasks.length, tasks }, 0, opts);
  }
  let taskId = args['task_id'] ? String(args['task_id']) : '';
  if (action === 'claim' && Boolean(args['next'])) {
    const planId = requiredArg(args, 'plan_id');
    taskId = listReadyTasks(db, { planId })[0]?.task_id ?? '';
    if (!taskId) return emit({ db_path: dbPath, error: `no ready tasks in plan ${planId}` }, 1, opts);
  }
  if (!taskId) die('--task-id is required');
  if (action === 'show') {
    const task = getTask(db, taskId);
    return task
      ? emit({ db_path: dbPath, task }, 0, opts)
      : emit({ db_path: dbPath, error: `task not found: ${taskId}` }, 1, opts);
  }
  if (action === 'depend') {
    const dependencies = valuesFor(args, 'depends_on');
    if (dependencies.length === 0) die('task depend requires at least one --depends-on');
    for (const dependsOnTaskId of dependencies) {
      addTaskDependency(db, { taskId, dependsOnTaskId, agentId });
    }
    return emit({ db_path: dbPath, task: getTask(db, taskId) }, 0, opts);
  }
  const leaseMinutes = args['lease_minutes'] == null ? undefined : Number(args['lease_minutes']);
  if (leaseMinutes != null && (leaseMinutes < 1 || leaseMinutes > 60)) die('--lease-minutes must be between 1 and 60');
  if (action === 'claim') {
    const result = claimTask(db, {
      taskId,
      agentId,
      leaseMs: leaseMinutes == null ? undefined : leaseMinutes * 60_000,
      testPlan: args['test_plan'] ? String(args['test_plan']) : undefined,
    });
    return emit({ db_path: dbPath, ...result }, result.ok ? 0 : 2, opts);
  }
  const runId = firstValue(args, 'run_id') ?? '';
  if (!runId) die('--run-id is required');
  if (action === 'heartbeat') {
    const claim = heartbeatTaskClaim(db, {
      taskId, runId, agentId,
      leaseMs: leaseMinutes == null ? undefined : leaseMinutes * 60_000,
    });
    return emit({ db_path: dbPath, claim }, 0, opts);
  }
  if (action === 'submit') {
    const result = submitTask(db, {
      taskId, runId, agentId,
      message: args['message'] ? String(args['message']) : undefined,
    });
    return emit({ db_path: dbPath, ...result }, 0, opts);
  }
  if (action === 'release') {
    const task = releaseTaskClaim(db, {
      taskId, runId, agentId,
      blockedReason: args['blocked_reason'] ? String(args['blocked_reason']) : null,
    });
    return emit({ db_path: dbPath, task }, 0, opts);
  }
  return emit({ db_path: dbPath, error: `unknown task action: ${action}` }, 1, opts);
}

function cmdForget(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawIds = args['memory_id'];
  const memoryIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const rawTags = [args['tag'], args['tags']].flatMap((v) =>
    Array.isArray(v) ? v : v && v !== true ? [String(v)] : []);
  const tags = rawTags;
  const result = forgetMemory(db, {
    memoryIds,
    tags,
    before: args['before'] ? String(args['before']) : undefined,
    maxImportance: args['max_importance'] ? parseInt(String(args['max_importance']), 10) : undefined,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    dryRun: Boolean(args['dry_run']),
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdRefineDelete(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawIds = args['refinement_id'];
  const refinementIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  if (refinementIds.length === 0) return emit({ error: '--refinement-id is required' }, 1, opts);
  const result = deleteRefinement(db, {
    refinementIds,
    workspacePath: args['workspace'] ? String(args['workspace']) : undefined,
    artifact: args['artifact'] ? String(args['artifact']) : undefined,
    dryRun: Boolean(args['dry_run']),
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdExportHarness(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const result = exportHarness(db, {
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : undefined,
    min_importance: args['min_importance'] ? parseInt(String(args['min_importance']), 10) : undefined,
    workspace_path: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdDeveloperReview(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const format = String(args['format'] ?? 'json').toLowerCase();
  const result = developerReviewDoc(db, {
    workspacePath: args['workspace'] ? String(args['workspace']) : process.cwd(),
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    query: args['query'] ? String(args['query']) : null,
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : undefined,
    state: Array.isArray(args['state']) ? args['state'].map(String) : args['state'] ? String(args['state']) : null,
  });
  if (format === 'markdown') {
    process.stdout.write(result.markdown);
    return 0;
  }
  return emit({
    db_path: dbPath,
    view: 'developer-review',
    open: result.open,
    resolved: result.resolved,
    count: result.rows.length,
    rows: result.rows,
  }, 0, opts);
}

function cmdQuery(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const view = String(args['view'] ?? args._[0] ?? 'all');
  const format = String(args['format'] ?? 'json').toLowerCase();
  const workspacePath = args['workspace'] ? String(args['workspace']) : process.cwd();
  const result = queryAwareness(db, {
    view,
    workspacePath,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    query: args['query'] ? String(args['query']) : null,
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : undefined,
    agentId: args['agent_id'] ? String(args['agent_id']) : null,
    state: Array.isArray(args['state']) ? args['state'].map(String) : args['state'] ? String(args['state']) : null,
    label: Array.isArray(args['label']) ? args['label'].map(String) : args['label'] ? String(args['label']) : null,
    file: args['file'] ? String(Array.isArray(args['file']) ? args['file'][0] : args['file']) : null,
    since: args['since'] ? String(args['since']) : null,
    includeBodies: flagBool(args['include_bodies']),
  });

  const outPath = args['out'] ? String(args['out']) : null;
  if (outPath) {
    const resolvedOutPath = isAbsolute(outPath) ? resolve(outPath) : resolve(workspacePath, outPath);
    mkdirSync(dirname(resolvedOutPath), { recursive: true });
    writeFileSync(resolvedOutPath, formatAwarenessQueryResult(result, format), 'utf8');
    return emit({ db_path: dbPath, path: resolvedOutPath, view: result.view, count: result.count }, 0, opts);
  }

  if (format === 'json') return emit({ db_path: dbPath, ...result }, 0, opts);
  process.stdout.write(formatAwarenessQueryResult(result, format));
  return 0;
}

function cmdAttend(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawFile = args['file'];
  const files = Array.isArray(rawFile) ? rawFile.map(String) : rawFile ? [String(rawFile)] : [];
  const result = attendAwareness(db, {
    workspacePath: args['workspace'] ? String(args['workspace']) : process.cwd(),
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    query: args['query'] ? String(args['query']) : null,
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : undefined,
    file: files,
    includeBodies: flagBool(args['include_bodies']),
    explainOrgan: flagBool(args['explain_organ']),
    compact: opts.compact,
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdRepoInject(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const outDir = args['out_dir'] ?? args['out'];
  const result = injectRepoContext(db, {
    workspacePath: args['workspace'] ? String(args['workspace']) : process.cwd(),
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    query: args['query'] ? String(args['query']) : null,
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : undefined,
    outDir: outDir ? String(outDir) : undefined,
    mode: args['mode'] ? String(args['mode']) : undefined,
    includeView: flagBool(args['include_view']),
    check: flagBool(args['check']),
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdDocsCatalog(_db: DatabaseSync, args: ParsedArgs, _dbPath: string, opts: EmitOptions): number {
  const action = String(args['action'] ?? args._[0] ?? 'list').trim().toLowerCase();
  if (action === 'list') {
    const result = listSkillDocs();
    return emit({
      ok: true,
      count: result.count,
      root: result.root,
      docs: result.docs.map((doc) => ({
        name: doc.name,
        title: doc.title,
        description: doc.description,
        kind: doc.kind,
        path: doc.path,
      })),
      next: 'octocode-awareness docs show <name> --compact',
    }, 0, opts);
  }
  if (action === 'show') {
    const name = String(args['name'] ?? args._[0] ?? '').trim();
    if (!name) return emit({ ok: false, error: 'docs show requires a name. Run docs list --compact.' }, 1, opts);
    const result = showSkillDoc(name);
    if (!result.ok) {
      return emit({ ok: false, error: result.error, suggestions: result.suggestions }, 1, opts);
    }
    if (opts.compact || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1') {
      return emit({
        ok: true,
        name: result.name,
        title: result.title,
        description: result.description,
        kind: result.kind,
        path: result.path,
        content: result.content,
      }, 0, opts);
    }
    process.stdout.write(`${result.content}${result.content.endsWith('\n') ? '' : '\n'}`);
    return 0;
  }
  return emit({ ok: false, error: `unknown docs action "${action}". Use docs list|show|staleness.` }, 1, opts);
}

function cmdDocStaleness(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawTargets = args['targets_json'];
  if (!rawTargets || typeof rawTargets !== 'string') {
    return emit({ error: '--targets-json is required, e.g. \'[{"docFile":"pkg/ARCHITECTURE.md","sourceDirs":["pkg/src"]}]\'' }, 1, opts);
  }
  let targets: Array<{ docFile: string; sourceDirs: string[] }>;
  try {
    const parsed = JSON.parse(rawTargets) as unknown;
    if (!Array.isArray(parsed)) throw new Error('not an array');
    targets = parsed.map((t) => {
      const obj = t as { docFile?: unknown; doc_file?: unknown; sourceDirs?: unknown; source_dirs?: unknown };
      const docFile = String(obj.docFile ?? obj.doc_file ?? '');
      const rawDirs = obj.sourceDirs ?? obj.source_dirs;
      const sourceDirs = Array.isArray(rawDirs) ? rawDirs.map(String) : [];
      if (!docFile || sourceDirs.length === 0) throw new Error('each target needs docFile and sourceDirs');
      return { docFile, sourceDirs };
    });
  } catch (err) {
    return emit({ error: `--targets-json is invalid: ${(err as Error).message}` }, 1, opts);
  }

  const workspacePath = args['workspace'] ? String(args['workspace']) : null;
  const artifact = args['artifact'] ? String(args['artifact']) : null;
  const result = mineDocStaleness(db, {
    targets,
    workspacePath,
    artifact,
    minEditsSinceSync: args['min_edits'] ? Number(args['min_edits']) : undefined,
    minLinesSinceSync: args['min_lines'] ? Number(args['min_lines']) : undefined,
  });

  const proposed: Array<{ target_file: string; harness_id: string }> = [];
  if (Boolean(args['propose'])) {
    const agentId = String(args['agent_id'] ?? 'agent');
    const sessionId = args['session_id'] ? String(args['session_id']) : null;
    for (const entry of result.entries) {
      if (!entry.stale) continue;
      const harnessId = proposeDocRefresh(db, entry, { agentId, sessionId, workspacePath, artifact });
      proposed.push({ target_file: entry.doc_file, harness_id: harnessId });
    }
  }

  return emit({ db_path: dbPath, ...result, proposed }, 0, opts);
}

function cmdAgentSignal(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  if (!args['agent_id']) return emit({ error: '--agent-id is required' }, 1, opts);
  const action = String(args['action'] ?? '');
  if (!['publish', 'list', 'reply', 'resolve', 'ack'].includes(action)) {
    return emit({ error: '--action must be publish, list, reply, resolve, or ack' }, 1, opts);
  }
  const rawTo = args['to_agent'] ?? args['to'];
  const toAgents = Array.isArray(rawTo) ? rawTo : rawTo ? [String(rawTo)] : [];
  const rawFiles = args['file'];
  const files = Array.isArray(rawFiles) ? rawFiles : rawFiles ? [String(rawFiles)] : [];
  const rawRefs = args['ref_id'];
  const refs = Array.isArray(rawRefs) ? rawRefs : rawRefs ? [String(rawRefs)] : [];
  const rawKinds = args['kind'];
  const kinds = Array.isArray(rawKinds) ? rawKinds : rawKinds ? [String(rawKinds)] : [];
  const compatCoerce = Boolean(args['compat_coerce']);
  const publishKind = kinds[0]
    ? normalizeNotificationKind(kinds[0], { coerce: compatCoerce })
    : undefined;
  const rawSignalIds = args['signal_id'];
  const signalIds = Array.isArray(rawSignalIds) ? rawSignalIds : rawSignalIds ? [String(rawSignalIds)] : [];
  const result = agentSignal(db, {
    action: action as import('../src/types.js').AgentSignalAction,
    agentId: String(args['agent_id']),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    kind: publishKind,
    subject: args['subject'] ? String(args['subject']) : undefined,
    body: args['body'] ? String(args['body']) : null,
    toAgents,
    files,
    refs,
    importance: args['importance'] ? parseInt(String(args['importance']), 10) : undefined,
    inReplyTo: args['in_reply_to'] ? String(args['in_reply_to']) : null,
    threadId: args['thread_id'] ? String(args['thread_id']) : null,
    signalIds,
    unreadOnly: args['all'] ? false : args['unread_only'] as boolean | undefined,
    markRead: Boolean(args['mark_read']),
    kinds: kinds.length
      ? kinds.map((k) => normalizeNotificationKind(k, { coerce: compatCoerce }))
      : [],
    compatCoerce,
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdNotifyPrune(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawIds = args['signal_id'];
  const notificationIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const result = pruneNotifications(db, {
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    notificationIds,
    resolvedOnly: Boolean(args['resolved']),
    olderThanDays: args['older_than_days'] ? parseInt(String(args['older_than_days']), 10) : undefined,
    dryRun: Boolean(args['dry_run']),
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdAgentRegistry(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const action = String(args['action'] ?? 'list');
  if (!['list', 'register'].includes(action)) {
    return emit({ error: '--action must be list or register' }, 1, opts);
  }

  const workspacePath = args['workspace'] ? String(args['workspace']) : null;
  const artifact = args['artifact'] ? String(args['artifact']) : null;

  if (action === 'register') {
    if (!args['agent_id']) return emit({ error: '--agent-id is required for register' }, 1, opts);
    const agent = registerAgent(db, {
      agentId: String(args['agent_id']),
      agentName: args['agent_name'] ? String(args['agent_name']) : '',
      workspacePath,
      artifact,
      context: args['context'] ? String(args['context']) : null,
    });
    return emit({ db_path: dbPath, action: 'register', agent }, 0, opts);
  }

  const limit = Math.min(200, Math.max(1, parseInt(String(args['limit'] ?? '50'), 10) || 50));
  const result = listAgents(db, { workspacePath, artifact });
  const agents = result.agents.slice(0, limit);
  return emit({
    db_path: dbPath,
    action: 'list',
    count: agents.length,
    total_count: result.count,
    agents,
    workspace_path: workspacePath,
    artifact,
  }, 0, opts);
}

function cmdStatus(db: DatabaseSync, dbPath: string, args: ParsedArgs, opts: EmitOptions): number {
  const rawWsPath = args['workspace'] ? String(args['workspace']) : null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact = args['artifact'] ? String(args['artifact']) : null;

  const memScope: string[] = [];
  const memScopeBinds: (string | number)[] = [];
  if (wsPath) { memScope.push('(workspace_path = ? OR workspace_path IS NULL)'); memScopeBinds.push(wsPath); }
  if (artifact) { memScope.push('(artifact = ? OR artifact IS NULL)'); memScopeBinds.push(artifact); }
  const memWhere = memScope.length > 0 ? `WHERE ${memScope.join(' AND ')}` : '';
  const memCount = (db.prepare(`SELECT COUNT(*) AS count FROM memories ${memWhere}`).get(...memScopeBinds) as { count: number }).count;
  const memStates = Object.fromEntries(
    (db.prepare(`SELECT state, COUNT(*) AS count FROM memories ${memWhere} GROUP BY state`).all(...memScopeBinds) as Array<{ state: string; count: number }>)
      .map(r => [r.state, r.count])
  );
  const memLabels = Object.fromEntries(
    (db.prepare(`SELECT COALESCE(label,'OTHER') AS label, COUNT(*) AS count FROM memories ${memWhere} GROUP BY label`).all(...memScopeBinds) as Array<{ label: string; count: number }>)
      .map(r => [r.label, r.count])
  );
  const limit = Math.min(100, Math.max(1, parseInt(String(args['limit'] ?? '20'), 10) || 20));
  const status = getWorkspaceStatus(db, { workspace_path: wsPath, artifact });

  return emit({
    db_path: dbPath,
    fts_enabled: hasFts(db),
    memory_count: memCount,
    memory_states: memStates,
    memory_labels: memLabels,
    ...status,
    locks: status.locks.slice(0, limit),
    workspace_path: wsPath,
    artifact,
  }, 0, opts);
}

function cmdInit(db: DatabaseSync, dbPath: string, opts: EmitOptions): number {
  const memCount = (db.prepare('SELECT COUNT(*) AS count FROM memories').get() as { count: number }).count;
  return emit({ db_path: dbPath, initialized: true, memory_count: memCount }, 0, opts);
}

function cmdSelfTest(opts: EmitOptions): number {
  const testDb = new DatabaseSync(':memory:');
  testDb.exec('PRAGMA foreign_keys = ON');
  initDb(testDb);

  const testAgent = 'self-test-agent';

  // Write
  const { memoryId } = insertMemory(testDb, {
    agentId: testAgent,
    taskContext: 'self-test task',
    observation: 'This is a smoke-test memory.',
    importance: 7,
    label: 'GOTCHA',
    tags: ['smoke-test'],
  });

  // Get
  const { memories: results } = getMemory(testDb, { query: 'smoke-test', limit: 5 });
  if (results.length === 0) {
    return emit({ ok: false, error: 'FTS recall returned no results' }, 1, opts);
  }

  // Reflect (direct call — no stdout patching)
  const reflectResult = reflect(testDb, {
    agentId: testAgent, task: 'self-test', outcome: 'worked', fixRepo: 'test fix',
  });

  return emit({
    ok: true,
    db: ':memory:',
    fts_enabled: hasFts(testDb),
    memory_written: memoryId,
    memory_recalled: results[0]!.memory_id,
    reflection_memory: reflectResult.learning_memory_id,
    refinement_id: reflectResult.repo_fix_refinement_id,
    checks: {
      write: Boolean(memoryId),
      fts_recall: results.length > 0,
      scoring: typeof results[0]!.score === 'number',
      refinement: Boolean(reflectResult.repo_fix_refinement_id),
    },
  }, 0, opts);
}

// ─── Help text ────────────────────────────────────────────────────────────────

const HELP = `usage: octocode-awareness <command> [options]
common: --db <path> --compact
agent map: octocode-awareness schema commands --compact
schema: octocode-awareness schema commands|list|json-schema <name>|example <name>|validate <name> <json-file|->

<AGENT_INSTRUCTIONS>
You are reading the octocode-awareness CLI. Run every command with --compact for JSON output.

BUNDLED SKILLS — install these two paths to activate the full operating loop:
  octocode-awareness : ${BUNDLED_SKILLS_DIR}/octocode-awareness
  octocode-skills    : ${BUNDLED_SKILLS_DIR}/octocode-skills

  The octocode-awareness skill contains ALL instructions for using every CLI command:
  memory, locks, planning, coordination, signals, verification, repo context, and hooks.
  Once installed, your agent reads the skill and knows exactly how and when to call this CLI.

  The octocode-skills skill teaches you how to install, manage, and create skills.
  Install both using your agent platform's skill mechanism (the octocode-skills skill guides you).
  Do not install awareness by registry name — use the bundled paths above.

FIRST COMMANDS after install:
  octocode-awareness attend --workspace "$PWD" --compact           # start packet
  octocode-awareness workspace status --compact                    # DB health
  octocode-awareness schema commands --compact                     # full command map
  octocode-awareness docs show full-flow                           # operating loop

COMMAND SURFACES:
  start:     attend, workspace status, plan list, task ready, memory recall, signal list, query <view>
  planning:  plan create|list|show|join|doc|status; task create|list|ready|show|claim|heartbeat|submit|release|depend
  edit:      lock acquire, lock wait, lock release, lock prune, verify mark, verify audit
  messages:  signal publish, signal list, signal reply, signal ack, signal resolve, signal prune, agent register, agent list
  learning:  memory record, memory forget, refinement set, refinement get, refinement delete, reflect record, reflect mine-weakness, reflect export-harness, reflect developer-review, docs list, docs show, docs staleness
  repo:      query files|workboard|all|developer-review [--format json|table|csv|markdown|html], repo inject
  hooks:     hook run <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end>, hooks install|check|remove --host claude|codex|cursor
  utility:   session capture, maintenance init, maintenance self-test, maintenance digest
</AGENT_INSTRUCTIONS>

supported agents: Codex, Claude Code, Cursor, Pi, and custom library/CLI hosts
surfaces: CLI = control plane; Agent Skill = operating loop; hooks/Pi bridge = lifecycle automation

examples:
  octocode-awareness workspace status --workspace "$PWD" --compact
  octocode-awareness attend --workspace "$PWD" --query "current task" --compact
  octocode-awareness task ready --plan-id plan_123 --compact
  octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact
  octocode-awareness docs list --compact
  octocode-awareness docs show full-flow
  octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit" --compact
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact
  octocode-awareness schema commands --compact
  octocode-awareness query files --workspace "$PWD" --format table --limit 50
  octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
  octocode-awareness repo inject --workspace "$PWD" --mode local --compact

Run "octocode-awareness <command> --help" for command flags. Exit 2 = lock conflict or wait timeout.`;

const HELP_COMPACT = `octocode-awareness: canonical noun/verb CLI. Use --compact for JSON.
bundled-skills: ${BUNDLED_SKILLS_DIR}/octocode-awareness | ${BUNDLED_SKILLS_DIR}/octocode-skills
start: attend; workspace status; plan create|list|show|join|doc|status; task create|list|ready|show|claim|heartbeat|submit|release|depend; memory recall; signal list; docs list
edit: lock acquire|wait|release|prune; verify audit|mark
msg: signal publish|list|reply|ack|resolve|prune; agent register|list
learn: memory record|forget; reflect record|mine-weakness|export-harness|developer-review; maintenance digest
repo: query files|workboard|all|developer-review --format json|table|csv|markdown|html; repo inject
inspect: schema commands --compact; docs list|show; schema json-schema <name>; <command> --help`;

const COMMAND_TO_SCHEMA: Record<string, string> = {
  'tell-memory': 'tell_memory',
  'get-memory': 'get_memory',
  'pre-flight-intent': 'pre_flight_intent',
  'wait-for-lock': 'wait_for_lock',
  'prune-stale-locks': 'prune_stale_locks',
  'release-file-lock': 'release_file_lock',
  'audit-unverified': 'audit_unverified',
  'verify': 'verify',
  'forget': 'forget_memory',
  'refine-set': 'refinement',
  'refine-get': 'refine_query',
  'refine-delete': 'refine_delete',
  'agent-registry': 'agent_registry',
  'agent-signal': 'agent_signal',
  'notify-prune': 'signal_prune',
  'status': 'workspace_status',
  'attend': 'attend',
  'export-harness': 'export_harness',
  'query': 'query',
  'repo-inject': 'repo_inject',
  'session-capture': 'session_capture',
  'mine-weakness': 'mine_weakness',
  'doc-staleness': 'doc_staleness',
  'docs-catalog': 'docs_catalog',
  'digest': 'digest',
  'reflect': 'reflect',
  'plan-command': 'plan',
  'task-command': 'task',
};

const COMMAND_DISPLAY: Record<string, string> = {
  'tell-memory': 'memory record',
  'get-memory': 'memory recall',
  'forget': 'memory forget',
  'pre-flight-intent': 'lock acquire',
  'wait-for-lock': 'lock wait',
  'prune-stale-locks': 'lock prune',
  'release-file-lock': 'lock release',
  'audit-unverified': 'verify audit',
  'verify': 'verify mark',
  'refine-set': 'refinement set',
  'refine-get': 'refinement get',
  'refine-delete': 'refinement delete',
  'agent-registry': 'agent register|list',
  'agent-signal': 'signal publish|list|reply|ack|resolve',
  'notify-prune': 'signal prune',
  'status': 'workspace status',
  'attend': 'attend',
  'export-harness': 'reflect export-harness',
  'developer-review': 'reflect developer-review',
  'query': 'query',
  'repo-inject': 'repo inject',
  'session-capture': 'session capture',
  'mine-weakness': 'reflect mine-weakness',
  'doc-staleness': 'docs staleness',
  'docs-catalog': 'docs list|show',
  'digest': 'maintenance digest',
  'init': 'maintenance init',
  'self-test': 'maintenance self-test',
  'reflect': 'reflect record',
  'plan-command': 'plan create|list|show|join|doc|status',
  'task-command': 'task create|list|ready|show|claim|heartbeat|submit|release|depend',
  'hook-run': 'hook run',
  'hooks-install': 'hooks install|check|remove',
  'schema': 'schema',
};

const COMMAND_EXAMPLE: Record<string, string> = {
  'tell-memory': 'octocode-awareness memory record --agent-id agent --task-context "build failure" --observation "Run yarn build before tests" --importance 7 --label GOTCHA --workspace "$PWD" --compact',
  'get-memory': 'octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact',
  'forget': 'octocode-awareness memory forget --memory-id mem_123 --dry-run --compact',
  'pre-flight-intent': 'octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact',
  'wait-for-lock': 'octocode-awareness lock wait --agent-id agent --target-file src/file.ts --wait-seconds 60 --compact',
  'prune-stale-locks': 'octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact',
  'release-file-lock': 'octocode-awareness lock release --agent-id agent --run-id run_123 --status SUCCESS --verified --compact',
  'audit-unverified': 'octocode-awareness verify audit --agent-id agent --workspace "$PWD" --compact',
  'verify': 'octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact',
  'refine-set': 'octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact',
  'refine-get': 'octocode-awareness refinement get --workspace "$PWD" --state open --compact',
  'refine-delete': 'octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact',
  'agent-registry': 'octocode-awareness agent register --agent-id agent --agent-name "Codex" --workspace "$PWD" --compact',
  'agent-signal': 'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact',
  'notify-prune': 'octocode-awareness signal prune --workspace "$PWD" --resolved --dry-run --compact',
  'status': 'octocode-awareness workspace status --workspace "$PWD" --compact',
  'attend': 'octocode-awareness attend --query "current task" --workspace "$PWD" --compact',
  'export-harness': 'octocode-awareness reflect export-harness --workspace "$PWD" --compact',
  'developer-review': 'octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact',
  'query': 'octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact',
  'repo-inject': 'octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact',
  'session-capture': 'octocode-awareness session capture --agent-id agent --workspace "$PWD" --reason handoff --compact',
  'mine-weakness': 'octocode-awareness reflect mine-weakness --workspace "$PWD" --compact',
  'doc-staleness': 'octocode-awareness docs staleness --targets-json \'[{"docFile":"README.md","sourceDirs":["src"]}]\' --compact',
  'docs-catalog': 'octocode-awareness docs list --compact',
  'digest': 'octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact',
  'init': 'octocode-awareness maintenance init --compact',
  'self-test': 'octocode-awareness maintenance self-test --compact',
  'reflect': 'octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep commands canonical" --compact',
  'plan-command': 'octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact',
  'task-command': 'octocode-awareness task ready --plan-id plan_123 --compact',
  'hook-run': 'octocode-awareness hook run pre-edit < hook-payload.json',
  'hooks-install': 'octocode-awareness hooks install --host codex --dry-run --compact',
  'schema': 'octocode-awareness schema commands --compact',
};

const ROUTE_EXAMPLE: Record<string, string> = {
  'signal publish': 'octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --workspace "$PWD" --compact',
  'signal list': 'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact',
  'signal reply': 'octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact',
  'signal ack': 'octocode-awareness signal ack --agent-id agent --signal-id ntf_123 --compact',
  'signal resolve': 'octocode-awareness signal resolve --agent-id agent --thread-id ntf_123 --compact',
  'agent register': 'octocode-awareness agent register --agent-id agent --agent-name "Codex" --workspace "$PWD" --compact',
  'agent list': 'octocode-awareness agent list --workspace "$PWD" --compact',
  'reflect developer-review': 'octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact',
  'docs list': 'octocode-awareness docs list --compact',
  'docs show': 'octocode-awareness docs show full-flow',
  'hooks install': 'octocode-awareness hooks install --host codex --dry-run --compact',
  'hooks check': 'octocode-awareness hooks check --host codex --strict --compact',
  'hooks remove': 'octocode-awareness hooks remove --host codex --dry-run --compact',
  'schema commands': 'octocode-awareness schema commands --compact',
  'schema list': 'octocode-awareness schema list --compact',
  'schema json-schema': 'octocode-awareness schema json-schema get_memory --compact',
  'schema example': 'octocode-awareness schema example get_memory --compact',
  'schema validate': 'octocode-awareness schema validate get_memory payload.json --compact',
};

const REMOVED_COMMAND_REPLACEMENTS: Record<string, string> = {
  'tell-memory': 'memory record',
  'get-memory': 'memory recall',
  'forget': 'memory forget',
  'memory-index': 'query memories --format markdown',
  'pre-flight-intent': 'lock acquire',
  'wait-for-lock': 'lock wait',
  'prune-stale-locks': 'lock prune',
  'release-file-lock': 'lock release',
  'audit-unverified': 'verify audit',
  'verify': 'verify mark',
  'refine-set': 'refinement set',
  'refine-get': 'refinement get',
  'refine-delete': 'refinement delete',
  'agent-registry': 'agent register|list',
  'agent-signal': 'signal publish|list|reply|ack|resolve',
  'notify': 'signal publish',
  'notify-get': 'signal list',
  'notify-resolve': 'signal resolve',
  'notify-prune': 'signal prune',
  'workspace-status': 'workspace status',
  'status': 'workspace status',
  'export-harness': 'reflect export-harness',
  'reflect': 'reflect record',
  'mine-weakness': 'reflect mine-weakness',
  'doc-staleness': 'docs staleness',
  'docs-catalog': 'docs list|show',
  'session-capture': 'session capture',
  'digest': 'maintenance digest',
  'view': 'query all --format html --out .octocode/awareness/index.html',
  'inject': 'repo inject',
  'init': 'maintenance init',
  'self-test': 'maintenance self-test',
};

const COMMAND_HELP: Record<string, string> = {
  'tell-memory': `usage: octocode-awareness memory record --agent-id <id> --task-context <text> --observation <text> --importance <1-10> [--label <l>] [--tag <t>]... [--reference <r>]... [--file <p>]...
example: octocode-awareness memory record --agent-id agent --task-context "build failure" --observation "Run yarn build before tests" --importance 7 --label GOTCHA --workspace "$PWD" --compact
note: unknown --label hard-errors unless --compat-coerce
schema: octocode-awareness schema json-schema tell_memory --compact`,
  'get-memory': `usage: octocode-awareness memory recall [options]
filters: [--query <text>] [--limit <n>] [--min-importance <n>] [--label <l>]... [--tag <t>]... [--reference <r>]... [--file <p>]... [--regex <r>]... [--file-regex <r>]...
scope: [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--strict-scope] [--global-only]
rank: [--sort smart|score|importance|recent|accessed] [--state ACTIVE|SUPERSEDED]... [--as-of <iso>] [--semantic] [--explain]
example: octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact
schema: octocode-awareness schema json-schema get_memory --compact`,
  'pre-flight-intent': `usage: octocode-awareness lock acquire --agent-id <id> --target-file <p>... [--run-id <claimed-run>] [--workspace <p>] [--artifact <a>] [--rationale <t>] [--test-plan <t>] [--lock-type EXCLUSIVE|SHARED] [--ttl-minutes <n>] [--wait-seconds <n>]
example: octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact
note: --run-id attaches file locks to a claimed task run; omit plan/task/run flags for a standalone quick-edit run
note: export OCTOCODE_AGENT_ID for CLI+hooks; --strict-agent-id / OCTOCODE_STRICT_AGENT_ID=1 hard-fails when missing
schema: octocode-awareness schema json-schema pre_flight_intent --compact`,
  'agent-signal': `usage: octocode-awareness signal publish|list|reply|ack|resolve --agent-id <id> [--to-agent <id>]... [--signal-id <id>]... [--thread-id <id>] [--kind <k>] [--subject <t>] [--body <t>] [--file <p>]...
examples:
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact
  octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --file src/file.ts --workspace "$PWD" --compact
  octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact
schema: octocode-awareness schema json-schema agent_signal --compact`,
  'verify': `usage: octocode-awareness verify mark (--run-id <id>... | --all-pending) --agent-id <id> [--status SUCCESS|FAILED] [--message <t>] [--workspace <p>] [--artifact <a>]
example: octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema verify --compact`,
  'reflect': `usage: octocode-awareness reflect record --agent-id <id> --task <text> --outcome worked|partial|failed [--lesson <t>] [--fix-repo <t>] [--fix-instructions <t>] [--fix-file <p>]... [--failure-signature <s>]
example: octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep CLI nouns canonical" --compact
note: --outcome must be worked|partial|failed (unknown hard-errors unless --compat-coerce)
note: --fix-repo → coding refinement; --fix-harness → skill/tooling; --fix-instructions → feedback to the human instruction author (see reflect developer-review)
schema: octocode-awareness schema json-schema reflect --compact`,
  'developer-review': `usage: octocode-awareness reflect developer-review [--workspace <repo>] [--state open|ongoing|done]... [--format json|markdown] [--limit <n>]
example: octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact
note: reads agent feedback on the instructions themselves (from reflect record --fix-instructions); same rows feed .octocode/DEVELOPER_REVIEW.md`,
  'query': `usage: octocode-awareness query <all|repo-profile|memories|gotchas|lessons|plans|tasks|runs|locks|agents|signals|refinements|files|activity|workboard|developer-review> [--workspace <repo>] [--format json|table|csv|markdown|html] [--out <path>]
examples:
  octocode-awareness query files --workspace "$PWD" --format table --limit 50
  octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact
  octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
note: files/memories expose missing file references as file_exists, missing_file, missing_references, and stale_file_refs workboard reasons
schema: octocode-awareness schema json-schema query --compact`,
  'attend': `usage: octocode-awareness attend [--workspace <repo>] [--query <text>] [--file <p>]... [--limit <n>] [--include-bodies] [--explain-organ]
example: octocode-awareness attend --query "current task" --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema attend --compact`,
  'repo-inject': `usage: octocode-awareness repo inject [--workspace <repo>] [--out .octocode] [--mode local|share] [--no-check] [--no-include-view]
example: octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact
schema: octocode-awareness schema json-schema repo_inject --compact`,
  'docs-catalog': `usage: octocode-awareness docs list|show [name]
examples:
  octocode-awareness docs list --compact
  octocode-awareness docs show full-flow
  octocode-awareness docs show full-flow --compact
schema: octocode-awareness schema json-schema docs_catalog --compact`,
  'plan-command': `usage: octocode-awareness plan create|list|show|join|doc|status [options]
create: --name <text> --objective <text> --lead-agent-id <id> --workspace <repo> [--artifact <name>]
show/join/doc/status: --plan-id <id>; join also --agent-id <id>; doc uses --agent-id <member> --path docs/NOTE.md --title <text>; status uses --agent-id <lead> --status DRAFT|ACTIVE|PAUSED|COMPLETED|CANCELLED
example: octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema plan --compact`,
  'task-command': `usage: octocode-awareness task create|list|ready|show|claim|heartbeat|submit|release|depend [options]
create: --plan-id <id> --title <text> --reasoning <text> --path <workspace-relative>... --agent-id <id> [--acceptance <text>] [--depends-on <task-id>]...
claim: --task-id <id> --agent-id <id>; or --next --plan-id <id> --agent-id <id>. Returns run_id for lock/submit/verify.
heartbeat/submit/release: --task-id <id> --run-id <id> --agent-id <id>; release optionally --blocked-reason <text>
example: octocode-awareness task ready --plan-id plan_123 --compact
schema: octocode-awareness schema json-schema task --compact`,
  'hook-run': `usage: octocode-awareness hook run <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json`,
  'hooks-install': hooksInstallUsage(),
  'schema': `usage: octocode-awareness schema commands|list|json-schema <name>|example <name>|validate <name> <json-file|->
examples:
  octocode-awareness schema commands --compact
  octocode-awareness schema json-schema query --compact`,
  'init': `usage: octocode-awareness maintenance init [--db <path>]
example: octocode-awareness maintenance init --db .octocode/awareness.sqlite3 --compact`,
  'self-test': `usage: octocode-awareness maintenance self-test
example: octocode-awareness maintenance self-test --compact`,
};

function hyphenFlag(flag: string): string {
  return `--${flag.replace(/_/g, '-')}`;
}

function helpFor(command: string | null, options: { compact?: boolean; routeKey?: string } = {}): string {
  if (!command) return options.compact ? HELP_COMPACT : HELP;
  const normalized = command.replace(/_/g, '-');
  const flags = KNOWN_FLAGS[normalized];
  if (!flags) return HELP;
  const schema = COMMAND_TO_SCHEMA[normalized] ?? null;
  const display = options.routeKey ?? COMMAND_DISPLAY[normalized] ?? normalized;
  const example = (options.routeKey ? ROUTE_EXAMPLE[options.routeKey] : undefined) ?? COMMAND_EXAMPLE[normalized];
  if (options.compact) {
    return [
      `usage: octocode-awareness ${display} [options]`,
      schema ? `schema: ${schema}` : 'schema: none',
      `example: ${example ?? `octocode-awareness ${display}`}`,
    ].join('\n').trimEnd();
  }
  if (COMMAND_HELP[normalized]) return COMMAND_HELP[normalized]!;
  return [
    `usage: octocode-awareness ${display} [options]`,
    `flags: ${flags.map(hyphenFlag).join(' ')}`,
    schema ? `schema: octocode-awareness schema json-schema ${schema} --compact` : 'schema: none',
    example ? `example: ${example}` : '',
  ].join('\n').trimEnd();
}

function commandFromHelpArgv(argv: string[]): { command: string | null; routeKey?: string } {
  const withoutHelp = argv.filter((arg) => arg !== '--help' && arg !== '-h' && arg !== '--compact');
  const filtered = extractGlobalDb(withoutHelp).filtered;
  const [firstRaw, secondRaw] = filtered;
  const first = normalizeToken(firstRaw);
  const second = normalizeToken(secondRaw);
  let routeKey: string | undefined;
  if (first === 'hook' && second === 'run') routeKey = 'hook run';
  else if (first === 'hooks' && second && ['install', 'check', 'remove'].includes(second)) routeKey = `hooks ${second}`;
  else if (first === 'schema' && second && ['commands', 'list', 'json-schema', 'example', 'validate'].includes(second)) routeKey = `schema ${second}`;
  else if (first && second && COMMAND_ROUTES[`${first} ${second}`]) routeKey = `${first} ${second}`;
  else if (first && SINGLE_COMMANDS.has(first)) routeKey = first;
  return { command: selectCommand(filtered).command ?? null, routeKey };
}

// ─── Entry point ──────────────────────────────────────────────────────────────

const rawArgv = process.argv.slice(2);

if (rawArgv.length === 0 || rawArgv.includes('--help') || rawArgv.includes('-h')) {
  const compactHelp = rawArgv.includes('--compact') || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
  const helpTarget = commandFromHelpArgv(rawArgv);
  process.stdout.write(helpFor(helpTarget.command, { compact: compactHelp, routeKey: helpTarget.routeKey }) + '\n');
  process.exit(0);
}

const { dbPath: globalDb, filtered: filteredArgv } = extractGlobalDb(rawArgv);
const { command, rest } = selectCommand(filteredArgv);
const args = parseArgs(rest ?? []);
if (globalDb) args['db'] = globalDb;

// Unknown flags are hard errors — a silently ignored flag reads as "it worked".
if (command && KNOWN_FLAGS[command]) {
  const unknown = validateFlags(command, args);
  if (unknown.length > 0) {
    const compactError = args['compact'] === true || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
    const payload = {
      ok: false,
      command: COMMAND_DISPLAY[command] ?? command,
      schema: COMMAND_TO_SCHEMA[command] ?? null,
      error: `unknown flag(s): ${unknown.map((f) => `--${f.replace(/_/g, '-')}`).join(', ')}`,
      known_flags: KNOWN_FLAGS[command].map((f) => `--${f.replace(/_/g, '-')}`),
      hint: `Run "octocode-awareness ${COMMAND_DISPLAY[command] ?? command} --help" for this command.`,
      example: COMMAND_EXAMPLE[command],
    };
    process.stdout.write(JSON.stringify(payload, null, compactError ? 0 : 2) + '\n');
    process.exit(1);
  }
}
if (command && command !== UNKNOWN_COMMAND) validateFlagValues(args);

const dbPath = resolveDbPath(globalDb ?? null);
const compact = args['compact'] === true || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
const opts: EmitOptions = { compact };

if (!command) {
  process.stdout.write((compact ? HELP_COMPACT : HELP) + '\n');
  process.exit(0);
}

if (command === UNKNOWN_COMMAND) {
  const requested = filteredArgv.slice(0, 2).join(' ') || filteredArgv[0] || '';
  const first = filteredArgv[0]?.replace(/_/g, '-');
  const replacement = first ? REMOVED_COMMAND_REPLACEMENTS[first] : undefined;
  const payload = {
    ok: false,
    error: `unknown command: ${requested}`,
    hint: replacement
      ? `Use canonical command: octocode-awareness ${replacement}`
      : 'Use canonical noun/verb commands only; run "octocode-awareness --help" for the command map.',
    replacement,
    examples: [
      'octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact',
      'octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit" --compact',
      'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact',
      'octocode-awareness query gotchas --workspace "$PWD" --format json --limit 20 --compact',
    ],
  };
  process.stdout.write(JSON.stringify(payload, null, compact ? 0 : 2) + '\n');
  process.exit(1);
}

if (command === 'self-test') {
  process.exit(cmdSelfTest(opts));
}

if (command === 'schema') {
  const script = packageSkillScriptPath('schema.mjs');
  const result = spawnSync(process.execPath, [script, ...rest], { stdio: 'inherit' });
  process.exit(result.status ?? 1);
}

if (command === 'hook-run') {
  // Hooks always write to the canonical store; a `--db` here was silently
  // ignored (edits would land in the real DB regardless), which is a footgun.
  // Fail loudly instead of misleading the caller.
  if (globalDb) die('hook run ignores --db: hooks always use the canonical store. Remove --db, or set OCTOCODE_MEMORY_HOME to relocate the store.');
  process.exit(await runHookCommand(String(args._[0] ?? 'help')));
}

if (command === 'hooks-install') {
  const result = runHooksInstall(rest, { hookDir: packageSkillScriptPath('hooks') });
  if (result.text !== undefined) process.stdout.write(result.text);
  else if (result.payload) emit(result.payload, result.exitCode, opts);
  process.exit(result.exitCode);
}

let db: DatabaseSync;
try {
  db = connectDb(dbPath);
} catch (err) {
  process.stderr.write(`octocode-awareness: failed to connect DB at ${dbPath}: ${String(err)}\n`);
  process.exit(1);
}

let exitCode = 0;
try {
  switch (command) {
    case 'tell-memory':    exitCode = cmdTellMemory(db, args, dbPath, opts); break;
    case 'get-memory':     exitCode = cmdGetMemory(db, args, dbPath, opts); break;
    case 'reflect':        exitCode = cmdReflect(db, args, dbPath, opts); break;
    case 'refine-set':     exitCode = cmdRefineSet(db, args, dbPath, opts); break;
    case 'refine-get':     exitCode = cmdRefineGet(db, args, dbPath, opts); break;
    case 'pre-flight-intent': exitCode = cmdPreFlightIntent(db, args, dbPath, opts); break;
    case 'release-file-lock': exitCode = cmdReleaseFileLock(db, args, dbPath, opts); break;
    case 'plan-command':   exitCode = cmdPlan(db, args, dbPath, opts); break;
    case 'task-command':   exitCode = cmdTask(db, args, dbPath, opts); break;
    case 'status':         exitCode = cmdStatus(db, dbPath, args, opts); break;
    case 'init':           exitCode = cmdInit(db, dbPath, opts); break;
    case 'prune-stale-locks': exitCode = emit({ db_path: dbPath, ...pruneStale(db, args) }, 0, opts); break;
    case 'audit-unverified':  exitCode = cmdAuditUnverified(db, args, dbPath, opts); break;
    case 'verify':             exitCode = cmdVerify(db, args, dbPath, opts); break;
    case 'session-capture': exitCode = emit({
      db_path: dbPath,
      ...sessionCapture(db, {
        agent_id: args['agent_id'],
        workspace: args['workspace'],
        artifact: args['artifact'],
        repo: args['repo'],
        ref: args['ref'],
        reason: args['reason'],
        cwd: args['cwd'],
      }),
    }, 0, opts); break;
    case 'mine-weakness': {
      const mwParams = {
        agentId:       args['agent_id'] as string | undefined,
        workspacePath: args['workspace'] as string | undefined,
        artifact:      args['artifact'] as string | undefined,
        minCount:      args['min_count'] ? Number(args['min_count']) : undefined,
        limit:         args['limit']     ? Number(args['limit'])     : undefined,
        cwd:           args['cwd']       as string | undefined,
      };
      exitCode = emit({ db_path: dbPath, ...mineWeakness(db, mwParams) }, 0, opts);
      break;
    }
    case 'doc-staleness': exitCode = cmdDocStaleness(db, args, dbPath, opts); break;
    case 'docs-catalog': exitCode = cmdDocsCatalog(db, args, dbPath, opts); break;
    case 'digest': {
      const retDays = args['retention_days'] ? Number(args['retention_days']) : undefined;
      const handoffDays = args['refinement_handoff_retention_days'] ? Number(args['refinement_handoff_retention_days']) : undefined;
      const doneDays = args['refinement_done_retention_days'] ? Number(args['refinement_done_retention_days']) : undefined;
      const isDryRun = Boolean(args['dry_run'] ?? args['dry-run']);
      const digestResult = digest(db, {
        ...(retDays !== undefined ? { retention_days: retDays } : {}),
        ...(handoffDays !== undefined ? { refinement_handoff_retention_days: handoffDays } : {}),
        ...(doneDays !== undefined ? { refinement_done_retention_days: doneDays } : {}),
        ...(args['workspace'] ? { workspace: String(args['workspace']) } : {}),
        ...(args['artifact'] ? { artifact: String(args['artifact']) } : {}),
        ...(isDryRun ? { dry_run: true } : {}),
      });
      const payload: Record<string, unknown> = { db_path: dbPath, ...digestResult };
      if (!isDryRun && (args['export_doc'] ?? args['export-doc'])) {
        try {
          const wsPath = (args['workspace'] as string | undefined) ?? process.cwd();
          const artifact = args['artifact'] as string | undefined;
          const { mkdirSync, writeFileSync } = await import('node:fs');
          const { join } = await import('node:path');
          const docDir = join(wsPath, '.octocode', 'memory-reports');
          mkdirSync(docDir, { recursive: true });
          const dateStr = new Date().toISOString().slice(0, 16).replace('T', '-').replace(':', '');
          const docPath = (typeof (args['export_doc'] ?? args['export-doc']) === 'string'
            ? args['export_doc'] ?? args['export-doc']
            : join(docDir, `memory-report-${dateStr}.md`)) as string;
          writeFileSync(docPath, exportMemoryDoc(db, { workspace_path: wsPath, artifact }), 'utf8');
          payload['doc_path'] = docPath;
        } catch (err) {
          payload['doc_warning'] = `Could not write doc: ${(err as Error).message}`;
        }
      }
      exitCode = emit(payload, 0, opts);
      break;
    }
    case 'wait-for-lock': {
      const rawWaitTarget = args['target_file'] ?? args['file'];
      const waitTargets = Array.isArray(rawWaitTarget) ? rawWaitTarget : rawWaitTarget ? [String(rawWaitTarget)] : [];
      const waitSecs = parseBoundedSeconds(args, 'wait_seconds', 0, MAX_CLI_WAIT_SECONDS);
      const retrySecs = parseBoundedSeconds(args, 'retry_interval', 1, MAX_CLI_RETRY_INTERVAL_SECONDS);
      const waitResult = waitForLock(db, {
        agent_id: args['agent_id'],
        target_files: waitTargets,
        workspace: args['workspace'],
        artifact: args['artifact'],
        lock_type: args['lock_type'],
        wait_ms: waitSecs != null ? waitSecs * 1000 : undefined,
        retry_interval_ms: retrySecs != null ? retrySecs * 1000 : undefined,
      });
      exitCode = emit({ db_path: dbPath, ...waitResult }, waitResult.lock_free ? 0 : 2, opts);
      break;
    }
    case 'forget':          exitCode = cmdForget(db, args, dbPath, opts); break;
    case 'refine-delete':   exitCode = cmdRefineDelete(db, args, dbPath, opts); break;
    case 'export-harness':  exitCode = cmdExportHarness(db, args, dbPath, opts); break;
    case 'developer-review': exitCode = cmdDeveloperReview(db, args, dbPath, opts); break;
    case 'query':           exitCode = cmdQuery(db, args, dbPath, opts); break;
    case 'attend':          exitCode = cmdAttend(db, args, dbPath, opts); break;
    case 'repo-inject':     exitCode = cmdRepoInject(db, args, dbPath, opts); break;
    case 'agent-registry':  exitCode = cmdAgentRegistry(db, args, dbPath, opts); break;
    case 'agent-signal': {
      const signalFormat = String(args['format'] ?? 'json');
      if (args['action'] === 'list' && signalFormat === 'hook') {
        const signalBriefing = notifyGet(db, {
          workspace: args['workspace'] as string | undefined,
          artifact: args['artifact'] as string | undefined,
          format: signalFormat,
          agent_id: args['agent_id'] as string | undefined,
        }) as unknown as Record<string, unknown>;
        exitCode = signalBriefing['additionalContext']
          ? emit({ additionalContext: signalBriefing['additionalContext'] }, 0, opts)
          : emit({ db_path: dbPath, ...signalBriefing }, 0, opts);
      } else {
        exitCode = cmdAgentSignal(db, args, dbPath, opts);
      }
      break;
    }
    case 'notify-prune':    exitCode = cmdNotifyPrune(db, args, dbPath, opts); break;
    default:
      exitCode = emit({ error: `unknown command: ${command}. Run --help for usage.` }, 1, opts);
  }
} catch (err) {
  exitCode = emit({
    error: err instanceof Error ? err.message : String(err),
  }, 1, opts);
}

process.exit(exitCode);
