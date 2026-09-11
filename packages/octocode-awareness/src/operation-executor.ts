import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { z } from 'zod';
import { attendWorkspace } from './attend-presence.js';
import {
  AwarenessInputError,
  commandOutput,
  emit,
  type AwarenessOperationOutput,
} from './command-output.js';
import {
  MAX_CLI_RETRY_INTERVAL_SECONDS,
  MAX_CLI_WAIT_SECONDS,
  parseBoundedSeconds,
  resolveAgentId,
  valuesFor,
  type ParsedArgs,
} from './commands/args.js';
import { cmdAgentSignal } from './commands/admin.js';
import { cmdGetMemory, cmdTellMemory } from './commands/memory.js';
import { cmdPlan, cmdTask } from './commands/plans.js';
import { cmdQuery } from './commands/repo.js';
import { cmdAuditUnverified, cmdPreFlightIntent, cmdReleaseFileLock, cmdVerify, cmdWork } from './commands/work.js';
import { connectDb, resolveDbPath } from './db-runtime.js';
import { beginWrite } from './db-transaction.js';
import { ensureCanonicalMutationEvent, workspaceEventHighWater } from './event-outbox.js';
import { normalizeWorkspacePath, repositoryWorkspacePaths } from './git.js';
import { HistoryError } from './history-store.js';
import { commandSchemaProperties } from './schema/command-properties.js';
import { DEFAULT_RETRY_MS, DEFAULT_WAIT_MS } from './maintenance-stale.js';
import { waitForLock } from './maintenance-session.js';
import type { AwarenessOperationResult, CanonicalExecutionContext, CanonicalRouteBinding } from './operation-contracts.js';
import { storageScopeForOperation } from './workspace-policy.js';

const validators = new Map<string, z.ZodType>();
const hash = (value: unknown): string => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const text = (value: unknown, limit = 120): string | undefined => {
  const output = String(value ?? '').trim();
  return output ? output.slice(0, limit) : undefined;
};
const record = (value: unknown): Record<string, unknown> | undefined =>
  value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
const asRows = (value: unknown): Array<Record<string, unknown>> =>
  Array.isArray(value) ? value.filter((row): row is Record<string, unknown> => Boolean(record(row))) : [];

interface OrientParams {
  if_revision?: string;
  limit?: number;
  offset?: number;
  file?: string | string[];
  query?: string;
}

function validate(command: string, params: Record<string, unknown>, schema: Record<string, unknown>): void {
  let validator = validators.get(command);
  if (!validator) {
    validator = z.fromJSONSchema(schema);
    validators.set(command, validator);
  }
  const result = validator.safeParse(params);
  if (!result.success) throw new AwarenessInputError(
    `Invalid parameters for ${command}: ${result.error.issues.map(issue => `${issue.path.join('.') || '/'} ${issue.message}`).join('; ')}`,
  );
}

function handlerParams(params: Record<string, unknown>): ParsedArgs {
  return Object.fromEntries([['_', []], ...Object.entries(params).map(([key, value]) => [key,
    key.endsWith('_json') && typeof value === 'object' ? JSON.stringify(value)
      : typeof value === 'number' ? String(value)
        : Array.isArray(value) ? value.map(item => typeof item === 'string' ? item : JSON.stringify(item))
          : value && typeof value === 'object' ? JSON.stringify(value) : value,
  ])]) as ParsedArgs;
}

function bindHost(
  params: Record<string, unknown>,
  properties: Record<string, unknown>,
  key: string,
  value: string | undefined,
): void {
  if (value === undefined || !Object.hasOwn(properties, key)) return;
  if (params[key] !== undefined && params[key] !== value
    && !(key === 'workspace' && typeof params[key] === 'string'
      && normalizeWorkspacePath(params[key], params[key]) === normalizeWorkspacePath(value, value))) {
    throw new AwarenessInputError(`${key} conflicts with the host binding`);
  }
  params[key] = value;
}

async function executeDomainHandler(
  db: import('node:sqlite').DatabaseSync,
  binding: CanonicalRouteBinding,
  args: ParsedArgs,
  dbPath: string,
  signal?: AbortSignal,
): Promise<number> {
  const opts = { compact: true, cli: false } as const;
  if (binding.action && ['plan', 'task', 'work', 'signal'].includes(binding.handler)) args.action = binding.action;
  switch (binding.handler) {
    case 'plan': return cmdPlan(db, args, dbPath, opts);
    case 'task': return cmdTask(db, args, dbPath, opts);
    case 'work': return cmdWork(db, args, dbPath, opts);
    case 'query':
      if (binding.action) args.view = binding.action;
      return cmdQuery(db, args, dbPath, opts);
    case 'verify': return cmdVerify(db, args, dbPath, opts);
    case 'verify-audit': return cmdAuditUnverified(db, args, dbPath, opts);
    case 'signal': return cmdAgentSignal(db, args, dbPath, opts);
    case 'memory-record': return cmdTellMemory(db, args, dbPath, opts);
    case 'memory-recall': return cmdGetMemory(db, args, dbPath, opts);
    case 'lock-release': return cmdReleaseFileLock(db, args, dbPath, opts);
    case 'history': {
      const { runHistoryCommand } = await import('./commands/history.js');
      signal?.throwIfAborted();
      const result = await runHistoryCommand(db, binding.action!, args);
      return emit(result, result.ok === false ? 2 : 0, opts);
    }
    case 'lock-acquire': {
      const waitMs = (parseBoundedSeconds(args, 'wait_seconds', 0, MAX_CLI_WAIT_SECONDS) ?? DEFAULT_WAIT_MS / 1000) * 1000;
      const retryMs = (parseBoundedSeconds(args, 'retry_interval', 1, MAX_CLI_RETRY_INTERVAL_SECONDS) ?? DEFAULT_RETRY_MS / 1000) * 1000;
      const started = performance.now();
      const claim = () => cmdPreFlightIntent(db, args, dbPath, opts);
      const first = claim();
      if (first !== 2 || args['wait_seconds'] === undefined || waitMs === 0) return first;
      while (performance.now() - started < waitMs) {
        await delay(Math.min(retryMs, waitMs - (performance.now() - started)), undefined, { signal });
        signal?.throwIfAborted();
        const available = waitForLock(db, {
          agent_id: resolveAgentId(args), target_files: valuesFor(args, 'target_file'),
          workspace_path: args['workspace'], artifact: args['artifact'], wait_ms: 0,
        });
        if (available.lock_free) return claim();
      }
      return first;
    }
    case 'lock-wait': {
      const waitMs = (parseBoundedSeconds(args, 'wait_seconds', 0, MAX_CLI_WAIT_SECONDS) ?? DEFAULT_WAIT_MS / 1000) * 1000;
      const retryMs = (parseBoundedSeconds(args, 'retry_interval', 1, MAX_CLI_RETRY_INTERVAL_SECONDS) ?? DEFAULT_RETRY_MS / 1000) * 1000;
      const started = performance.now();
      const check = () => waitForLock(db, {
        agent_id: resolveAgentId(args), target_files: valuesFor(args, 'target_file'),
        workspace_path: args['workspace'], artifact: args['artifact'], wait_ms: 0,
      });
      signal?.throwIfAborted();
      let result = check();
      while (!result.lock_free && performance.now() - started < waitMs) {
        await delay(Math.min(retryMs, waitMs - (performance.now() - started)), undefined, { signal });
        signal?.throwIfAborted();
        result = check();
      }
      return emit({ db_path: dbPath, ...result, waited_ms: Math.floor(performance.now() - started) }, result.lock_free ? 0 : 2, opts);
    }
  }
}

/** Canonical operation execution path with direct domain binding. */
export async function executeCanonicalRoute(
  operation: string,
  binding: CanonicalRouteBinding,
  input: Record<string, unknown>,
  context: CanonicalExecutionContext,
): Promise<AwarenessOperationResult> {
  const command = binding.command;
  const output: AwarenessOperationOutput = { command, compact: true, text: '', diagnostics: [] };
  return commandOutput.run(output, async () => {
    try {
      context.signal?.throwIfAborted();
      const params = { ...input };
      const properties = commandSchemaProperties(binding.schema);
      bindHost(params, properties, 'workspace', context.workspace);
      if ((Object.hasOwn(properties, 'agent_id') || Object.hasOwn(properties, 'lead_agent_id'))
        && command !== 'work list' && command !== 'work show') {
        bindHost(params, properties, Object.hasOwn(properties, 'agent_id') ? 'agent_id' : 'lead_agent_id', context.agentId);
      }
      bindHost(params, properties, 'session_id', context.sessionId);
      validate(command, params, binding.schema as Record<string, unknown>);
      const workspace = normalizeWorkspacePath(context.workspace, context.workspace) ?? resolve(context.workspace);
      const scope = storageScopeForOperation(operation, workspace, context.scope);
      const dbPath = resolveDbPath(context.database, { scope, workspace });
      const db = connectDb(dbPath);
      // Memory evidence performs filesystem reads before its domain-owned
      // row+event transaction. Task mutations also own their row+event
      // transaction so claim lifecycle methods can use BEGIN IMMEDIATE without
      // nesting under this executor.
      const domainOwnsWriteTransaction = binding.handler === 'memory-record'
        || (binding.handler === 'task' && binding.effect === 'coordination-write')
        || binding.handler === 'lock-acquire'
        || binding.handler === 'lock-release';
      const transactional = binding.effect === 'coordination-write' && !domainOwnsWriteTransaction;
      const outer = transactional ? beginWrite(db) : undefined;
      const beforeSequence = binding.effect === 'read' ? undefined : workspaceEventHighWater(db, workspace);
      let exitCode: number;
      try {
        exitCode = await executeDomainHandler(db, binding, handlerParams(params), dbPath, context.signal);
        if (exitCode !== 0) {
          outer?.rollback();
          return { payload: output.payload ?? null, exitCode };
        }
        if (beforeSequence !== undefined) {
          const eventWrite = outer ?? beginWrite(db);
          try {
            ensureCanonicalMutationEvent(db, {
              workspace, actorId: context.agentId, sessionId: context.sessionId,
              command: operation, beforeSequence, payload: { effect: binding.effect },
            });
            if (!outer) eventWrite.commit();
          } catch (error) {
            if (!outer) eventWrite.rollback();
            throw error;
          }
        }
        outer?.commit();
      } catch (error) {
        outer?.rollback();
        throw error;
      } finally {
        db.close();
      }
      return {
        payload: output.payload ?? null,
        exitCode,
        ...(output.text ? { text: output.text } : {}),
        ...(output.diagnostics.length ? { diagnostics: output.diagnostics } : {}),
      };
    } catch (error) {
      const cancelled = context.signal?.aborted === true
        && (error === context.signal.reason || (error instanceof Error && error.name === 'AbortError'));
      return {
        exitCode: 1,
        payload: {
          ok: false,
          operation: command,
          error: error instanceof HistoryError ? { code: error.code, message: error.message }
            : error instanceof Error ? error.message : String(error),
        },
        ...(cancelled ? { cancelled: true } : {}),
      };
    }
  });
}

interface AttendDetail {
  partial?: boolean;
  partial_reasons?: string[];
  workboard?: Record<string, Array<Record<string, unknown>>>;
  counts?: Record<string, number>;
  operational_state?: { unavailable?: unknown[]; context?: { pressure?: string } };
  next?: { continuations?: Array<{ command?: string; params?: Record<string, unknown> }> };
}

function itemSummary(row: Record<string, unknown>) {
  const id = text(row.id ?? row.run_id ?? row.task_id ?? row.signal_id, 128);
  const title = text(row.title, 100);
  const detail = text(row.detail ?? row.body, 160);
  const actorId = text(row.agent_id ?? row.actor_id, 128);
  const path = text(row.path ?? row.file_path, 240);
  const status = text(row.status, 32);
  return {
    ...(id ? { id } : {}), ...(title ? { title } : {}), ...(detail ? { detail } : {}),
    ...(actorId ? { actorId } : {}), ...(path ? { path } : {}), ...(status ? { status } : {}),
    ...(row.locked === true ? { locked: true } : {}),
  };
}

/** One read transaction; the not-modified branch stops after the event high-water query. */
export async function executeContextOrient(
  context: CanonicalExecutionContext,
  input: OrientParams = {},
): Promise<AwarenessOperationResult> {
  const limit = input.limit ?? 3;
  const offset = input.offset ?? 0;
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 3) throw new Error('context.orient limit must be an integer from 1 to 3');
  if (!Number.isSafeInteger(offset) || offset < 0) throw new Error('context.orient offset must be a non-negative integer');
  const workspace = resolve(context.workspace);
  const scope = storageScopeForOperation('context.orient', workspace, context.scope);
  const dbPath = resolveDbPath(context.database, { scope, workspace });
  const db = connectDb(dbPath);
  db.exec('BEGIN');
  try {
    const workspaces = repositoryWorkspacePaths(workspace);
    const highWater = db.prepare(`SELECT COALESCE(MAX(sequence), 0) AS sequence
      FROM event_outbox WHERE workspace_path IN (SELECT value FROM json_each(?))`)
      .get(JSON.stringify(workspaces)) as { sequence: number | bigint };
    const revision = `o2.${hash({ sequence: String(highWater.sequence), workspaces, agentId: context.agentId,
      sessionId: context.sessionId, limit, offset, file: input.file, query: input.query })}`;
    if (input.if_revision === revision) {
      db.exec('COMMIT');
      return { exitCode: 0, payload: { revision, unchanged: true } };
    }

    const detail = attendWorkspace(db, {
      details: true,
      limit,
      ...(input.file === undefined ? {} : { file: input.file }),
      ...(input.query === undefined ? {} : { query: input.query }),
      agentId: context.agentId,
      workspacePath: workspace,
      compact: true,
    }) as AttendDetail;
    const presence = attendWorkspace(db, {
      limit, offset, agentId: context.agentId, workspacePath: workspace, compact: true,
    }) as { peers?: Array<Record<string, unknown>>; partial?: boolean; partialReasons?: string[] };
    const peers = asRows(presence.peers).map(peer => ({
      actorId: String(peer.agent_id ?? ''),
      ...(text(peer.agent_name, 80) ? { name: text(peer.agent_name, 80) } : {}),
      ...(text(peer.status, 24) ? { status: text(peer.status, 24) } : {}),
      ...(text(peer.last_seen_at, 40) ? { lastSeenAt: text(peer.last_seen_at, 40) } : {}),
    }));
    const board = detail.workboard ?? {};
    const claimed = asRows(board.Claimed);
    const files = asRows(board.FilesUnderWork);
    const inbox = asRows(board.Inbox).slice(0, 3).map(itemSummary);
    const owned = claimed.find(row => String(row.agent_id ?? '') === context.agentId);
    const overlaps = files.filter(row => {
      const agents = Array.isArray(row.agents) ? row.agents.map(String) : [];
      return agents.some(agent => agent !== context.agentId)
        || (row.locked === true && String(row.lock_agent ?? '') !== context.agentId);
    }).slice(0, 3).map(itemSummary);
    const verifyRows = asRows(board.Verify);
    const partialReasons = [...new Set([...(presence.partialReasons ?? []), ...(detail.partial_reasons ?? [])])];
    const next: Array<Record<string, unknown>> = [];
    if (presence.partial === true) next.push({ operation: 'context.orient', params: { limit, offset: offset + peers.length } });
    if (detail.partial === true) {
      const continuations = detail.next?.continuations ?? [];
      if (!continuations.length) throw new Error('partial orientation detail is missing an executable continuation');
      next.push(...continuations);
    }
    const unavailable = detail.operational_state?.unavailable ?? [];
    const handoff = inbox.find(message => message.title?.toLowerCase().includes('handoff'));
    const payload: Record<string, unknown> = {
      revision,
      unchanged: false,
      self: { actorId: context.agentId, ...(context.sessionId ? { sessionId: context.sessionId } : {}) },
      peers: { items: peers, partial: presence.partial === true },
      work: { ...(owned ? { owned: itemSummary(owned) } : {}), overlaps },
      inbox,
      verification: { pending: Number(detail.counts?.Verify ?? verifyRows.length), stale: verifyRows.filter(row => row.stale === true || row.stale_file === true).length },
      ...(handoff ? { continuation: handoff } : {}),
      ...(unavailable.length ? { recovery: { degraded: true, pressure: text(detail.operational_state?.context?.pressure, 80) } } : {}),
      next,
      partial: presence.partial === true || detail.partial === true,
      partialReasons,
    };
    db.exec('COMMIT');
    if (context.insightProvider) {
      const suggested = await context.insightProvider.suggest({
        workspace, agentId: context.agentId, overlaps, limit: 3,
      });
      const candidates = suggested.slice(0, 3).flatMap(candidate => {
        const summary = text(candidate.summary, 160);
        const attribution = text(candidate.attribution, 80);
        if (!summary || !attribution || !Number.isFinite(candidate.confidence)) return [];
        return [{
          summary, attribution, confidence: Math.max(0, Math.min(1, candidate.confidence)),
          ...(text(candidate.path, 240) ? { path: text(candidate.path, 240) } : {}),
        }];
      });
      if (candidates.length) payload.insights = { advisory: true, candidates };
    }
    return { exitCode: 0, payload };
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction already ended */ }
    throw error;
  } finally {
    db.close();
  }
}
