/**
 * Audit event projections for workspace edits and harness lifecycle events.
 */

import type { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { normalizeArtifact, utcNow } from './helpers.js';
import { appendDomainEvent } from './event-outbox.js';
import type { InsertEditLogParams, EditLogRow, QueryEditLogParams, InsertHarnessLogParams, HarnessLogRow, HarnessEventType } from './types/plans-docs.js';

// ─── sha256 helper ────────────────────────────────────────────────────────────

/** Hash a string with sha256, returns hex (for content_hash). */
export function sha256Hex(content: string): string {
  return createHash('sha256').update(content).digest('hex');
}

function eventWorkspace(db: DatabaseSync, params: {
  workspacePath?: string | null;
  runId?: string | null;
  sessionId?: string | null;
}): string {
  if (params.workspacePath?.trim()) return resolve(params.workspacePath);
  if (params.runId) {
    const run = db.prepare('SELECT workspace_path FROM task_runs WHERE run_id = ?')
      .get(params.runId) as { workspace_path: string | null } | undefined;
    if (run?.workspace_path) return run.workspace_path;
  }
  if (params.sessionId) {
    const session = db.prepare('SELECT workspace_path FROM sessions WHERE session_id = ?')
      .get(params.sessionId) as { workspace_path: string | null } | undefined;
    if (session?.workspace_path) return session.workspace_path;
  }
  return resolve(process.cwd());
}

function boundedLimit(limit: number | undefined): number {
  if (limit === undefined) return 1000;
  if (!Number.isSafeInteger(limit) || limit < 1) throw new Error('limit must be a positive integer');
  return Math.min(limit, 1000);
}

// ─── edit event projection ────────────────────────────────────────────────────

/** Record a single file edit. Returns an object with the generated editId. */
export function insertEditLog(db: DatabaseSync, params: InsertEditLogParams): { editId: string } {
  const editId = 'edit_' + randomUUID();
  const now = utcNow();
  appendDomainEvent(db, {
    workspace: eventWorkspace(db, params),
    eventType: `workspace.edit.${params.operation}`,
    retentionClass: 'audit',
    actorId: params.agentId,
    source: 'hook',
    aggregateKind: 'file',
    aggregateId: params.filePath,
    sessionId: params.sessionId,
    correlationId: params.runId,
    createdAt: now,
    payload: {
      old_file_path: params.oldFilePath ?? null,
      lines_added: params.linesAdded ?? null,
      lines_removed: params.linesRemoved ?? null,
      artifact: normalizeArtifact(params.artifact),
    },
    eventId: editId,
  });

  return { editId };
}

/** Query the edit log with optional filters. */
export function queryEditLog(db: DatabaseSync, params: QueryEditLogParams): EditLogRow[] {
  const conditions: string[] = ["event_type LIKE 'workspace.edit.%'"];
  const bindings: (string | number)[] = [];

  if (params.sessionId !== undefined) {
    conditions.push('session_id = ?');
    bindings.push(params.sessionId);
  }
  if (params.runId !== undefined) {
    conditions.push('correlation_id = ?');
    bindings.push(params.runId);
  }
  if (params.agentId !== undefined) {
    conditions.push("json_extract(actor_json, '$.id') = ?");
    bindings.push(params.agentId);
  }
  if (params.filePath !== undefined) {
    conditions.push('aggregate_id = ?');
    bindings.push(params.filePath);
  }
  if (params.workspacePath !== undefined) {
    conditions.push('workspace_path = ?');
    bindings.push(params.workspacePath);
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact !== null) {
    conditions.push("json_extract(payload_json, '$.artifact') = ?");
    bindings.push(artifact);
  }
  if (params.operation !== undefined) {
    conditions.push('event_type = ?');
    bindings.push(`workspace.edit.${params.operation}`);
  }
  if (params.since !== undefined) {
    conditions.push('created_at >= ?');
    bindings.push(params.since);
  }

  const sql = `SELECT event_id AS edit_id, session_id, correlation_id AS run_id,
      json_extract(actor_json, '$.id') AS agent_id, aggregate_id AS file_path,
      substr(event_type, 16) AS operation,
      json_extract(payload_json, '$.old_file_path') AS old_file_path,
      json_extract(payload_json, '$.lines_added') AS lines_added,
      json_extract(payload_json, '$.lines_removed') AS lines_removed,
      NULL AS content_hash, workspace_path,
      json_extract(payload_json, '$.artifact') AS artifact, created_at
    FROM event_outbox WHERE ${conditions.join(' AND ')}
    ORDER BY sequence DESC LIMIT ?`;
  return db.prepare(sql).all(...bindings, boundedLimit(params.limit)) as unknown as EditLogRow[];
}

// ─── harness event projection ─────────────────────────────────────────────────

/** Record a harness lifecycle event. Returns the harness_id. */
export function insertHarnessLog(db: DatabaseSync, params: InsertHarnessLogParams): string {
  const harnessId = 'harness_' + randomUUID();
  const now = utcNow();
  if (params.sessionId && !db.prepare('SELECT 1 FROM sessions WHERE session_id = ?').get(params.sessionId)) {
    throw new Error(`unknown session ${params.sessionId}`);
  }
  if (params.memoryId && !db.prepare('SELECT 1 FROM awareness_memories WHERE memory_id = ?').get(params.memoryId)) {
    throw new Error(`unknown memory ${params.memoryId}`);
  }
  if (params.runId && !db.prepare('SELECT 1 FROM task_runs WHERE run_id = ?').get(params.runId)) {
    throw new Error(`unknown run ${params.runId}`);
  }
  appendDomainEvent(db, {
    workspace: eventWorkspace(db, params),
    eventType: `harness.${params.eventType}`,
    retentionClass: 'audit',
    actorId: params.agentId,
    source: 'harness',
    aggregateKind: 'harness',
    aggregateId: harnessId,
    sessionId: params.sessionId,
    correlationId: params.runId,
    createdAt: now,
    payload: {
      data: params.payload ?? null,
      artifact: normalizeArtifact(params.artifact),
      memory_id: params.memoryId ?? null,
    },
    eventId: harnessId,
  });

  return harnessId;
}

/** Query harness events with optional filters. */
export function queryHarnessLog(
  db: DatabaseSync,
  params: { sessionId?: string; agentId?: string; workspacePath?: string; artifact?: string | null; eventType?: HarnessEventType; limit?: number },
): HarnessLogRow[] {
  const conditions: string[] = ["event_type LIKE 'harness.%'"];
  const bindings: (string | number)[] = [];

  if (params.sessionId !== undefined) {
    conditions.push('session_id = ?');
    bindings.push(params.sessionId);
  }
  if (params.agentId !== undefined) {
    conditions.push("json_extract(actor_json, '$.id') = ?");
    bindings.push(params.agentId);
  }
  if (params.workspacePath !== undefined) {
    conditions.push('workspace_path = ?');
    bindings.push(params.workspacePath);
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact !== null) {
    conditions.push("json_extract(payload_json, '$.artifact') = ?");
    bindings.push(artifact);
  }
  if (params.eventType !== undefined) {
    conditions.push('event_type = ?');
    bindings.push(`harness.${params.eventType}`);
  }

  const sql = `SELECT aggregate_id AS harness_id, session_id,
      json_extract(actor_json, '$.id') AS agent_id, workspace_path,
      json_extract(payload_json, '$.artifact') AS artifact,
      substr(event_type, 9) AS event_type,
      CASE WHEN json_type(payload_json, '$.data') = 'null' THEN NULL
        ELSE json_extract(payload_json, '$.data') END AS payload_json,
      json_extract(payload_json, '$.memory_id') AS memory_id,
      correlation_id AS run_id, created_at
    FROM event_outbox WHERE ${conditions.join(' AND ')}
    ORDER BY sequence DESC LIMIT ?`;
  const rows = db.prepare(sql).all(...bindings, boundedLimit(params.limit)) as unknown as HarnessLogRow[];
  return rows.map((row) => ({
    ...row,
    payload_json: row.payload_json === null
      ? null
      : typeof row.payload_json === 'string' ? row.payload_json : JSON.stringify(row.payload_json),
  }));
}
