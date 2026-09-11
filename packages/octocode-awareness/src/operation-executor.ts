import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { z } from 'zod';
import { attendWorkspace } from './attend-presence.js';
import { runDatabaseCommandHandler } from './command-dispatch.js';
import { runLockCommand } from './command-locks.js';
import { structuredAwarenessContinuations } from './command-continuations.js';
import {
  AwarenessInputError,
  commandOutput,
  type AwarenessCommandOutput,
} from './command-output.js';
import type { AwarenessCommandResult } from './command-api.js';
import { COMMAND_ROUTES } from './commands/routes.js';
import type { ParsedArgs } from './commands/args.js';
import { connectDb, resolveDbPath } from './db-runtime.js';
import { normalizeWorkspacePath, repositoryWorkspacePaths } from './git.js';
import { HistoryError } from './history-store.js';
import { getAwarenessCommandDescriptor } from './schema/cli.js';
import { commandSchemaProperties } from './schema/command-properties.js';
import { storageScopeForCommand } from './workspace-policy.js';

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

interface CanonicalExecutionContext {
  database?: string;
  workspace: string;
  agentId: string;
  sessionId?: string;
  scope?: import('./storage-scope.js').AwarenessStorageScope;
  signal?: AbortSignal;
}

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

/** Canonical operation execution path: no argv and no legacy command API. */
export async function executeCanonicalCommand(
  command: string,
  input: Record<string, unknown>,
  context: CanonicalExecutionContext,
): Promise<AwarenessCommandResult> {
  const output: AwarenessCommandOutput = { command, compact: true, text: '', diagnostics: [] };
  return commandOutput.run(output, async () => {
    try {
      context.signal?.throwIfAborted();
      const descriptor = getAwarenessCommandDescriptor(command);
      if (!descriptor) throw new AwarenessInputError(`Unknown Awareness command route: ${command}`);
      const params = { ...input };
      const properties = commandSchemaProperties(descriptor.inputSchema);
      bindHost(params, properties, 'workspace', context.workspace);
      if (descriptor.injected.includes('agent-id')) {
        bindHost(params, properties, Object.hasOwn(properties, 'agent_id') ? 'agent_id' : 'lead_agent_id', context.agentId);
      }
      bindHost(params, properties, 'session_id', context.sessionId);
      validate(command, params, descriptor.inputSchema as Record<string, unknown>);
      const workspace = resolve(context.workspace);
      const route = COMMAND_ROUTES[command];
      const scope = storageScopeForCommand(route?.command ?? command, workspace, context.scope);
      const dbPath = resolveDbPath(context.database, { scope, workspace });
      const db = connectDb(dbPath);
      let exitCode: number;
      try {
        const args = handlerParams(params);
        if (route?.action) args.action = route.action;
        const [noun, action] = command.split(' ');
        if (noun === 'query' && action) args.view = action;
        if (command === 'lock wait' || command === 'lock acquire') {
          exitCode = await runLockCommand(db, command, args, dbPath, { compact: true, cli: false }, context.signal);
        } else {
          exitCode = await runDatabaseCommandHandler(db, route?.command ?? noun!, args, dbPath, { compact: true, cli: false }, context.signal);
        }
      } finally {
        db.close();
      }
      return {
        payload: structuredAwarenessContinuations(output.payload ?? null),
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
): Promise<AwarenessCommandResult> {
  const limit = input.limit ?? 3;
  const offset = input.offset ?? 0;
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 3) throw new Error('context.orient limit must be an integer from 1 to 3');
  if (!Number.isSafeInteger(offset) || offset < 0) throw new Error('context.orient offset must be a non-negative integer');
  const workspace = resolve(context.workspace);
  const scope = storageScopeForCommand('attend', workspace, context.scope);
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
    const next: Array<{ operation: string; params?: Record<string, unknown> }> = [];
    if (presence.partial === true) next.push({ operation: 'context.orient', params: { limit, offset: offset + peers.length } });
    const unavailable = detail.operational_state?.unavailable ?? [];
    const handoff = inbox.find(message => message.title?.toLowerCase().includes('handoff'));
    const payload = {
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
    return { exitCode: 0, payload };
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction already ended */ }
    throw error;
  } finally {
    db.close();
  }
}
