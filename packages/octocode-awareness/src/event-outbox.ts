import { isDeepStrictEqual } from 'node:util';
import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import {
  parseAgentEventEnvelopeV1,
  type AgentEventEnvelopeV1,
  type EventRetentionClass,
  type ParsedAgentEventEnvelopeV1,
} from './continuity-contracts.js';

export interface OutboxRow {
  sequence: number;
  event_id: string;
  workspace_path: string;
  event_type: string;
  schema_version: number;
  retention_class: EventRetentionClass;
  aggregate_kind: string | null;
  aggregate_id: string | null;
  aggregate_revision: string | null;
  actor_json: string;
  provenance_json: string;
  payload_json: string;
  session_id: string | null;
  correlation_id: string | null;
  created_at: string;
  expires_at: string | null;
}

export interface StoredOutboxEventV1<T = unknown> extends ParsedAgentEventEnvelopeV1<T> {
  sequence: number;
}

export interface OutboxEventPage<T = unknown> {
  events: StoredOutboxEventV1<T>[];
  next: { afterSequence: number } | null;
}

export interface DomainEventInput<T = unknown> {
  workspace: string;
  eventType: string;
  retentionClass: EventRetentionClass;
  actorId: string;
  actorKind?: 'agent' | 'hook' | 'system' | 'tool';
  source?: 'harness' | 'hook' | 'tool';
  aggregateKind?: string;
  aggregateId?: string;
  aggregateRevision?: string;
  sessionId?: string | null;
  correlationId?: string | null;
  createdAt: string;
  payload: T;
  eventId?: string;
  eventIdPrefix?: string;
}

export function workspaceEventHighWater(db: DatabaseSync, workspace: string): number {
  const row = db.prepare('SELECT COALESCE(MAX(sequence), 0) AS sequence FROM event_outbox WHERE workspace_path = ?')
    .get(workspace) as { sequence: number | bigint };
  return Number(row.sequence);
}

/**
 * Append a canonical fallback only when the domain mutation did not already
 * append a more specific event in this workspace.
 */
export function ensureCanonicalMutationEvent(db: DatabaseSync, params: {
  workspace: string;
  actorId: string;
  sessionId?: string | null;
  command: string;
  beforeSequence: number;
  payload?: Record<string, unknown>;
  createdAt?: string;
}): number {
  const current = workspaceEventHighWater(db, params.workspace);
  if (current > params.beforeSequence) return current;
  return appendDomainEvent(db, {
    workspace: params.workspace,
    eventType: `canonical.${params.command.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    retentionClass: 'operational',
    actorId: params.actorId,
    sessionId: params.sessionId,
    createdAt: params.createdAt ?? new Date().toISOString(),
    payload: params.payload ?? {},
    eventIdPrefix: 'cevt',
  });
}

/** Build and append one canonical domain event. The caller owns the transaction. */
export function appendDomainEvent<T>(db: DatabaseSync, input: DomainEventInput<T>): number {
  const prefix = input.eventIdPrefix ?? 'evt';
  return insertOutboxEvent(db, {
    version: 1,
    eventId: input.eventId ?? `${prefix}_${randomUUID().replace(/-/g, '')}`,
    workspace: input.workspace,
    type: input.eventType,
    retentionClass: input.retentionClass,
    actor: { kind: input.actorKind ?? 'agent', id: input.actorId },
    provenance: { source: input.source ?? 'tool', trust: 'attributed-data' },
    ...(input.aggregateKind && input.aggregateId ? { aggregate: {
      kind: input.aggregateKind,
      id: input.aggregateId,
      ...(input.aggregateRevision ? { revision: input.aggregateRevision } : {}),
    } } : {}),
    ...(input.sessionId ? { sessionId: input.sessionId } : {}),
    ...(input.correlationId ? { correlationId: input.correlationId } : {}),
    createdAt: input.createdAt,
    payload: input.payload,
  });
}

function domainEventName(domain: string, eventType: string): string {
  return `${domain}.${eventType.trim().toLowerCase().replace(/_/g, '-')}`;
}

export function appendTaskEvent(db: DatabaseSync, params: {
  taskId: string;
  runId: string | null;
  agentId: string;
  eventType: string;
  message: string;
  createdAt: string;
}): number {
  const task = db.prepare(`SELECT p.workspace_path, t.updated_at
    FROM awareness_tasks t JOIN awareness_plans p ON p.plan_id = t.plan_id
    WHERE t.task_id = ?`).get(params.taskId) as { workspace_path: string; updated_at: string } | undefined;
  if (!task) throw new Error(`cannot append task event for missing task ${params.taskId}`);
  return appendDomainEvent(db, {
    workspace: task.workspace_path,
    eventType: domainEventName('task', params.eventType),
    retentionClass: 'operational',
    actorId: params.agentId,
    aggregateKind: 'task',
    aggregateId: params.taskId,
    aggregateRevision: task.updated_at,
    correlationId: params.runId,
    createdAt: params.createdAt,
    payload: { event_type: params.eventType, message: params.message },
    eventIdPrefix: 'tevt',
  });
}

export function appendRunVerificationEvent(db: DatabaseSync, params: {
  runId: string;
  agentId: string;
  message: string;
  createdAt: string;
}): number {
  const run = db.prepare(`SELECT workspace_path, task_id, session_id, updated_at
    FROM task_runs WHERE run_id = ?`).get(params.runId) as {
      workspace_path: string | null;
      task_id: string | null;
      session_id: string | null;
      updated_at: string;
    } | undefined;
  if (!run?.workspace_path) throw new Error(`cannot append run event without a workspace for ${params.runId}`);
  return appendDomainEvent(db, {
    workspace: run.workspace_path,
    eventType: 'run.verified',
    retentionClass: 'audit',
    actorId: params.agentId,
    aggregateKind: 'run',
    aggregateId: params.runId,
    aggregateRevision: run.updated_at,
    sessionId: run.session_id,
    correlationId: run.task_id,
    createdAt: params.createdAt,
    payload: { event_type: 'VERIFIED', message: params.message },
    eventIdPrefix: 'revt',
  });
}

export function latestRunVerification(db: DatabaseSync, runId: string): {
  agent_id: string;
  message: string;
  created_at: string;
} | null {
  const row = db.prepare(`SELECT actor_json, payload_json, created_at
    FROM event_outbox WHERE aggregate_kind = 'run' AND aggregate_id = ?
      AND event_type = 'run.verified'
    ORDER BY sequence DESC LIMIT 1`).get(runId) as {
      actor_json: string;
      payload_json: string;
      created_at: string;
    } | undefined;
  if (!row) return null;
  const actor = JSON.parse(row.actor_json) as { id: string };
  const payload = JSON.parse(row.payload_json) as { message: string };
  return { agent_id: actor.id, message: payload.message, created_at: row.created_at };
}

export function outboxEventFromRow<T = unknown>(row: OutboxRow): StoredOutboxEventV1<T> {
  if (row.schema_version !== 1) throw new Error(`unsupported event schema_version ${row.schema_version}`);
  return { ...parseAgentEventEnvelopeV1<T>({
    version: 1,
    eventId: row.event_id,
    workspace: row.workspace_path,
    type: row.event_type,
    retentionClass: row.retention_class,
    actor: JSON.parse(row.actor_json) as unknown,
    provenance: JSON.parse(row.provenance_json) as unknown,
    ...(row.aggregate_kind && row.aggregate_id ? { aggregate: {
      kind: row.aggregate_kind,
      id: row.aggregate_id,
      ...(row.aggregate_revision ? { revision: row.aggregate_revision } : {}),
    } } : {}),
    ...(row.session_id ? { sessionId: row.session_id } : {}),
    ...(row.correlation_id ? { correlationId: row.correlation_id } : {}),
    createdAt: row.created_at,
    ...(row.expires_at ? { expiresAt: row.expires_at } : {}),
    payload: JSON.parse(row.payload_json) as T,
  }), sequence: row.sequence };
}

/**
 * Append one validated continuity event inside the caller's durable write
 * boundary. The caller owns its transaction so a domain row and its delivery
 * event commit together; nested callers do not begin a second transaction.
 */
export function insertOutboxEvent(db: DatabaseSync, input: AgentEventEnvelopeV1): number {
  const event = parseAgentEventEnvelopeV1(input);
  const result = db.prepare(`INSERT INTO event_outbox(
    event_id, workspace_path, event_type, schema_version, retention_class,
    aggregate_kind, aggregate_id, aggregate_revision,
    actor_json, provenance_json, payload_json, session_id, correlation_id, created_at, expires_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  ON CONFLICT(event_id) DO NOTHING`).run(
    event.eventId,
    event.workspace,
    event.type,
    event.version,
    event.retentionClass,
    event.aggregate?.kind ?? null,
    event.aggregate?.id ?? null,
    event.aggregate?.revision ?? null,
    JSON.stringify(event.actor),
    JSON.stringify(event.provenance),
    JSON.stringify(event.payload),
    event.sessionId ?? null,
    event.correlationId ?? null,
    event.createdAt,
    event.expiresAt ?? null,
  ) as { changes: number; lastInsertRowid: number | bigint };
  if (result.changes > 0) return Number(result.lastInsertRowid);

  const existing = db.prepare('SELECT * FROM event_outbox WHERE event_id = ?')
    .get(event.eventId) as Record<string, unknown> | undefined;
  if (!existing) throw new Error(`outbox insert failed for ${event.eventId}`);
  const expected: Record<string, unknown> = {
    workspace_path: event.workspace,
    event_type: event.type,
    schema_version: event.version,
    retention_class: event.retentionClass,
    aggregate_kind: event.aggregate?.kind ?? null,
    aggregate_id: event.aggregate?.id ?? null,
    aggregate_revision: event.aggregate?.revision ?? null,
    actor_json: event.actor,
    provenance_json: event.provenance,
    payload_json: event.payload,
    session_id: event.sessionId ?? null,
    correlation_id: event.correlationId ?? null,
    created_at: event.createdAt,
    expires_at: event.expiresAt ?? null,
  };
  for (const [key, value] of Object.entries(expected)) {
    const actual = key.endsWith('_json') ? JSON.parse(String(existing[key])) as unknown : existing[key];
    if (!isDeepStrictEqual(actual, value)) throw new Error(`event ID conflict: ${event.eventId}`);
  }
  return Number(existing.sequence);
}

/** Bounded ascending replay. `next` is directly executable and lossless. */
export function listOutboxEvents<T = unknown>(db: DatabaseSync, params: {
  workspace: string;
  afterSequence?: number;
  limit?: number;
  eventType?: string;
  retentionClass?: EventRetentionClass;
}): OutboxEventPage<T> {
  const workspace = params.workspace.trim();
  if (!workspace) throw new Error('workspace is required');
  const afterSequence = params.afterSequence ?? 0;
  if (!Number.isSafeInteger(afterSequence) || afterSequence < 0) throw new Error('afterSequence must be a non-negative integer');
  const limit = Math.min(Math.max(params.limit ?? 100, 1), 1000);
  const where = ['workspace_path = ?', 'sequence > ?'];
  const bindings: Array<string | number> = [workspace, afterSequence];
  if (params.eventType !== undefined) {
    where.push('event_type = ?');
    bindings.push(params.eventType);
  }
  if (params.retentionClass !== undefined) {
    if (!['delivery', 'operational', 'audit'].includes(params.retentionClass)) throw new Error('retentionClass is invalid');
    where.push('retention_class = ?');
    bindings.push(params.retentionClass);
  }
  const rows = db.prepare(`SELECT * FROM event_outbox
    WHERE ${where.join(' AND ')} ORDER BY sequence ASC LIMIT ?`)
    .all(...bindings, limit + 1) as unknown as OutboxRow[];
  const hasMore = rows.length > limit;
  const events = rows.slice(0, limit).map((row) => outboxEventFromRow<T>(row));
  return {
    events,
    next: hasMore && events.length > 0 ? { afterSequence: events.at(-1)!.sequence } : null,
  };
}
