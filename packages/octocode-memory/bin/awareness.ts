/**
 * awareness.ts — CLI entry point for @octocodeai/octocode-memory.
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

import { DatabaseSync } from 'node:sqlite';

import {
  connectDb, initDb, hasFts, resolveDbPath,
} from '../src/db.js';
import { insertMemory, getMemory, mineWeakness } from '../src/memory.js';
import { insertRefinement, getRefinements } from '../src/refinements.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { reflect } from '../src/reflect.js';
import { pruneStale, notifyGet, sessionCapture, waitForLock, digest } from '../src/stubs.js';
import { auditUnverified, markVerified } from '../src/verify.js';
import {
  utcNow, normalizeLabel,
} from '../src/helpers.js';

// ─── Arg parser ───────────────────────────────────────────────────────────────

type ArgValue = string | boolean | string[];
type ParsedArgs = Record<string, ArgValue> & { _: string[] };

const ARRAY_FLAGS = new Set([
  'tag', 'reference', 'file', 'target_file', 'supersedes', 'label', 'state',
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
    asOf: args['as_of'] ? String(args['as_of']) : null,
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
    quality: (String(args['quality'] ?? 'good')) as 'good' | 'bad',
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
  });
  return emit({ db_path: dbPath, ...result }, result.count > 0 ? 1 : 0, opts);
}

function cmdVerify(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  if (!args['intent_id']) return emit({ error: '--intent-id is required' }, 1, opts);
  const statusArg = args['status'] ? String(args['status']) : 'SUCCESS';
  if (statusArg !== 'SUCCESS' && statusArg !== 'FAILED') {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts);
  }
  const result = markVerified(db, {
    intentId: String(args['intent_id']),
    agentId: String(args['agent_id'] ?? 'agent'),
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

commands: tell-memory  get-memory  reflect  refine-set  refine-get
          pre-flight-intent  release-file-lock  status  init  self-test
          prune-stale-locks  audit-unverified  verify
          notify-get  session-capture  wait-for-lock

common options:
  --db <path>     Override DB path (default: $OCTOCODE_MEMORY_HOME/awareness.sqlite3)
  --compact       Compact JSON output (or OCTOCODE_AWARENESS_COMPACT=1)

tell-memory:
  --agent-id <id>  --task-context <text>  --observation <text>
  --importance-score <1-10>  --label <LABEL>  [--tag <t>]...  [--reference <r>]...

get-memory:
  --query <text>  [--limit <n>]  [--min-importance <n>]  [--label <L>]  [--smart]

reflect:
  --agent-id <id>  --task <text>  --outcome worked|partial|failed
  [--lesson <text>]  [--worked <text>]  [--didnt-work <text>]
  [--fix-repo <text>]  [--fix-harness <text>]

pre-flight-intent:
  --agent-id <id>  [--workspace <path>]  [--target-file <path>]...  [--ttl-minutes <n>]

release-file-lock:
  --agent-id <id>  (--intent-id <id> | --target-file <path>)  [--status SUCCESS|PENDING|FAILED]

audit-unverified:
  [--agent-id <id>]  [--workspace <path>]
  exits 1 when unverified (PENDING) intents exist; exits 0 when clear

verify:
  --intent-id <id>  --agent-id <id>  [--status SUCCESS|FAILED (default SUCCESS)]
  marks a PENDING intent as verified; clears it from audit-unverified
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
    case 'prune-stale-locks': exitCode = emit({ db_path: dbPath, ...pruneStale(db, {}) }, 0, opts); break;
    case 'audit-unverified':  exitCode = cmdAuditUnverified(db, args, dbPath, opts); break;
    case 'verify':             exitCode = cmdVerify(db, args, dbPath, opts); break;
    case 'notify-get': {
      // Pass workspace + format flags through so smart briefing can scope correctly
      const ngParams: Record<string, unknown> = {
        workspace: args['workspace'] as string | undefined,
        format:    args['format'] as string | undefined ?? 'json',
        agent_id:  args['agent_id'] as string | undefined,
      };
      const ngResult = notifyGet(db, ngParams) as unknown as Record<string, unknown>;
      // For hook format, emit ONLY additionalContext so pi injects cleanly
      if (ngParams.format === 'hook' && ngResult.additionalContext) {
        exitCode = emit({ additionalContext: ngResult.additionalContext }, 0, opts);
      } else {
        exitCode = emit({ db_path: dbPath, ...ngResult }, 0, opts);
      }
      break;
    }
    case 'session-capture': exitCode = emit({ db_path: dbPath, ...sessionCapture(db, {}) }, 0, opts); break;
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
    case 'digest': {
      const retDays = args['retention_days'] ? Number(args['retention_days']) : undefined;
      exitCode = emit({ db_path: dbPath, ...digest(db, retDays !== undefined ? { retention_days: retDays } : {}) }, 0, opts);
      break;
    }
    case 'wait-for-lock':  exitCode = emit({ db_path: dbPath, ...waitForLock(db, {}) }, 0, opts); break;
    default:
      exitCode = emit({ error: `unknown command: ${command}. Run --help for usage.` }, 1, opts);
  }
} catch (err) {
  exitCode = emit({
    error: err instanceof Error ? err.message : String(err),
    stack: err instanceof Error ? err.stack : undefined,
  }, 1, opts);
}

process.exit(exitCode);
