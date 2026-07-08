/**
 * awareness.ts — CLI entry point for @octocodeai/octocode-awareness.
 *
 * Thin wrapper: parse args → call domain functions → emit JSON.
 * Compiled to dist/bin/awareness.js by build.mjs.
 */

import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

import {
  connectDb, initDb, hasFts, resolveDbPath, evictExpiredLocks,
} from '../src/db.js';
import { insertMemory, getMemory, mineWeakness, forgetMemory } from '../src/memory.js';
import { mineDocStaleness, proposeDocRefresh } from '../src/docs.js';
import { insertRefinement, getRefinements, updateRefinement, deleteRefinement } from '../src/refinements.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { reflect } from '../src/reflect.js';
import type { EvalFailure } from '../src/types.js';
import { pruneStale, notifyGet, sessionCapture, waitForLock, digest, getWorkspaceStatus, exportMemoryDoc, exportHarness } from '../src/maintenance.js';
import { insertNotification, getNotifications, resolveNotification, pruneNotifications, agentSignal } from '../src/notifications.js';
import { auditUnverified, markVerified } from '../src/verify.js';
import { registerAgent, listAgents } from '../src/agents.js';
import {
  normalizeLabel,
} from '../src/helpers.js';

// ─── Arg parser ───────────────────────────────────────────────────────────────

type ArgValue = string | boolean | string[];
type ParsedArgs = Record<string, ArgValue> & { _: string[] };

const ARRAY_FLAGS = new Set([
  'tag', 'tags', 'reference', 'file', 'fix_file', 'target_file', 'supersedes', 'label', 'state',
  'memory_id', 'refinement_id', 'signal_id', 'ref_id', 'regex', 'file_regex',
  'to_agent', 'kind',
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
const KNOWN_FLAGS: Record<string, string[]> = {
  'tell-memory': ['agent_id', 'task_context', 'observation', 'importance', 'label', 'tag', 'reference', 'supersedes', 'failure_signature', 'valid_from', 'valid_to', 'workspace', 'artifact', 'repo', 'ref', 'file', 'file_tree_fingerprint'],
  'get-memory': ['query', 'limit', 'min_importance', 'label', 'tag', 'smart', 'workspace', 'artifact', 'repo', 'ref', 'state', 'sort', 'global_only', 'strict_scope', 'as_of', 'reference', 'regex', 'file_regex', 'file', 'explain', 'semantic'],
  'forget': ['memory_id', 'tag', 'tags', 'before', 'max_importance', 'dry_run'],
  'reflect': ['agent_id', 'task', 'outcome', 'lesson', 'worked', 'didnt_work', 'fix_repo', 'fix_file', 'fix_harness', 'failure_signature', 'importance', 'judgment_note', 'duo', 'eval_failure_json', 'workspace', 'artifact', 'repo', 'ref'],
  'refine-set': ['agent_id', 'reasoning', 'remember', 'quality', 'state', 'workspace', 'artifact', 'repo', 'ref', 'file', 'refinement_id'],
  'refine-get': ['workspace', 'artifact', 'repo', 'ref', 'quality', 'include_handoffs', 'state', 'limit'],
  'refine-delete': ['refinement_id', 'workspace', 'artifact', 'dry_run'],
  'pre-flight-intent': ['agent_id', 'workspace', 'artifact', 'rationale', 'test_plan', 'plan_doc_ref', 'target_file', 'file', 'lock_type', 'ttl_minutes', 'ttl_seconds', 'wait_seconds'],
  'release-file-lock': ['agent_id', 'task_id', 'target_file', 'file', 'status', 'verified', 'verified_note', 'workspace', 'artifact'],
  'status': ['workspace', 'artifact', 'limit'],
  'workspace-status': ['workspace', 'artifact'],
  'init': [],
  'self-test': [],
  'prune-stale-locks': ['older_than_minutes', 'expired_only', 'agent_id', 'target_file', 'workspace', 'artifact', 'dry_run'],
  'audit-unverified': ['agent_id', 'workspace', 'artifact', 'abandon'],
  'verify': ['task_id', 'all_pending', 'agent_id', 'status', 'message', 'workspace', 'artifact'],
  'mine-weakness': ['agent_id', 'workspace', 'artifact', 'min_count', 'limit', 'cwd'],
  'doc-staleness': ['agent_id', 'workspace', 'artifact', 'targets_json', 'min_edits', 'min_lines', 'propose', 'session_id'],
  'export-harness': ['limit', 'min_importance', 'workspace', 'artifact'],
  'memory-index': ['limit', 'min_importance', 'out', 'stdout', 'workspace', 'artifact', 'repo', 'ref'],
  'agent-registry': ['action', 'agent_id', 'agent_name', 'workspace', 'artifact', 'context', 'limit'],
  'notify': ['agent_id', 'to', 'kind', 'subject', 'body', 'file', 'ref_id', 'in_reply_to', 'importance', 'workspace', 'artifact', 'repo', 'ref'],
  'agent-signal': ['action', 'agent_id', 'workspace', 'artifact', 'repo', 'ref', 'kind', 'subject', 'body', 'to_agent', 'file', 'ref_id', 'importance', 'in_reply_to', 'thread_id', 'signal_id', 'all', 'mark_read', 'limit'],
  'notify-get': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'all', 'mark_read', 'kind', 'thread_id', 'limit', 'format'],
  'notify-resolve': ['signal_id', 'thread_id', 'workspace', 'artifact'],
  'notify-prune': ['signal_id', 'resolved', 'older_than_days', 'dry_run', 'workspace', 'artifact'],
  'session-capture': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'reason', 'cwd'],
  'wait-for-lock': ['agent_id', 'target_file', 'file', 'workspace', 'artifact', 'lock_type', 'wait_seconds', 'retry_interval'],
  'digest': ['retention_days', 'dry_run', 'export_doc', 'workspace', 'artifact'],
};

function validateFlags(command: string, args: ParsedArgs): string[] {
  const known = KNOWN_FLAGS[command];
  if (!known) return [];
  const allowed = new Set([...known, ...GLOBAL_FLAGS]);
  return Object.keys(args).filter((k) => k !== '_' && !allowed.has(k));
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

// ─── Output ───────────────────────────────────────────────────────────────────

interface EmitOptions { compact?: boolean }

function emit(payload: Record<string, unknown>, exitCode = 0, opts: EmitOptions = {}): number {
  payload['ok'] = payload['ok'] ?? (exitCode === 0);
  const compact = opts.compact === true || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
  process.stdout.write((compact ? JSON.stringify(payload) : JSON.stringify(payload, null, 2)) + '\n');
  return exitCode;
}

function die(message: string, extras: Record<string, unknown> = {}): never {
  process.stdout.write(JSON.stringify({ ok: false, error: message, ...extras }, null, 2) + '\n');
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
  const rawSup = args['supersedes'];
  const supersedes = Array.isArray(rawSup) ? rawSup : rawSup ? [String(rawSup)] : [];
  const rawLabel = args['label'];
  const label = Array.isArray(rawLabel) ? rawLabel[0] : String(rawLabel ?? '');

  const { memory, superseded, noveltyScore, similarMemoryIds } = insertMemory(db, {
    agentId, taskContext, observation, importance: imp,
    label: normalizeLabel(label),
    tags, references, supersedes,
    failureSignature: args['failure_signature'] ? String(args['failure_signature']) : null,
    validFrom: args['valid_from'] ? String(args['valid_from']) : null,
    validTo: args['valid_to'] ? String(args['valid_to']) : null,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    fileTreeFingerprint: args['file_tree_fingerprint'] ? String(args['file_tree_fingerprint']) : null,
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
    sort: String(args['sort'] ?? 'smart'),
    globalOnly: Boolean(args['global_only']),
    strictScope: Boolean(args['strict_scope']),
    asOf: args['as_of'] ? String(args['as_of']) : null,
    references,
    regex,
    fileRegex,
    files: getFiles,
    explain: Boolean(args['explain']),
  });

  // The CLI has no embedding source; semantic ranking needs embeddings stored
  // via the library API (storeEmbedding/semanticSearch). Be honest about it.
  const payload: Record<string, unknown> = { db_path: dbPath, ...result };
  if (args['semantic']) {
    payload['warnings'] = [
      'semantic ranking is unavailable in the CLI (no embedding source); results use lexical FTS + decay. Use the library storeEmbedding()/semanticSearch() API for semantic recall.',
    ];
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
      ...(args['quality'] !== undefined ? { quality: String(args['quality']) as 'good' | 'bad' | 'handoff' } : {}),
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
    quality: (String(args['quality'] ?? 'good')) as 'good' | 'bad' | 'handoff',
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
    quality: args['quality'] ? String(args['quality']) as 'good' | 'bad' | 'handoff' : undefined,
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

  const result = reflect(db, {
    agentId: String(args['agent_id'] ?? 'agent'),
    task: String(args['task']),
    outcome: String(args['outcome'] ?? 'partial') as 'worked' | 'partial' | 'failed',
    lesson: args['lesson'] ? String(args['lesson']) : null,
    worked: args['worked'] ? String(args['worked']) : null,
    didntWork: args['didnt_work'] ? String(args['didnt_work']) : null,
    fixRepo: args['fix_repo'] ? String(args['fix_repo']) : null,
    fixHarness: args['fix_harness'] ? String(args['fix_harness']) : null,
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
  });

  return emit({ ...result, db_path: dbPath }, 0, opts);
}

function cmdPreFlightIntent(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawTarget = args['target_file'] ?? args['file'];
  const targetFiles = Array.isArray(rawTarget) ? rawTarget : rawTarget ? [String(rawTarget)] : [];
  const ttlMinutes = args['ttl_minutes'] ? parseInt(String(args['ttl_minutes']), 10) : null;
  const ttlSeconds = args['ttl_seconds'] ? parseInt(String(args['ttl_seconds']), 10) : null;
  if (ttlMinutes != null && (!Number.isInteger(ttlMinutes) || ttlMinutes < 1)) die('--ttl-minutes must be >= 1');
  if (ttlSeconds != null && (!Number.isInteger(ttlSeconds) || ttlSeconds < 1)) die('--ttl-seconds must be >= 1');
  const ttlMs = ttlMinutes != null ? ttlMinutes * 60000 : ttlSeconds != null ? ttlSeconds * 1000 : null;

  const claimParams = {
    agentId: String(args['agent_id'] ?? 'agent'),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    rationale: String(args['rationale'] ?? 'agent write operation'),
    testPlan: String(args['test_plan'] ?? 'post-edit verification'),
    targetFiles,
    lockType: (String(args['lock_type'] ?? 'EXCLUSIVE')) as 'EXCLUSIVE' | 'SHARED',
    ttlMs,
  };
  let result = preFlightIntent(db, claimParams);

  // --wait-seconds: bounded wait for the current holder, then claim.
  // waitForLock sleeps outside SQLite transactions; a small window between
  // "clear" and the claim is inherent — the re-claim below closes it or conflicts again.
  const waitSeconds = args['wait_seconds'] ? parseInt(String(args['wait_seconds']), 10) : null;
  if (!result.ok && waitSeconds != null && waitSeconds > 0) {
    const wait = waitForLock(db, {
      agent_id: claimParams.agentId,
      target_files: targetFiles,
      workspace: claimParams.workspacePath ?? undefined,
      artifact: claimParams.artifact ?? undefined,
      lock_type: claimParams.lockType,
      wait_ms: waitSeconds * 1000,
    });
    if (wait.lock_free) result = preFlightIntent(db, claimParams);
  }

  if (!result.ok) return emit({ db_path: dbPath, ...result }, 2, opts);
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdAuditUnverified(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const result = auditUnverified(db, {
    agentId: args['agent_id'] ? String(args['agent_id']) : null,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    abandon: Boolean(args['abandon']),
  });
  return emit({ db_path: dbPath, ...result }, result.count > 0 ? 1 : 0, opts);
}

function cmdVerify(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const allPending = Boolean(args['all_pending']);
  if (!allPending && !args['task_id']) {
    return emit({ error: '--task-id is required (or use --all-pending)' }, 1, opts);
  }
  const statusArg = args['status'] ? String(args['status']) : 'SUCCESS';
  if (statusArg !== 'SUCCESS' && statusArg !== 'FAILED') {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts);
  }
  const result = markVerified(db, {
    taskId: args['task_id'] ? String(args['task_id']) : undefined,
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

  if (!args['task_id'] && targetFiles.length === 0) {
    return emit({ error: 'release-file-lock requires --task-id or --target-file' }, 1, opts);
  }

  const result = releaseFileLock(db, {
    agentId: String(args['agent_id'] ?? 'agent'),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    taskId: args['task_id'] ? String(args['task_id']) : null,
    targetFiles,
    status: (String(args['status'] ?? 'SUCCESS')) as 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED',
    verified: Boolean(args['verified']),
    verifiedNote: args['verified_note'] ? String(args['verified_note']) : undefined,
  });

  // When release succeeded but verification is still pending, signal this clearly:
  // ok:false + exit 2 so agents don't interpret the release as fully complete and
  // then get unexpectedly blocked by stop-verify at session end.
  if ('unverifiedConclusion' in result) {
    return emit({ db_path: dbPath, ...result, ok: false }, 2, opts);
  }
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdMemoryIndex(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const limit = args['limit'] ? parseInt(String(args['limit']), 10) : 30;
  const minImportance = args['min_importance'] ? parseInt(String(args['min_importance']), 10) : 1;
  const stdout = Boolean(args['stdout']);

  // Query top memories by importance + access
  const wsPath = args['workspace'] ? String(args['workspace']) : null;
  const conds: (string | number)[] = [];
  const binds: (string | number)[] = [minImportance];
  let sql = `SELECT memory_id, label, importance, task_context, observation, tags_json, created_at
     FROM memories WHERE state = 'ACTIVE' AND importance >= ?`;
  if (wsPath) { sql += ' AND (workspace_path = ? OR workspace_path IS NULL)'; binds.push(wsPath); }
  if (args['artifact']) { sql += ' AND (artifact = ? OR artifact IS NULL)'; binds.push(String(args['artifact'])); }
  if (args['repo']) { sql += ' AND (repo = ? OR repo IS NULL)'; binds.push(String(args['repo'])); }
  if (args['ref']) { sql += ' AND (ref = ? OR ref IS NULL)'; binds.push(String(args['ref'])); }
  sql += ' ORDER BY importance DESC, access_count DESC, last_accessed_at DESC LIMIT ?';
  binds.push(limit);
  void conds;

  type MemRow = { memory_id: string; label: string; importance: number; task_context: string; observation: string; tags_json: string; created_at: string };
  const rows = db.prepare(sql).all(...binds) as unknown as MemRow[];

  const now = new Date().toISOString().slice(0, 10);
  const lines = [
    `# Memory Index — ${now}`,
    `<!-- Auto-generated by awareness memory-index. Regenerate after recording or forgetting memories. -->`,
    '',
    `**${rows.length} active memories** (importance ≥ ${minImportance}, sorted by salience)`,
    '',
  ];
  for (const m of rows) {
    const tags = (() => { try { return (JSON.parse(m.tags_json) as string[]).join(', '); } catch { return ''; } })();
    lines.push(`## [${m.label}:${m.importance}] ${m.task_context.slice(0, 80)}`);
    lines.push(`> ${m.observation.slice(0, 200)}`);
    if (tags) lines.push(`*Tags: ${tags}*`);
    lines.push('');
  }

  const content = lines.join('\n');

  if (stdout) {
    process.stdout.write(content + '\n');
    return 0;
  }

  const outPath = args['out'] ? String(args['out']) : null;
  const targetPath = outPath ?? (resolveDbPath(null).replace('awareness.sqlite3', 'MEMORY.md'));
  try {
    mkdirSync(dirname(targetPath), { recursive: true });
    writeFileSync(targetPath, content, 'utf8');
  } catch (err) {
    return emit({ db_path: dbPath, error: `Could not write MEMORY.md: ${(err as Error).message}` }, 1, opts);
  }

  return emit({ db_path: dbPath, ok: true, path: targetPath, count: rows.length }, 0, opts);
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

function cmdNotify(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  if (!args['agent_id']) return emit({ error: '--agent-id is required' }, 1, opts);
  if (!args['kind']) return emit({ error: '--kind is required' }, 1, opts);
  if (!args['subject']) return emit({ error: '--subject is required' }, 1, opts);
  const rawFiles = args['file'];
  const files = Array.isArray(rawFiles) ? rawFiles : rawFiles ? [String(rawFiles)] : [];
  const rawRefIds = args['ref_id'];
  const refIds = Array.isArray(rawRefIds) ? rawRefIds : rawRefIds ? [String(rawRefIds)] : [];
  const result = insertNotification(db, {
    agentId: String(args['agent_id']),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    toAgent: args['to'] ? String(args['to']) : null,
    kind: String(args['kind']) as import('../src/types.js').NotificationKind,
    subject: String(args['subject']),
    body: args['body'] ? String(args['body']) : null,
    files,
    refIds,
    inReplyTo: args['in_reply_to'] ? String(args['in_reply_to']) : null,
    importance: args['importance'] ? parseInt(String(args['importance']), 10) : 5,
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdNotifyGet(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  if (!args['agent_id']) return emit({ error: '--agent-id is required' }, 1, opts);
  const rawKinds = args['kind'];
  const kinds = Array.isArray(rawKinds) ? rawKinds : rawKinds ? [String(rawKinds)] : [];
  const result = getNotifications(db, {
    agentId: String(args['agent_id']),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    kinds: kinds as import('../src/types.js').NotificationKind[],
    threadId: args['thread_id'] ? String(args['thread_id']) : null,
    unreadOnly: args['all'] ? false : true,
    markRead: Boolean(args['mark_read']),
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : 20,
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdNotifyResolve(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawIds = args['signal_id'];
  const notificationIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const result = resolveNotification(db, {
    notificationIds,
    threadId: args['thread_id'] ? String(args['thread_id']) : null,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
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
  const publishKind = kinds[0] as import('../src/types.js').NotificationKind | undefined;
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
    kinds: kinds as import('../src/types.js').NotificationKind[],
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
  // Use the canonical evictExpiredLocks (<=) instead of duplicating the DELETE with < (off by one).
  evictExpiredLocks(db);
  const wsPath = args['workspace'] ? String(args['workspace']) : null;
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
  const taskScope: string[] = ["status='ACTIVE'"];
  const taskBinds: (string | number)[] = [];
  if (wsPath) { taskScope.push('workspace_path = ?'); taskBinds.push(wsPath); }
  if (artifact) { taskScope.push('(artifact = ? OR artifact IS NULL)'); taskBinds.push(artifact); }
  const activeTasks = (db.prepare(`SELECT COUNT(*) AS count FROM tasks WHERE ${taskScope.join(' AND ')}`).get(...taskBinds) as { count: number }).count;
  const limit = Math.min(100, Math.max(1, parseInt(String(args['limit'] ?? '20'), 10) || 20));
  const lockWhere: string[] = [];
  const lockBinds: (string | number)[] = [];
  if (wsPath) { lockWhere.push('ai.workspace_path = ?'); lockBinds.push(wsPath); }
  if (artifact) { lockWhere.push('(ai.artifact = ? OR ai.artifact IS NULL)'); lockBinds.push(artifact); }
  const locks = db.prepare(
    `SELECT fl.file_path, fl.task_id, ai.agent_id, ai.workspace_path, ai.artifact, fl.lock_type, fl.acquired_at, fl.expires_at
       FROM locks fl
       JOIN tasks ai ON ai.task_id = fl.task_id
       ${lockWhere.length > 0 ? `WHERE ${lockWhere.join(' AND ')}` : ''}
       ORDER BY fl.acquired_at DESC LIMIT ?`
  ).all(...lockBinds, limit);
  const openRefinements = (db.prepare(
    `SELECT COUNT(*) AS count FROM refinements
      WHERE state IN ('open','ongoing')
      ${wsPath ? 'AND (workspace_path = ? OR workspace_path IS NULL)' : ''}
      ${artifact ? 'AND (artifact = ? OR artifact IS NULL)' : ''}`
  ).get(...[...(wsPath ? [wsPath] : []), ...(artifact ? [artifact] : [])]) as { count: number }).count;

  return emit({
    db_path: dbPath,
    fts_enabled: hasFts(db),
    memory_count: memCount,
    memory_states: memStates,
    memory_labels: memLabels,
    active_task_count: activeTasks,
    open_refinements: openRefinements,
    locks,
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

const HELP = `usage: awareness <command> [options]

commands: tell-memory  get-memory  forget  reflect  refine-set  refine-get  refine-delete
          pre-flight-intent  release-file-lock  status  workspace-status  init  self-test
          prune-stale-locks  audit-unverified  verify  mine-weakness  doc-staleness  export-harness  memory-index
          agent-registry  notify  agent-signal  notify-get  notify-resolve  notify-prune  session-capture  wait-for-lock  digest

common options:
  --db <path>     Override DB path (default: $OCTOCODE_MEMORY_HOME/awareness.sqlite3)
  --compact       Compact JSON output (or OCTOCODE_AWARENESS_COMPACT=1)

tell-memory:
  --agent-id <id>  --task-context <text>  --observation <text>
  --importance <1-10>  --label <LABEL>  [--tag <t>]...  [--reference <r>]...

get-memory:
  --query <text>  [--limit <n>]  [--min-importance <n>]  [--label <L>]  [--smart]
  [--reference <r>]...  [--regex <pattern>]...  [--file-regex <pat>]...  [--file <path>]...
  [--sort smart|importance|recent|accessed]  [--state ACTIVE|SUPERSEDED]...
  [--strict-scope]  [--global-only]  [--as-of <ISO>]  [--explain]
  --explain: attach per-result score_components (importance/recency/access/relevance)

forget:
  [--memory-id <id>]...  [--tag <t>]...  [--before <ISO>]  [--max-importance <n>]  [--dry-run]

refine-delete:
  --refinement-id <id>...  [--workspace <path>]  [--dry-run]

export-harness:
  [--limit <n>]  [--min-importance <n>]  [--workspace <path>]
  preview top lessons as an AGENTS.md block

agent-registry:
  [--action list|register]  [--workspace <path>]  [--artifact <name>]  [--limit <n>]
  register: --agent-id <id>  [--agent-name <name>]  [--context <host>]
  list known agents from the same SQLite store, ordered by last_seen_at

notify:
  --agent-id <id>  --kind claim|handoff|question|reply|blocker|request|decision|fyi
  --subject <text>  [--to <agent-id>]  [--body <text>]  [--file <path>]...
  [--ref-id <id>]...  [--in-reply-to <signal-id>]  [--importance <1-10>]

agent-signal:
  --action publish|list|reply|resolve|ack  --agent-id <id>
  publish: --kind claim|handoff|question|reply|blocker|request|decision|fyi  --subject <text>
           [--to-agent <id>]...  [--body <text>]  [--file <path>]...  [--ref-id <id>]...
           [--in-reply-to <signal-id>]  [--importance <1-10>]
  reply:   same as publish, plus --in-reply-to <signal-id> (thread_id is inherited from the parent)
  list:    [--all]  [--kind <k>]...  [--thread-id <id>]  [--mark-read]  [--limit <n>]
           inbox = unread signals addressed to --agent-id, plus broadcasts where to_agent is unset
  ack:     --signal-id <id>...  idempotent per-agent read receipt (shares signal_reads with --mark-read)
  resolve: [--signal-id <id>]...  [--thread-id <id>]
  [--workspace <path>]  [--artifact <name>]  [--repo <r>]  [--ref <r>]
  generated signal ids use an "ntf_" prefix

notify-get:
  [--agent-id <id>]  [--workspace <path>]  [--artifact <name>]  [--all]  [--mark-read]
  [--kind <k>]  [--thread-id <id>]  [--limit <n>]  [--format json|hook]
  with --agent-id and --format json (default): this agent's real inbox
  without --agent-id, or with --format hook: smart-briefing payload used by hooks

notify-resolve:
  [--signal-id <id>]...  [--thread-id <id>]

notify-prune:
  [--signal-id <id>]...  [--resolved]  [--older-than-days <n>]  [--dry-run]

session-capture:
  [--agent-id <id>]  [--workspace <path>]  [--artifact <name>]  [--repo <r>]  [--ref <r>]
  [--reason <text>]  [--cwd <path>]
  writes a work-handoff refinement summarizing this agent's active locks and dirty git tree

wait-for-lock:
  --agent-id <id>  --target-file <path>...  [--workspace <path>]  [--artifact <name>]
  [--lock-type EXCLUSIVE|SHARED]  [--wait-seconds <n>]  [--retry-interval <n>]
  polls until the target file(s) are lock-free or --wait-seconds elapses
  exits 0 when lock_free; exits 2 on timeout with conflicts[]

reflect:
  --agent-id <id>  --task <text>  --outcome worked|partial|failed
  [--lesson <text>]  [--worked <text>]  [--didnt-work <text>]
  [--fix-repo <text>]  [--fix-file <path>]...  [--fix-harness <text>]
  [--failure-signature <sig>]  [--importance <1-10>]  [--judgment-note <text>]
  [--duo]  [--eval-failure-json '<[{id,dimension?,failure_signature?,suggested_lesson?}]>']
  --duo emits an advisory reflection_duo packet (not stored); eval failures
  become eval-tagged memories clustered by failure_signature

refine-set:
  new:    --agent-id <id> --reasoning <text> --remember <text>
          [--quality good|bad|handoff]  [--state open|ongoing|done]  [--file <path>]...
  update: --refinement-id <id> plus only the flags to change (e.g. --state done)

refine-get:
  [--state open|ongoing|done]...  [--quality good|bad|handoff]  [--include-handoffs]
  session handoffs are hidden unless --include-handoffs or --quality handoff is passed

prune-stale-locks:
  [--older-than-minutes <n>]  [--expired-only]  [--agent-id <id>]
  [--target-file <path>]...  [--dry-run]
  expired locks always qualify; --older-than-minutes also catches old live locks

workspace-status:
  [--workspace <path>] [--artifact <name>]   show active locks, agent tasks, and memory counts

mine-weakness:
  [--agent-id <id>]  [--workspace <path>]  [--min-count <n>]  [--limit <n>]
  find recurring failure patterns grouped by failure_signature

doc-staleness:
  --targets-json '<[{"docFile":"pkg/ARCHITECTURE.md","sourceDirs":["pkg/src"]}]>'
  [--workspace <path>]  [--min-edits <n>]  [--min-lines <n>]
  [--propose]  [--agent-id <id>]  [--session-id <id>]
  compares edit_log activity under sourceDirs against docFile's own last edit_log
  timestamp; --propose records a harness_log 'propose' event (failure_signature
  'doc-staleness') for each stale entry

digest:
  [--retention-days <n>]  [--dry-run]  [--export-doc [path]]
  archive expired memories, prune old superseded rows/refinements, rebuild FTS
  --dry-run: preview counts without mutating anything
  --export-doc: write a markdown memory report to .octocode/memory-reports/

pre-flight-intent:
  --agent-id <id>  --target-file <path>...  [--workspace <path>]  [--artifact <name>]
  [--rationale <text>]  [--test-plan <text>]  [--lock-type EXCLUSIVE|SHARED]
  [--ttl-minutes <n> | --ttl-seconds <n>]  [--wait-seconds <n>]
  claims target file(s); lock TTL is hard-capped at 10 minutes regardless of the requested value
  --wait-seconds: on conflict, wait up to <n>s for the current holder to release, then retry once

release-file-lock:
  --agent-id <id>  (--task-id <id> | --target-file <path>)  [--status SUCCESS|PENDING|FAILED]
  [--verified]  [--verified-note <text>]

audit-unverified:
  [--agent-id <id>]  [--workspace <path>]  [--artifact <name>]  [--abandon]
  exits 1 when unverified (PENDING) tasks exist; exits 0 when clear
  --abandon: dismiss all PENDING tasks as FAILED (clear orphaned sessions)

verify:
  (--task-id <id> | --all-pending)  --agent-id <id>
  [--status SUCCESS|FAILED]  [--message <text>]  [--workspace <path>]  [--artifact <name>]
  marks a PENDING task as verified; --all-pending clears every PENDING task for this agent
`;

// ─── Entry point ──────────────────────────────────────────────────────────────

const rawArgv = process.argv.slice(2);

if (rawArgv.length === 0 || rawArgv.includes('--help') || rawArgv.includes('-h')) {
  process.stdout.write(HELP + '\n');
  process.exit(0);
}

const { dbPath: globalDb, filtered: filteredArgv } = extractGlobalDb(rawArgv);
const [rawCommand, ...rest] = filteredArgv;
// Accept protocol-style underscore aliases (tell_memory → tell-memory).
const command = rawCommand?.replace(/_/g, '-');
const args = parseArgs(rest ?? []);
if (globalDb) args['db'] = globalDb;

// Unknown flags are hard errors — a silently ignored flag reads as "it worked".
if (command && KNOWN_FLAGS[command]) {
  const unknown = validateFlags(command, args);
  if (unknown.length > 0) {
    process.stdout.write(JSON.stringify({
      ok: false,
      error: `unknown flag(s) for ${command}: ${unknown.map((f) => `--${f.replace(/_/g, '-')}`).join(', ')}`,
      known_flags: KNOWN_FLAGS[command].map((f) => `--${f.replace(/_/g, '-')}`),
    }, null, 2) + '\n');
    process.exit(1);
  }
}

const dbPath = resolveDbPath(globalDb ?? null);
const compact = args['compact'] === true || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
const opts: EmitOptions = { compact };

if (!command) {
  process.stdout.write('No command given. Run --help for usage.\n');
  process.exit(1);
}

if (command === 'self-test') {
  process.exit(cmdSelfTest(opts));
}

let db: DatabaseSync;
try {
  db = connectDb(dbPath);
} catch (err) {
  process.stderr.write(`awareness: failed to connect DB at ${dbPath}: ${String(err)}\n`);
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
    case 'status':         exitCode = cmdStatus(db, dbPath, args, opts); break;
    case 'init':           exitCode = cmdInit(db, dbPath, opts); break;
    case 'prune-stale-locks': exitCode = emit({ db_path: dbPath, ...pruneStale(db, args) }, 0, opts); break;
    case 'audit-unverified':  exitCode = cmdAuditUnverified(db, args, dbPath, opts); break;
    case 'verify':             exitCode = cmdVerify(db, args, dbPath, opts); break;
    case 'notify-get': {
      const ngFormat = String(args['format'] ?? 'json');
      const ngAgentId = args['agent_id'] as string | undefined;
      // If agent-id provided and NOT hook format → real inbox
      // Otherwise → smart briefing (hooks path)
      if (ngAgentId && ngFormat !== 'hook') {
        exitCode = cmdNotifyGet(db, args, dbPath, opts);
      } else {
        const ngParams: Record<string, unknown> = {
          workspace: args['workspace'] as string | undefined,
          artifact: args['artifact'] as string | undefined,
          format: ngFormat,
          agent_id: ngAgentId,
        };
        const ngResult = notifyGet(db, ngParams) as unknown as Record<string, unknown>;
        if (ngFormat === 'hook' && ngResult['additionalContext']) {
          exitCode = emit({ additionalContext: ngResult['additionalContext'] }, 0, opts);
        } else {
          exitCode = emit({ db_path: dbPath, ...ngResult }, 0, opts);
        }
      }
      break;
    }
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
    case 'workspace-status': {
      const wsStatusResult = getWorkspaceStatus(db, {
        workspace_path: args['workspace'] as string | undefined,
        artifact: args['artifact'] as string | undefined,
      });
      exitCode = emit({ db_path: dbPath, ...wsStatusResult }, 0, opts);
      break;
    }
    case 'digest': {
      const retDays = args['retention_days'] ? Number(args['retention_days']) : undefined;
      const isDryRun = Boolean(args['dry_run'] ?? args['dry-run']);
      const digestResult = digest(db, {
        ...(retDays !== undefined ? { retention_days: retDays } : {}),
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
      const waitSecs = args['wait_seconds'] ? parseInt(String(args['wait_seconds']), 10) : null;
      const retrySecs = args['retry_interval'] ? parseInt(String(args['retry_interval']), 10) : null;
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
    case 'memory-index':    exitCode = cmdMemoryIndex(db, args, dbPath, opts); break;
    case 'forget':          exitCode = cmdForget(db, args, dbPath, opts); break;
    case 'refine-delete':   exitCode = cmdRefineDelete(db, args, dbPath, opts); break;
    case 'export-harness':  exitCode = cmdExportHarness(db, args, dbPath, opts); break;
    case 'agent-registry':  exitCode = cmdAgentRegistry(db, args, dbPath, opts); break;
    case 'notify':          exitCode = cmdNotify(db, args, dbPath, opts); break;
    case 'agent-signal':    exitCode = cmdAgentSignal(db, args, dbPath, opts); break;
    case 'notify-resolve':  exitCode = cmdNotifyResolve(db, args, dbPath, opts); break;
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
