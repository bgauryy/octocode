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
  connectDb, initDb, hasFts, resolveDbPath, evictExpiredLocks,
} from '../src/db.js';
import { insertMemory, getMemory, mineWeakness, forgetMemory, storeEmbedding, searchByEmbedding, loadMemoriesByIds, bumpAccess } from '../src/memory.js';
import { resolveEmbedCommand, runHostEmbedder } from '../src/embed-host.js';
import { mineDocStaleness, proposeDocRefresh } from '../src/docs.js';
import { listSkillDocs, showSkillDoc } from '../src/docs-catalog.js';
import { insertRefinement, getRefinements, updateRefinement, deleteRefinement } from '../src/refinements.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { reflect } from '../src/reflect.js';
import type { EvalFailure } from '../src/types.js';
import { pruneStale, notifyGet, sessionCapture, waitForLock, digest, getWorkspaceStatus, exportMemoryDoc, exportHarness } from '../src/maintenance.js';
import { insertNotification, getNotifications, resolveNotification, pruneNotifications, agentSignal } from '../src/notifications.js';
import { auditUnverified, markVerified } from '../src/verify.js';
import { registerAgent, listAgents } from '../src/agents.js';
import { hooksInstallUsage, runHooksInstall } from '../src/hooks-install.js';
import { attendAwareness } from '../src/attend.js';
import { formatAwarenessQueryResult, injectRepoContext, queryAwareness, writeAwarenessView } from '../src/repo-context.js';
import {
  normalizeLabel,
  normalizeFilePath,
  parseJsonList,
} from '../src/helpers.js';
import { normalizeWorkspacePath } from '../src/git.js';
import { runHookCommand } from './hook-runner.js';

// ─── Arg parser ───────────────────────────────────────────────────────────────

type ArgValue = string | boolean | string[];
type ParsedArgs = Record<string, ArgValue> & { _: string[] };

const MAX_CLI_TTL_SECONDS = 10 * 60;
const MEMORY_SORTS = new Set(['smart', 'score', 'importance', 'recent', 'accessed']);

const ARRAY_FLAGS = new Set([
  'tag', 'tags', 'reference', 'file', 'fix_file', 'target_file', 'supersedes', 'label', 'state',
  'memory_id', 'refinement_id', 'signal_id', 'ref_id', 'task_id', 'regex', 'file_regex',
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
  'forget': ['memory_id', 'tag', 'tags', 'before', 'max_importance', 'workspace', 'artifact', 'repo', 'ref', 'dry_run'],
  'reflect': ['agent_id', 'task', 'outcome', 'lesson', 'worked', 'didnt_work', 'fix_repo', 'fix_file', 'fix_harness', 'failure_signature', 'importance', 'judgment_note', 'duo', 'eval_failure_json', 'workspace', 'artifact', 'repo', 'ref'],
  'refine-set': ['agent_id', 'reasoning', 'remember', 'quality', 'state', 'workspace', 'artifact', 'repo', 'ref', 'file', 'refinement_id'],
  'refine-get': ['workspace', 'artifact', 'repo', 'ref', 'quality', 'include_handoffs', 'state', 'limit'],
  'refine-delete': ['refinement_id', 'workspace', 'artifact', 'dry_run'],
  'pre-flight-intent': ['agent_id', 'workspace', 'artifact', 'rationale', 'test_plan', 'plan_doc_ref', 'target_file', 'file', 'lock_type', 'ttl_minutes', 'ttl_seconds', 'wait_seconds', 'retry_interval'],
  'release-file-lock': ['agent_id', 'task_id', 'target_file', 'file', 'status', 'verified', 'verified_note', 'workspace', 'artifact'],
  'status': ['workspace', 'artifact', 'limit'],
  'init': [],
  'self-test': [],
  'prune-stale-locks': ['older_than_minutes', 'expired_only', 'agent_id', 'target_file', 'workspace', 'artifact', 'dry_run'],
  'audit-unverified': ['agent_id', 'workspace', 'artifact', 'abandon'],
  'verify': ['task_id', 'all_pending', 'agent_id', 'status', 'message', 'workspace', 'artifact'],
  'mine-weakness': ['agent_id', 'workspace', 'artifact', 'min_count', 'limit', 'cwd'],
  'doc-staleness': ['agent_id', 'workspace', 'artifact', 'targets_json', 'min_edits', 'min_lines', 'propose', 'session_id'],
  'docs-catalog': ['action', 'name'],
  'export-harness': ['limit', 'min_importance', 'workspace', 'artifact'],
  'query': ['view', 'query', 'limit', 'format', 'out', 'workspace', 'artifact', 'repo', 'ref', 'agent_id', 'state', 'label', 'file', 'since', 'include_bodies'],
  'attend': ['query', 'limit', 'workspace', 'artifact', 'repo', 'ref', 'file', 'include_bodies', 'explain_organ'],
  'repo-inject': ['query', 'limit', 'out', 'out_dir', 'workspace', 'artifact', 'repo', 'ref', 'mode', 'check', 'include_view', 'include_bodies'],
  'agent-registry': ['action', 'agent_id', 'agent_name', 'workspace', 'artifact', 'context', 'limit'],
  'agent-signal': ['action', 'agent_id', 'workspace', 'artifact', 'repo', 'ref', 'kind', 'subject', 'body', 'to_agent', 'file', 'ref_id', 'importance', 'in_reply_to', 'thread_id', 'signal_id', 'all', 'unread_only', 'mark_read', 'limit', 'format'],
  'notify-prune': ['signal_id', 'resolved', 'older_than_days', 'dry_run', 'workspace', 'artifact'],
  'session-capture': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'reason', 'cwd'],
  'wait-for-lock': ['agent_id', 'target_file', 'file', 'workspace', 'artifact', 'lock_type', 'wait_seconds', 'retry_interval'],
  'digest': ['retention_days', 'refinement_handoff_retention_days', 'refinement_done_retention_days', 'dry_run', 'export_doc', 'workspace', 'artifact'],
  'hook-run': [],
  'hooks-install': ['host', 'project_dir', 'global', 'check', 'strict', 'dry_run', 'remove'],
  'schema': [],
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

  const { memory, superseded, noveltyScore, similarMemoryIds } = insertMemory(db, {
    agentId, taskContext, observation, importance: imp,
    label: normalizeLabel(label),
    tags, references: [...references, ...fileReferences], supersedes,
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
          const ranked = loadMemoriesByIds(db, hits.map(hit => hit.memory_id));
          const simById = new Map(hits.map(hit => [hit.memory_id, hit.similarity]));
          for (const memory of ranked) {
            const similarity = simById.get(memory.memory_id) ?? 0;
            memory.score = similarity;
            memory.lexical = similarity;
          }
          bumpAccess(db, ranked.map(memory => memory.memory_id));
          payload['memories'] = ranked.slice(0, limit);
          payload['count'] = Math.min(ranked.length, limit);
          payload['mode'] = 'semantic';
          payload['embedding_model'] = model;
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
  if (ttlMinutes != null && ttlMinutes > 10) die('--ttl-minutes must be <= 10');
  if (ttlSeconds != null && ttlSeconds > MAX_CLI_TTL_SECONDS) die('--ttl-seconds must be <= 600');
  const ttlMs = ttlSeconds != null ? ttlSeconds * 1000 : ttlMinutes != null ? ttlMinutes * 60000 : null;

  const claimParams = {
    agentId: String(args['agent_id'] ?? 'agent'),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    rationale: String(args['rationale'] ?? 'agent write operation'),
    testPlan: String(args['test_plan'] ?? 'post-edit verification'),
    planDocRef: args['plan_doc_ref'] ? String(args['plan_doc_ref']) : null,
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
    const retrySeconds = args['retry_interval'] ? parseInt(String(args['retry_interval']), 10) : null;
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
  const taskIds = valuesFor(args, 'task_id');
  if (!allPending && taskIds.length === 0) {
    return emit({ error: '--task-id is required (or use --all-pending)' }, 1, opts);
  }
  const statusArg = args['status'] ? String(args['status']) : 'SUCCESS';
  if (statusArg !== 'SUCCESS' && statusArg !== 'FAILED') {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts);
  }
  if (!allPending && taskIds.length > 1) {
    const results = taskIds.map((taskId) => markVerified(db, {
      taskId,
      agentId: String(args['agent_id'] ?? 'agent'),
      workspacePath: args['workspace'] ? String(args['workspace']) : null,
      artifact: args['artifact'] ? String(args['artifact']) : null,
      message: args['message'] ? String(args['message']) : undefined,
      status: statusArg as 'SUCCESS' | 'FAILED',
    }));
    const failed = results.find((result) => !result.ok);
    if (failed && !failed.ok) {
      return emit({ db_path: dbPath, ok: false, error: failed.error, task_id: null, task_ids: taskIds, results }, 1, opts);
    }
    return emit({
      db_path: dbPath,
      task_id: null,
      task_ids: taskIds,
      count: results.length,
      status: statusArg,
      results,
    }, 0, opts);
  }
  const result = markVerified(db, {
    taskId: taskIds[0],
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

  const taskId = firstValue(args, 'task_id');
  if (!taskId && targetFiles.length === 0) {
    return emit({ error: 'release-file-lock requires --task-id or --target-file' }, 1, opts);
  }

  const result = releaseFileLock(db, {
    agentId: String(args['agent_id'] ?? 'agent'),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    taskId: taskId ?? null,
    targetFiles,
    status: (String(args['status'] ?? 'SUCCESS')) as 'PENDING' | 'SUCCESS' | 'FAILED',
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
  let sql = `SELECT memory_id, label, importance, task_context, observation, tags_json,
                    failure_signature, created_at
     FROM memories WHERE state = 'ACTIVE' AND importance >= ?`;
  if (wsPath) { sql += ' AND (workspace_path = ? OR workspace_path IS NULL)'; binds.push(wsPath); }
  if (args['artifact']) { sql += ' AND (artifact = ? OR artifact IS NULL)'; binds.push(String(args['artifact'])); }
  if (args['repo']) { sql += ' AND (repo = ? OR repo IS NULL)'; binds.push(String(args['repo'])); }
  if (args['ref']) { sql += ' AND (ref = ? OR ref IS NULL)'; binds.push(String(args['ref'])); }
  sql += ' ORDER BY importance DESC, access_count DESC, last_accessed_at DESC LIMIT ?';
  binds.push(limit);
  void conds;

  type MemRow = {
    memory_id: string;
    label: string;
    importance: number;
    task_context: string;
    observation: string;
    tags_json: string;
    failure_signature: string | null;
    created_at: string;
    references: string[];
  };
  const rows = db.prepare(sql).all(...binds) as unknown as MemRow[];
  if (rows.length > 0) {
    const refs = db.prepare(
      `SELECT memory_id, reference
       FROM memory_refs
       WHERE memory_id IN (${rows.map(() => '?').join(',')})
       ORDER BY memory_id, ordinal`
    ).all(...rows.map(row => row.memory_id)) as unknown as Array<{ memory_id: string; reference: string }>;
    const refsByMemory = new Map<string, string[]>();
    for (const ref of refs) {
      const list = refsByMemory.get(ref.memory_id) ?? [];
      list.push(ref.reference);
      refsByMemory.set(ref.memory_id, list);
    }
    for (const row of rows) row.references = refsByMemory.get(row.memory_id) ?? [];
  }

  const now = new Date().toISOString().slice(0, 10);
  const lines = [
    `# Memory Index — ${now}`,
    `<!-- Auto-generated by octocode-awareness memory-index. Regenerate after recording or forgetting memories. -->`,
    '',
    `**${rows.length} active memories** (importance ≥ ${minImportance}, sorted by salience)`,
    '',
  ];
  for (const m of rows) {
    const tags = parseJsonList(m.tags_json).join(', ');
    lines.push(`## [${m.label}:${m.importance}] ${m.task_context.slice(0, 80)}`);
    lines.push(`> ${m.observation.slice(0, 200)}`);
    if (tags) lines.push(`*Tags: ${tags}*`);
    if (m.references.length > 0) lines.push(`*References: ${m.references.join(', ')}*`);
    if (m.failure_signature) lines.push(`*Failure: ${m.failure_signature}*`);
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

function cmdView(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const view = String(args['view'] ?? args._[0] ?? 'all');
  const result = writeAwarenessView(db, {
    view,
    workspacePath: args['workspace'] ? String(args['workspace']) : process.cwd(),
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
    out: args['out'] ? String(args['out']) : undefined,
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
    agentId: args['agent_id'] ? String(args['agent_id']) : null,
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

const HELP = `usage: octocode-awareness <command> [options]
common: --db <path> --compact
local-first: use octocode-awareness or a bundled local node path when present
fallback: npx @octocodeai/octocode-awareness <command>
agent map: octocode-awareness schema commands --compact
schema: octocode-awareness schema commands|list|json-schema <name>|example <name>|validate <name> <json-file|->

easy install:
  If the CLI is bundled locally, tell your agent to run that local CLI:
    octocode-awareness maintenance init --compact
  Registry fallback only when no local CLI exists:
    npx @octocodeai/octocode-awareness maintenance init --compact
  Then install the bundled Agent Skill:
    npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common
  Registry fallback:
    npx octocode skill --name octocode-awareness

supported agents: Codex, Claude Code, Cursor, Pi, and custom library/CLI hosts
surfaces: CLI = control plane; Agent Skill = operating loop; hooks/Pi bridge = lifecycle automation

start: attend, workspace status, memory recall, refinement get, signal list, query <view>
edit: lock acquire, lock wait, lock release, lock prune, verify mark, verify audit
messages: signal publish, signal list, signal reply, signal ack, signal resolve, signal prune, agent register, agent list
learning: memory record, memory forget, refinement set, refinement get, refinement delete, reflect record, reflect mine-weakness, reflect export-harness, docs list, docs show, docs staleness
repo context: query <view> [--format json|table|csv|markdown|html], repo inject
hooks: hook run <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end>, hooks install|check|remove --host claude|codex|cursor
utility: session capture, maintenance init, maintenance self-test, maintenance digest

examples:
  octocode-awareness workspace status --workspace "$PWD" --compact
  octocode-awareness attend --workspace "$PWD" --query "current task" --compact
  octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact
  octocode-awareness docs list --compact
  octocode-awareness docs show full-flow
  octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit" --compact
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact
  octocode-awareness schema commands --compact
  octocode-awareness query gotchas --workspace "$PWD" --format json --limit 20 --compact
  octocode-awareness repo inject --workspace "$PWD" --mode local --compact

Run "octocode-awareness <command> --help" for command flags. Exit 2 = lock conflict or wait timeout.`;

const HELP_COMPACT = `octocode-awareness: canonical noun/verb CLI. Use --compact for JSON.
local-first: octocode-awareness <command>; fallback: npx @octocodeai/octocode-awareness <command>; skill: npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common; agents: Codex, Claude, Cursor, Pi
start: attend; workspace status; memory recall; refinement get; signal list; docs list
edit: lock acquire|wait|release|prune; verify audit|mark
msg: signal publish|list|reply|ack|resolve|prune; agent register|list
learn: memory record|forget; reflect record|mine-weakness|export-harness; maintenance digest
repo: query <view> --format json|table|csv|markdown|html; repo inject
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
  'release-file-lock': 'octocode-awareness lock release --agent-id agent --task-id task_123 --status SUCCESS --verified --compact',
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
schema: octocode-awareness schema json-schema tell_memory --compact`,
  'get-memory': `usage: octocode-awareness memory recall [options]
filters: [--query <text>] [--limit <n>] [--min-importance <n>] [--label <l>]... [--tag <t>]... [--reference <r>]... [--file <p>]... [--regex <r>]... [--file-regex <r>]...
scope: [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--strict-scope] [--global-only]
rank: [--sort smart|score|importance|recent|accessed] [--state ACTIVE|SUPERSEDED]... [--as-of <iso>] [--semantic] [--explain]
example: octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact
schema: octocode-awareness schema json-schema get_memory --compact`,
  'pre-flight-intent': `usage: octocode-awareness lock acquire --agent-id <id> --target-file <p>... [--workspace <p>] [--artifact <a>] [--rationale <t>] [--test-plan <t>] [--lock-type EXCLUSIVE|SHARED] [--ttl-minutes <n>] [--wait-seconds <n>]
example: octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact
schema: octocode-awareness schema json-schema pre_flight_intent --compact`,
  'agent-signal': `usage: octocode-awareness signal publish|list|reply|ack|resolve --agent-id <id> [--to-agent <id>]... [--signal-id <id>]... [--thread-id <id>] [--kind <k>] [--subject <t>] [--body <t>] [--file <p>]...
examples:
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact
  octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --file src/file.ts --workspace "$PWD" --compact
  octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact
schema: octocode-awareness schema json-schema agent_signal --compact`,
  'verify': `usage: octocode-awareness verify mark (--task-id <id>... | --all-pending) --agent-id <id> [--status SUCCESS|FAILED] [--message <t>] [--workspace <p>] [--artifact <a>]
example: octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema verify --compact`,
  'reflect': `usage: octocode-awareness reflect record --agent-id <id> --task <text> --outcome worked|partial|failed [--lesson <t>] [--fix-repo <t>] [--fix-file <p>]... [--failure-signature <s>]
example: octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep CLI nouns canonical" --compact
schema: octocode-awareness schema json-schema reflect --compact`,
  'query': `usage: octocode-awareness query <all|repo-profile|memories|gotchas|lessons|tasks|locks|agents|signals|refinements|files|activity|workboard> [--workspace <repo>] [--format json|table|csv|markdown|html] [--out <path>]
examples:
  octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact
  octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
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
    case 'docs-catalog': exitCode = cmdDocsCatalog(db, args, dbPath, opts); break;
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
    case 'query':           exitCode = cmdQuery(db, args, dbPath, opts); break;
    case 'attend':          exitCode = cmdAttend(db, args, dbPath, opts); break;
    case 'view':            exitCode = cmdView(db, args, dbPath, opts); break;
    case 'repo-inject':     exitCode = cmdRepoInject(db, args, dbPath, opts); break;
    case 'agent-registry':  exitCode = cmdAgentRegistry(db, args, dbPath, opts); break;
    case 'notify':          exitCode = cmdNotify(db, args, dbPath, opts); break;
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
