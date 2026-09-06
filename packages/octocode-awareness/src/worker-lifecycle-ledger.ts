import path from 'node:path';
import type { DatabaseSync } from '@octocodeai/octocode-shared/sqlite';
import { withSqliteBusyRetry } from '@octocodeai/octocode-shared/sqlite';
import { containsSecretLikeText } from './memory-hardening.js';

export type WorkerLifecycleRedaction = 'public' | 'sensitive' | 'secret' | 'internal';
export type WorkerLifecycleJsonValue =
  | null
  | boolean
  | number
  | string
  | readonly WorkerLifecycleJsonValue[]
  | { readonly [key: string]: WorkerLifecycleJsonValue };

export interface WorkerLifecycleEventInput {
  readonly packetId: string;
  readonly workspace: string;
  readonly sessionId: string;
  readonly workerId: string;
  readonly correlationId: string;
  readonly type: string;
  readonly redaction: WorkerLifecycleRedaction;
  readonly createdAt: string;
  readonly payload: WorkerLifecycleJsonValue;
}

export interface StoredWorkerLifecycleEvent extends WorkerLifecycleEventInput {
  readonly sequence: number;
}

export interface AppendWorkerLifecycleEventResult {
  readonly event: StoredWorkerLifecycleEvent;
  readonly duplicate: boolean;
}

export interface ListWorkerLifecycleEventsOptions {
  readonly workspace: string;
  readonly sessionId: string;
  readonly workerId?: string;
  readonly correlationId?: string;
  readonly type?: string;
  readonly afterSequence?: number;
  readonly limit?: number;
}

interface WorkerLifecycleRow {
  sequence: number | bigint;
  packet_id: string;
  workspace_path: string;
  session_id: string;
  worker_id: string;
  correlation_id: string;
  event_type: string;
  redaction: WorkerLifecycleRedaction;
  created_at: string;
  payload_json: string;
}

const MAX_IDENTIFIER_BYTES = 512;
const MAX_WORKSPACE_BYTES = 4096;
const MAX_EVENT_TYPE_BYTES = 128;
export const MAX_WORKER_LIFECYCLE_PAYLOAD_BYTES = 64 * 1024;
export const MAX_WORKER_LIFECYCLE_REPLAY_LIMIT = 1_000;
const REDACTIONS = new Set<WorkerLifecycleRedaction>(['public', 'sensitive', 'secret', 'internal']);

export const WORKER_LIFECYCLE_DDL = `
  CREATE TABLE IF NOT EXISTS worker_lifecycle_events (
    sequence        INTEGER PRIMARY KEY AUTOINCREMENT,
    packet_id       TEXT NOT NULL UNIQUE,
    workspace_path  TEXT NOT NULL,
    session_id      TEXT NOT NULL,
    worker_id       TEXT NOT NULL,
    correlation_id  TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    redaction       TEXT NOT NULL CHECK(redaction IN ('public','sensitive','secret','internal')),
    created_at      TEXT NOT NULL,
    payload_json    TEXT NOT NULL,
    recorded_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
  );
  CREATE INDEX IF NOT EXISTS idx_worker_lifecycle_scope_sequence
    ON worker_lifecycle_events(workspace_path, session_id, sequence);
  CREATE INDEX IF NOT EXISTS idx_worker_lifecycle_worker_sequence
    ON worker_lifecycle_events(workspace_path, session_id, worker_id, sequence);
  CREATE INDEX IF NOT EXISTS idx_worker_lifecycle_correlation_sequence
    ON worker_lifecycle_events(workspace_path, session_id, correlation_id, sequence);
`;

function boundedText(value: unknown, label: string, maxBytes: number): string {
  if (typeof value !== 'string') throw new Error(`${label} must be a string`);
  const normalized = value.trim();
  if (normalized.length === 0) throw new Error(`${label} must not be empty`);
  if (/\0|[\u0001-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(normalized)) {
    throw new Error(`${label} contains unsupported control characters`);
  }
  if (Buffer.byteLength(normalized, 'utf8') > maxBytes) throw new Error(`${label} exceeds ${maxBytes} bytes`);
  return normalized;
}

function isoTimestamp(value: unknown): string {
  const raw = boundedText(value, 'createdAt', 64);
  const milliseconds = Date.parse(raw);
  if (!Number.isFinite(milliseconds)) throw new Error('createdAt must be a valid ISO timestamp');
  return new Date(milliseconds).toISOString();
}

function canonicalJson(value: unknown, ancestors = new Set<object>()): WorkerLifecycleJsonValue {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error('payload must be JSON-safe');
    return value;
  }
  if (typeof value !== 'object') throw new Error('payload must be JSON-safe');
  if (ancestors.has(value)) throw new Error('payload must be JSON-safe and acyclic');
  ancestors.add(value);
  try {
    if (Array.isArray(value)) return value.map((entry) => canonicalJson(entry, ancestors));
    const prototype = Object.getPrototypeOf(value);
    if (prototype !== Object.prototype && prototype !== null) throw new Error('payload must be a JSON-safe plain object');
    const result: Record<string, WorkerLifecycleJsonValue> = {};
    for (const key of Object.keys(value).sort()) {
      if (key.length === 0) throw new Error('payload object keys must not be empty');
      result[key] = canonicalJson((value as Record<string, unknown>)[key], ancestors);
    }
    return result;
  } finally {
    ancestors.delete(value);
  }
}

function normalizeInput(input: WorkerLifecycleEventInput): WorkerLifecycleEventInput & { payloadJson: string } {
  if (typeof input !== 'object' || input === null || Array.isArray(input)) throw new Error('worker lifecycle event must be an object');
  const packetId = boundedText(input.packetId, 'packetId', MAX_IDENTIFIER_BYTES);
  const workspace = path.resolve(boundedText(input.workspace, 'workspace', MAX_WORKSPACE_BYTES));
  const sessionId = boundedText(input.sessionId, 'sessionId', MAX_IDENTIFIER_BYTES);
  const workerId = boundedText(input.workerId, 'workerId', MAX_IDENTIFIER_BYTES);
  const correlationId = boundedText(input.correlationId, 'correlationId', MAX_IDENTIFIER_BYTES);
  const type = boundedText(input.type, 'type', MAX_EVENT_TYPE_BYTES);
  if (!REDACTIONS.has(input.redaction)) throw new Error('redaction must be public, sensitive, secret, or internal');
  const createdAt = isoTimestamp(input.createdAt);
  const payload = canonicalJson(input.payload);
  const payloadJson = JSON.stringify(payload);
  if (Buffer.byteLength(payloadJson, 'utf8') > MAX_WORKER_LIFECYCLE_PAYLOAD_BYTES) {
    throw new Error(`payload exceeds ${MAX_WORKER_LIFECYCLE_PAYLOAD_BYTES} bytes`);
  }
  const secretScanText = `${packetId}\n${sessionId}\n${workerId}\n${correlationId}\n${type}\n${payloadJson.replace(/["{}\[\],]/g, ' ')}`;
  if (containsSecretLikeText(secretScanText)) {
    throw new Error('worker lifecycle event rejected: secret-like content must never enter the durable ledger');
  }
  return { packetId, workspace, sessionId, workerId, correlationId, type, redaction: input.redaction, createdAt, payload, payloadJson };
}

function fromRow(row: WorkerLifecycleRow): StoredWorkerLifecycleEvent {
  let payload: WorkerLifecycleJsonValue;
  try {
    payload = canonicalJson(JSON.parse(row.payload_json));
  } catch {
    throw new Error(`worker lifecycle ledger contains malformed payload for packetId ${row.packet_id}`);
  }
  return {
    sequence: Number(row.sequence),
    packetId: row.packet_id,
    workspace: row.workspace_path,
    sessionId: row.session_id,
    workerId: row.worker_id,
    correlationId: row.correlation_id,
    type: row.event_type,
    redaction: row.redaction,
    createdAt: row.created_at,
    payload,
  };
}

function sameEvent(stored: StoredWorkerLifecycleEvent, input: ReturnType<typeof normalizeInput>): boolean {
  return stored.packetId === input.packetId
    && stored.workspace === input.workspace
    && stored.sessionId === input.sessionId
    && stored.workerId === input.workerId
    && stored.correlationId === input.correlationId
    && stored.type === input.type
    && stored.redaction === input.redaction
    && stored.createdAt === input.createdAt
    && JSON.stringify(stored.payload) === input.payloadJson;
}

function initializeWorkerLifecycleLedger(db: DatabaseSync): void {
  withSqliteBusyRetry(() => db.exec(WORKER_LIFECYCLE_DDL));
}

export function appendWorkerLifecycleEvent(
  db: DatabaseSync,
  input: WorkerLifecycleEventInput,
): AppendWorkerLifecycleEventResult {
  const normalized = normalizeInput(input);
  initializeWorkerLifecycleLedger(db);
  return withSqliteBusyRetry(() => {
    db.exec('BEGIN IMMEDIATE');
    try {
      let row = db.prepare('SELECT * FROM worker_lifecycle_events WHERE packet_id = ?')
        .get(normalized.packetId) as WorkerLifecycleRow | undefined;
      const duplicate = row !== undefined;
      if (row === undefined) {
        db.prepare(`INSERT INTO worker_lifecycle_events(
          packet_id, workspace_path, session_id, worker_id, correlation_id, event_type, redaction, created_at, payload_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`).run(
          normalized.packetId,
          normalized.workspace,
          normalized.sessionId,
          normalized.workerId,
          normalized.correlationId,
          normalized.type,
          normalized.redaction,
          normalized.createdAt,
          normalized.payloadJson,
        );
        row = db.prepare('SELECT * FROM worker_lifecycle_events WHERE packet_id = ?')
          .get(normalized.packetId) as WorkerLifecycleRow | undefined;
      }
      if (row === undefined) throw new Error('worker lifecycle event was not persisted');
      const event = fromRow(row);
      if (duplicate && !sameEvent(event, normalized)) {
        throw new Error(`packetId ${normalized.packetId} already identifies a different worker lifecycle event`);
      }
      db.exec('COMMIT');
      return { event, duplicate };
    } catch (error) {
      try { db.exec('ROLLBACK'); } catch { /* Preserve the original failure. */ }
      throw error;
    }
  });
}

export function listWorkerLifecycleEvents(
  db: DatabaseSync,
  options: ListWorkerLifecycleEventsOptions,
): StoredWorkerLifecycleEvent[] {
  if (typeof options !== 'object' || options === null || Array.isArray(options)) throw new Error('worker lifecycle replay options must be an object');
  const workspace = path.resolve(boundedText(options.workspace, 'workspace', MAX_WORKSPACE_BYTES));
  const sessionId = boundedText(options.sessionId, 'sessionId', MAX_IDENTIFIER_BYTES);
  const clauses = ['workspace_path = ?', 'session_id = ?'];
  const values: Array<string | number> = [workspace, sessionId];
  if (options.workerId !== undefined) {
    clauses.push('worker_id = ?');
    values.push(boundedText(options.workerId, 'workerId', MAX_IDENTIFIER_BYTES));
  }
  if (options.correlationId !== undefined) {
    clauses.push('correlation_id = ?');
    values.push(boundedText(options.correlationId, 'correlationId', MAX_IDENTIFIER_BYTES));
  }
  if (options.type !== undefined) {
    clauses.push('event_type = ?');
    values.push(boundedText(options.type, 'type', MAX_EVENT_TYPE_BYTES));
  }
  const afterSequence = options.afterSequence ?? 0;
  if (!Number.isSafeInteger(afterSequence) || afterSequence < 0) throw new Error('afterSequence must be a non-negative safe integer');
  clauses.push('sequence > ?');
  values.push(afterSequence);
  const limit = options.limit ?? 100;
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > MAX_WORKER_LIFECYCLE_REPLAY_LIMIT) {
    throw new Error(`limit must be an integer in 1..${MAX_WORKER_LIFECYCLE_REPLAY_LIMIT}`);
  }
  values.push(limit);
  initializeWorkerLifecycleLedger(db);
  const rows = db.prepare(`SELECT * FROM worker_lifecycle_events
    WHERE ${clauses.join(' AND ')} ORDER BY sequence ASC LIMIT ?`).all(...values) as unknown as WorkerLifecycleRow[];
  return rows.map(fromRow);
}
