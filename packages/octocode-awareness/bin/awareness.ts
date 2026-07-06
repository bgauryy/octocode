/**
 * awareness.ts — CLI entry point for @octocodeai/octocode-awareness.
 *
 * Thin wrapper: parse args → call domain functions → emit JSON.
 * Compiled to dist/bin/awareness.js by build.mjs.
 * Requires Node >=22.
 */

// Fail fast if Node is too old
if (parseInt(process.version.slice(1), 10) < 22) {
  process.stderr.write(`awareness requires Node >=22 (got ${process.version})\n`);
  process.exit(1);
}

import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { DatabaseSync } from 'node:sqlite';

import {
  connectDb, initDb, hasFts, resolveDbPath,
} from '../src/db.js';
import { insertMemory, getMemory, mineWeakness, forgetMemory } from '../src/memory.js';
import { insertRefinement, getRefinements, deleteRefinement } from '../src/refinements.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { reflect } from '../src/reflect.js';
import { pruneStale, notifyGet, sessionCapture, waitForLock, digest, getWorkspaceStatus, exportMemoryDoc, exportHarness } from '../src/maintenance.js';
import { insertNotification, getNotifications, resolveNotification, pruneNotifications, agentSignal } from '../src/notifications.js';
import { auditUnverified, markVerified } from '../src/verify.js';
import {
  utcNow, normalizeLabel,
} from '../src/helpers.js';

// ─── Arg parser ───────────────────────────────────────────────────────────────

type ArgValue = string | boolean | string[];
type ParsedArgs = Record<string, ArgValue> & { _: string[] };

const ARRAY_FLAGS = new Set([
  'tag', 'reference', 'file', 'target_file', 'supersedes', 'label', 'state',
  'memory_id', 'refinement_id', 'notification_id', 'ref_id', 'regex', 'file_regex',
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
  payload['schema_version'] = 1;
  const compact = opts.compact === true || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
  process.stdout.write((compact ? JSON.stringify(payload) : JSON.stringify(payload, null, 2)) + '\n');
  return exitCode;
}

function die(message: string, extras: Record<string, unknown> = {}): never {
  process.stdout.write(JSON.stringify({ ok: false, error: message, schema_version: 1, ...extras }, null, 2) + '\n');
  process.exit(1);
}

// ─── Commands ─────────────────────────────────────────────────────────────────

function cmdTellMemory(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const agentId = String(args['agent_id'] ?? 'agent');
  const taskContext = String(args['task_context'] ?? '');
  const observation = String(args['observation'] ?? '');
  const importanceScore = args['importance_score'];

  if (!taskContext) die('--task-context is required');
  if (!observation) die('--observation is required');
  const imp = parseInt(String(importanceScore), 10);
  if (isNaN(imp) || imp < 1 || imp > 10) die('--importance-score must be 1–10');

  const rawTag = args['tag'];
  const tags = Array.isArray(rawTag) ? rawTag : rawTag ? [String(rawTag)] : [];
  const rawRef = args['reference'];
  const references = Array.isArray(rawRef) ? rawRef : rawRef ? [String(rawRef)] : [];
  const rawSup = args['supersedes'];
  const supersedes = Array.isArray(rawSup) ? rawSup : rawSup ? [String(rawSup)] : [];
  const rawLabel = args['label'];
  const label = Array.isArray(rawLabel) ? rawLabel[0] : String(rawLabel ?? '');

  const { memory, superseded } = insertMemory(db, {
    agentId, taskContext, observation, importanceScore: imp,
    label: normalizeLabel(label),
    tags, references, supersedes,
    failureSignature: args['failure_signature'] ? String(args['failure_signature']) : null,
    validFrom: args['valid_from'] ? String(args['valid_from']) : null,
    validTo: args['valid_to'] ? String(args['valid_to']) : null,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    file: args['file'] ? String(args['file']) : null,
  });

  return emit({ db_path: dbPath, memory, superseded }, 0, opts);
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
    states,
    sort: String(args['sort'] ?? 'smart'),
    globalOnly: Boolean(args['global_only']),
    strictScope: Boolean(args['strict_scope']),
    asOf: args['as_of'] ? String(args['as_of']) : null,
    references,
    regex,
    fileRegex,
    files: getFiles,
  });

  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdRefineSet(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const reasoning = String(args['reasoning'] ?? '');
  const remember = String(args['remember'] ?? '');
  if (!reasoning) die('--reasoning is required');
  if (!remember) die('--remember is required');

  const rawState = args['state'];
  const stateVal = Array.isArray(rawState) ? rawState[0] : String(rawState ?? 'open');
  const rawFile = args['file'];
  const files = Array.isArray(rawFile) ? rawFile : rawFile ? [String(rawFile)] : [];

  const { refinement } = insertRefinement(db, {
    agentId: String(args['agent_id'] ?? 'agent'),
    reasoning, remember,
    quality: (String(args['quality'] ?? 'good')) as 'good' | 'bad' | 'handoff',
    state: (stateVal ?? 'open') as 'open' | 'ongoing' | 'done',
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
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
    repo: args['repo'] ? String(args['repo']) : null,
    quality: args['quality'] ? String(args['quality']) as 'good' | 'bad' | 'handoff' : undefined,
    includeHandoffs: Boolean(args['include_handoffs']),
    states,
    limit: parseInt(String(args['limit'] ?? '10'), 10),
  });

  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdReflect(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  if (!args['task']) die('--task is required');

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
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
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

  const result = preFlightIntent(db, {
    agentId: String(args['agent_id'] ?? 'agent'),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    rationale: String(args['rationale'] ?? 'agent write operation'),
    testPlan: String(args['test_plan'] ?? 'post-edit verification'),
    targetFiles,
    lockType: (String(args['lock_type'] ?? 'EXCLUSIVE')) as 'EXCLUSIVE' | 'SHARED',
    ttlMs,
  });

  if (!result.ok) return emit({ db_path: dbPath, ...result }, 2, opts);
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdAuditUnverified(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const result = auditUnverified(db, {
    agentId: args['agent_id'] ? String(args['agent_id']) : null,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    abandon: Boolean(args['abandon']),
  });
  return emit({ db_path: dbPath, ...result }, result.count > 0 ? 1 : 0, opts);
}

function cmdVerify(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const allPending = Boolean(args['all_pending']);
  if (!allPending && !args['intent_id']) {
    return emit({ error: '--intent-id is required (or use --all-pending)' }, 1, opts);
  }
  const statusArg = args['status'] ? String(args['status']) : 'SUCCESS';
  if (statusArg !== 'SUCCESS' && statusArg !== 'FAILED') {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts);
  }
  const result = markVerified(db, {
    intentId: args['intent_id'] ? String(args['intent_id']) : undefined,
    agentId: String(args['agent_id'] ?? 'agent'),
    allPending,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
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

  if (!args['intent_id'] && targetFiles.length === 0) {
    return emit({ error: 'release-file-lock requires --intent-id or --target-file' }, 1, opts);
  }

  const result = releaseFileLock(db, {
    agentId: String(args['agent_id'] ?? 'agent'),
    intentId: args['intent_id'] ? String(args['intent_id']) : null,
    targetFiles,
    status: (String(args['status'] ?? 'SUCCESS')) as 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED',
    verified: Boolean(args['verified']),
    verifiedNote: args['verified_note'] ? String(args['verified_note']) : undefined,
  });

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
  let sql = `SELECT memory_id, label, importance_score, task_context, observation, file, tags_json, created_at
     FROM agent_memories WHERE state = 'ACTIVE' AND importance_score >= ?`;
  if (wsPath) { sql += ' AND (workspace_path = ? OR workspace_path IS NULL)'; binds.push(wsPath); }
  sql += ' ORDER BY importance_score DESC, access_count DESC, last_accessed_at DESC LIMIT ?';
  binds.push(limit);
  void conds;

  type MemRow = { memory_id: string; label: string; importance_score: number; task_context: string; observation: string; file: string | null; tags_json: string; created_at: string };
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
    lines.push(`## [${m.label}:${m.importance_score}] ${m.task_context.slice(0, 80)}`);
    lines.push(`> ${m.observation.slice(0, 200)}`);
    if (tags) lines.push(`*Tags: ${tags}*`);
    if (m.file) lines.push(`*File: ${m.file}*`);
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
  const rawTags = args['tag'];
  const tags = Array.isArray(rawTags) ? rawTags : rawTags ? [String(rawTags)] : [];
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
    dryRun: Boolean(args['dry_run']),
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdExportHarness(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const result = exportHarness(db, {
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : undefined,
    min_importance: args['min_importance'] ? parseInt(String(args['min_importance']), 10) : undefined,
    workspace_path: args['workspace'] ? String(args['workspace']) : null,
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
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
    repo: args['repo'] ? String(args['repo']) : null,
    kinds: kinds as import('../src/types.js').NotificationKind[],
    threadId: args['thread_id'] ? String(args['thread_id']) : null,
    unreadOnly: args['all'] ? false : true,
    markRead: Boolean(args['mark_read']),
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : 20,
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdNotifyResolve(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawIds = args['notification_id'];
  const notificationIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const result = resolveNotification(db, {
    notificationIds,
    threadId: args['thread_id'] ? String(args['thread_id']) : null,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
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
  const rawNotificationIds = args['notification_id'];
  const notificationIds = Array.isArray(rawNotificationIds) ? rawNotificationIds : rawNotificationIds ? [String(rawNotificationIds)] : [];
  const result = agentSignal(db, {
    action: action as import('../src/types.js').AgentSignalAction,
    agentId: String(args['agent_id']),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    kind: args['kind'] && !Array.isArray(args['kind']) ? String(args['kind']) as import('../src/types.js').NotificationKind : undefined,
    subject: args['subject'] ? String(args['subject']) : undefined,
    body: args['body'] ? String(args['body']) : null,
    toAgents,
    files,
    refs,
    importance: args['importance'] ? parseInt(String(args['importance']), 10) : undefined,
    inReplyTo: args['in_reply_to'] ? String(args['in_reply_to']) : null,
    threadId: args['thread_id'] ? String(args['thread_id']) : null,
    notificationIds,
    unreadOnly: args['all'] ? false : args['unread_only'] as boolean | undefined,
    markRead: Boolean(args['mark_read']),
    kinds: kinds as import('../src/types.js').NotificationKind[],
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdNotifyPrune(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawIds = args['notification_id'];
  const notificationIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const result = pruneNotifications(db, {
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    notificationIds,
    resolvedOnly: Boolean(args['resolved']),
    olderThanDays: args['older_than_days'] ? parseInt(String(args['older_than_days']), 10) : undefined,
    dryRun: Boolean(args['dry_run']),
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

function cmdStatus(db: DatabaseSync, dbPath: string, args: ParsedArgs, opts: EmitOptions): number {
  const now = utcNow();
  db.prepare("DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?").run(now);

  const memCount = (db.prepare('SELECT COUNT(*) AS count FROM agent_memories').get() as { count: number }).count;
  const memStates = Object.fromEntries(
    (db.prepare("SELECT state, COUNT(*) AS count FROM agent_memories GROUP BY state").all() as Array<{ state: string; count: number }>)
      .map(r => [r.state, r.count])
  );
  const memLabels = Object.fromEntries(
    (db.prepare("SELECT COALESCE(label,'OTHER') AS label, COUNT(*) AS count FROM agent_memories GROUP BY label").all() as Array<{ label: string; count: number }>)
      .map(r => [r.label, r.count])
  );
  const activeIntents = (db.prepare("SELECT COUNT(*) AS count FROM agent_intents WHERE status='ACTIVE'").get() as { count: number }).count;
  const limit = Math.min(100, Math.max(1, parseInt(String(args['limit'] ?? '20'), 10) || 20));
  const locks = db.prepare(
    'SELECT file_path, intent_id, agent_id, lock_type, acquired_at, expires_at FROM file_locks ORDER BY acquired_at DESC LIMIT ?'
  ).all(limit);
  const openRefinements = (db.prepare(
    "SELECT COUNT(*) AS count FROM refinements WHERE state IN ('open','ongoing')"
  ).get() as { count: number }).count;

  return emit({
    db_path: dbPath,
    fts_enabled: hasFts(db),
    memory_count: memCount,
    memory_states: memStates,
    memory_labels: memLabels,
    active_intent_count: activeIntents,
    open_refinements: openRefinements,
    locks,
    workspace_path: args['workspace'] ? String(args['workspace']) : null,
  }, 0, opts);
}

function cmdInit(db: DatabaseSync, dbPath: string, opts: EmitOptions): number {
  const memCount = (db.prepare('SELECT COUNT(*) AS count FROM agent_memories').get() as { count: number }).count;
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
    importanceScore: 7,
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
          prune-stale-locks  audit-unverified  verify  mine-weakness  export-harness  memory-index
          notify  agent-signal  notify-get  notify-resolve  notify-prune  session-capture  wait-for-lock  digest

common options:
  --db <path>     Override DB path (default: $OCTOCODE_MEMORY_HOME/awareness.sqlite3)
  --compact       Compact JSON output (or OCTOCODE_AWARENESS_COMPACT=1)

tell-memory:
  --agent-id <id>  --task-context <text>  --observation <text>
  --importance-score <1-10>  --label <LABEL>  [--tag <t>]...  [--reference <r>]...

get-memory:
  --query <text>  [--limit <n>]  [--min-importance <n>]  [--label <L>]  [--smart]
  [--reference <r>]...  [--regex <pattern>]...  [--file-regex <pat>]...  [--file <path>]...
  [--sort smart|importance|recent|accessed]  [--state ACTIVE|SUPERSEDED]...
  [--strict-scope]  [--global-only]  [--as-of <ISO>]

forget:
  [--memory-id <id>]...  [--tag <t>]...  [--before <ISO>]  [--max-importance <n>]  [--dry-run]

refine-delete:
  --refinement-id <id>...  [--workspace <path>]  [--dry-run]

export-harness:
  [--limit <n>]  [--min-importance <n>]  [--workspace <path>]
  preview top lessons as an AGENTS.md block

notify:
  --agent-id <id>  --kind claim|handoff|question|reply|blocker|request|decision|fyi
  --subject <text>  [--to <agent-id>]  [--body <text>]  [--file <path>]...
  [--ref-id <id>]...  [--in-reply-to <notification-id>]  [--importance <1-10>]

notify-resolve:
  [--notification-id <id>]...  [--thread-id <id>]

notify-prune:
  [--notification-id <id>]...  [--resolved]  [--older-than-days <n>]  [--dry-run]

reflect:
  --agent-id <id>  --task <text>  --outcome worked|partial|failed
  [--lesson <text>]  [--worked <text>]  [--didnt-work <text>]
  [--fix-repo <text>]  [--fix-harness <text>]

refine-get:
  [--state open|ongoing|done]...  [--quality good|bad|handoff]  [--include-handoffs]
  session handoffs are hidden unless --include-handoffs or --quality handoff is passed

workspace-status:
  [--workspace <path>]   show active locks, agent intents, and memory counts

mine-weakness:
  [--agent-id <id>]  [--workspace <path>]  [--min-count <n>]  [--limit <n>]
  find recurring failure patterns grouped by failure_signature

digest:
  [--retention-days <n>]  [--dry-run]  [--export-doc [path]]
  archive expired memories, prune old superseded rows/refinements, rebuild FTS
  --dry-run: preview counts without mutating anything
  --export-doc: write a markdown memory report to .octocode/memory-reports/

pre-flight-intent:
  --agent-id <id>  [--workspace <path>]  [--target-file <path>]...  [--ttl-minutes <n>]

release-file-lock:
  --agent-id <id>  (--intent-id <id> | --target-file <path>)  [--status SUCCESS|PENDING|FAILED]
  [--verified]  [--verified-note <text>]

audit-unverified:
  [--agent-id <id>]  [--workspace <path>]  [--abandon]
  exits 1 when unverified (PENDING) intents exist; exits 0 when clear
  --abandon: dismiss all PENDING intents as FAILED (clear orphaned sessions)

verify:
  (--intent-id <id> | --all-pending)  --agent-id <id>
  [--status SUCCESS|FAILED]  [--message <text>]  [--workspace <path>]
  marks a PENDING intent as verified; --all-pending clears every PENDING for this agent
`;

// ─── Entry point ──────────────────────────────────────────────────────────────

const rawArgv = process.argv.slice(2);

if (rawArgv.length === 0 || rawArgv.includes('--help') || rawArgv.includes('-h')) {
  process.stdout.write(HELP + '\n');
  process.exit(0);
}

const { dbPath: globalDb, filtered: filteredArgv } = extractGlobalDb(rawArgv);
const [command, ...rest] = filteredArgv;
const args = parseArgs(rest ?? []);
if (globalDb) args['db'] = globalDb;

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
    case 'release-file-lock':
    case 'release-intent': exitCode = cmdReleaseFileLock(db, args, dbPath, opts); break;
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
        minCount:      args['min_count'] ? Number(args['min_count']) : undefined,
        limit:         args['limit']     ? Number(args['limit'])     : undefined,
        cwd:           args['cwd']       as string | undefined,
      };
      exitCode = emit({ db_path: dbPath, ...mineWeakness(db, mwParams) }, 0, opts);
      break;
    }
    case 'workspace-status': {
      const wsStatusResult = getWorkspaceStatus(db, {
        workspace_path: args['workspace'] as string | undefined,
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
          const { mkdirSync, writeFileSync } = await import('node:fs');
          const { join } = await import('node:path');
          const docDir = join(wsPath, '.octocode', 'memory-reports');
          mkdirSync(docDir, { recursive: true });
          const dateStr = new Date().toISOString().slice(0, 16).replace('T', '-').replace(':', '');
          const docPath = (typeof (args['export_doc'] ?? args['export-doc']) === 'string'
            ? args['export_doc'] ?? args['export-doc']
            : join(docDir, `memory-report-${dateStr}.md`)) as string;
          writeFileSync(docPath, exportMemoryDoc(db, { workspace_path: wsPath }), 'utf8');
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
