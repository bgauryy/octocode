import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { attendWorkspace } from './attend-presence.js';
import type { OperationalState, Regulation } from './attend-physiology.js';
import {
  assessContextRegulation,
  observeContext,
  recordContextFeedback,
  type ContextObservation,
  type ContextFeedback,
} from './context-regulation.js';
import { interpretRunState, selectContextNudge } from './context-state.js';
import { connectDb, resolveDbPath } from './db-runtime.js';
import { normalizeWorkspacePath, repositoryWorkspacePaths } from './git.js';
import { storageScopeForOperation } from './workspace-policy.js';
import type { CanonicalExecutionContext, AwarenessOperationResult } from './operation-contracts.js';
import type { AwarenessOperationCall } from './schema/operation-types.js';
import { getKnowledgeBriefing } from './knowledge-memory.js';

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
  flow?: string;
  failure_signature?: string;
}

interface AttendDetail {
  partial?: boolean;
  partial_reasons?: string[];
  workboard?: Record<string, Array<Record<string, unknown>>>;
  counts?: Record<string, number>;
  operational_state?: OperationalState;
  regulation?: Regulation;
  next?: { operation?: AwarenessOperationCall; continuations?: AwarenessOperationCall[] };
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

/** One read transaction; revision reuse also checks session sensor freshness. */
export async function executeContextOrient(
  context: CanonicalExecutionContext,
  input: OrientParams = {},
): Promise<AwarenessOperationResult> {
  const limit = input.limit ?? 3;
  const offset = input.offset ?? 0;
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 3) throw new Error('context.orient limit must be an integer from 1 to 3');
  if (!Number.isSafeInteger(offset) || offset < 0) throw new Error('context.orient offset must be a non-negative integer');
  const workspace = normalizeWorkspacePath(context.workspace, context.workspace) ?? resolve(context.workspace);
  const scope = storageScopeForOperation('context.orient', workspace, context.scope);
  const dbPath = resolveDbPath(context.database, { scope, workspace });
  const db = connectDb(dbPath);
  const memoryPath = resolveDbPath(context.database, {
    scope: storageScopeForOperation('memory.get', workspace, context.scope), workspace,
  });
  let memoryDb: ReturnType<typeof connectDb> | undefined;
  db.exec('BEGIN');
  try {
    const wantsKnowledge = input.file !== undefined || input.query !== undefined
      || input.flow !== undefined || input.failure_signature !== undefined;
    if (wantsKnowledge) memoryDb = memoryPath === dbPath ? db : connectDb(memoryPath);
    const knowledge = memoryDb ? await getKnowledgeBriefing(memoryDb, {
      workspace, actorId: context.agentId, sessionId: context.sessionId,
    }, {
      files: typeof input.file === 'string' ? [input.file] : input.file,
      query: input.query, flow: input.flow, failure: input.failure_signature, limit,
    }) : undefined;
    const workspaces = repositoryWorkspacePaths(workspace);
    const highWater = db.prepare(`SELECT COALESCE(MAX(sequence), 0) AS sequence
      FROM event_outbox WHERE workspace_path IN (SELECT value FROM json_each(?))`)
      .get(JSON.stringify(workspaces)) as { sequence: number | bigint };
    const runtime = context.sessionId ? assessContextRegulation(db, {
      workspace, actorId: context.agentId, sessionId: context.sessionId,
    }) : undefined;
    const { advisories = [], run_state = interpretRunState(false, []), ...runtimeState } = runtime ?? {};
    const nudge = selectContextNudge(advisories);
    const revision = `o2.${hash({ version: 4, sequence: String(highWater.sequence), workspaces, agentId: context.agentId,
      runtimeFreshness: runtime?.freshness,
      sessionId: context.sessionId, limit, offset, file: input.file, query: input.query,
      flow: input.flow, failure: input.failure_signature, knowledge })}`;
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
    const partialReasons = [...new Set([...(presence.partialReasons ?? []), ...(detail.partial_reasons ?? []),
      ...(knowledge?.partial ? ['knowledge_partial'] : [])])];
    const next: Array<Record<string, unknown>> = [];
    if (presence.partial === true) next.push({ operation: 'context.orient', params: {
      limit, offset: offset + peers.length,
      ...(input.file === undefined ? {} : { file: input.file }),
      ...(input.query === undefined ? {} : { query: input.query }),
      ...(input.flow === undefined ? {} : { flow: input.flow }),
      ...(input.failure_signature === undefined ? {} : { failure_signature: input.failure_signature }),
    } });
    if (detail.partial === true) {
      const continuations = detail.next?.continuations ?? [];
      if (!continuations.length) throw new Error('partial orientation detail is missing an executable continuation');
      next.push(...continuations);
    }
    if (knowledge?.next) next.push(knowledge.next.call);
    const handoff = inbox.find(message => message.title?.toLowerCase().includes('handoff'));
    const payload: Record<string, unknown> = {
      revision,
      unchanged: false,
      self: { actorId: context.agentId, ...(context.sessionId ? { sessionId: context.sessionId } : {}) },
      run_state,
      peers: { items: peers, partial: presence.partial === true },
      work: { ...(owned ? { owned: itemSummary(owned) } : {}), overlaps },
      inbox,
      verification: { pending: Number(detail.counts?.Verify ?? verifyRows.length), stale: verifyRows.filter(row => row.stale === true || row.stale_file === true).length },
      ...(handoff ? { continuation: handoff } : {}),
      operational: {
        unavailable: (detail.operational_state?.unavailable ?? []).filter(sensor =>
          !(runtime?.freshness === 'fresh' && ['tool_health', 'context', 'repetition'].includes(sensor) && !runtime.unavailable.includes(sensor))),
        ...(runtime?.observed_at ? { runtime: runtimeState } : {}),
      },
      regulation: { ...(detail.regulation ?? { advisory: true, actions: [] }),
        ...(advisories.length ? { advisories } : {}),
        ...(nudge ? { nudge } : {}),
        ...(detail.next?.operation ? { next: detail.next.operation } : {}),
      },
      next,
      ...(knowledge ? { knowledge: { advisory: true, ...knowledge } } : {}),
      partial: presence.partial === true || detail.partial === true || knowledge?.partial === true,
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
    if (memoryDb && memoryDb !== db) memoryDb.close();
    db.close();
  }
}

/** Explicit reports share one durable domain boundary for CLI and API. */
export async function executeContextReport(
  operation: 'context.observe' | 'context.feedback', context: CanonicalExecutionContext,
  input: ContextObservation | ContextFeedback,
): Promise<AwarenessOperationResult> {
  if (!context.sessionId?.trim()) throw new Error(`${operation} requires a stable session_id`);
  context.signal?.throwIfAborted();
  const workspace = normalizeWorkspacePath(context.workspace, context.workspace) ?? resolve(context.workspace);
  const scope = storageScopeForOperation(operation, workspace, context.scope);
  const db = connectDb(resolveDbPath(context.database, { scope, workspace }));
  const binding = { workspace, actorId: context.agentId, sessionId: context.sessionId };
  try {
    db.exec('BEGIN IMMEDIATE');
    const receipt = operation === 'context.observe'
      ? observeContext(db, binding, input as ContextObservation)
      : recordContextFeedback(db, binding, input as ContextFeedback);
    db.exec('COMMIT');
    return { exitCode: 0, payload: { ok: true, ...receipt } };
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not begin */ }
    throw error;
  } finally { db.close(); }
}
