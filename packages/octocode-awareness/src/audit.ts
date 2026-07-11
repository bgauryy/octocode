/**
 * audit.ts — edit_log and harness_log operations.
 * Records file edits and harness lifecycle events into the SQLite store.
 */

import type { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { createHash } from 'node:crypto';
import { normalizeArtifact, utcNow } from './helpers.js';
import {
  EDIT_LOG_INSERT,
  HARNESS_LOG_INSERT,
} from './sql/audit.js';
import type {
  InsertEditLogParams,
  EditLogRow,
  QueryEditLogParams,
  InsertHarnessLogParams,
  HarnessLogRow,
  HarnessEventType,
} from './types.js';

// ─── sha256 helper ────────────────────────────────────────────────────────────

/** Hash a string with sha256, returns hex (for content_hash). */
export function sha256Hex(content: string): string {
  return createHash('sha256').update(content).digest('hex');
}

// ─── edit_log ─────────────────────────────────────────────────────────────────

/** Record a single file edit. Returns an object with the generated editId. */
export function insertEditLog(db: DatabaseSync, params: InsertEditLogParams): { editId: string } {
  const editId = 'edit_' + randomUUID();
  const now = utcNow();

  db.prepare(EDIT_LOG_INSERT).run(
    editId,
    params.sessionId ?? null,
    params.runId ?? null,
    params.agentId,
    params.filePath,
    params.operation,
    params.oldFilePath ?? null,
    params.linesAdded ?? null,
    params.linesRemoved ?? null,
    params.contentHash ?? null,
    params.workspacePath ?? null,
    normalizeArtifact(params.artifact),
    now,
  );

  return { editId };
}

/** Query the edit log with optional filters. */
export function queryEditLog(db: DatabaseSync, params: QueryEditLogParams): EditLogRow[] {
  const conditions: string[] = [];
  const bindings: (string | number)[] = [];

  if (params.sessionId !== undefined) {
    conditions.push('session_id = ?');
    bindings.push(params.sessionId);
  }
  if (params.runId !== undefined) {
    conditions.push('run_id = ?');
    bindings.push(params.runId);
  }
  if (params.agentId !== undefined) {
    conditions.push('agent_id = ?');
    bindings.push(params.agentId);
  }
  if (params.filePath !== undefined) {
    conditions.push('file_path = ?');
    bindings.push(params.filePath);
  }
  if (params.workspacePath !== undefined) {
    conditions.push('workspace_path = ?');
    bindings.push(params.workspacePath);
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact !== null) {
    conditions.push('artifact = ?');
    bindings.push(artifact);
  }
  if (params.operation !== undefined) {
    conditions.push('operation = ?');
    bindings.push(params.operation);
  }
  if (params.since !== undefined) {
    conditions.push('created_at >= ?');
    bindings.push(params.since);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';
  const limit = params.limit !== undefined ? `LIMIT ${params.limit}` : '';

  const sql = `SELECT * FROM edit_log ${where} ORDER BY created_at DESC ${limit}`.trim();
  return db.prepare(sql).all(...bindings) as unknown as EditLogRow[];
}

// ─── harness_log ──────────────────────────────────────────────────────────────

/** Record a harness lifecycle event. Returns the harness_id. */
export function insertHarnessLog(db: DatabaseSync, params: InsertHarnessLogParams): string {
  const harnessId = 'harness_' + randomUUID();
  const now = utcNow();
  const payloadJson = params.payload !== undefined ? JSON.stringify(params.payload) : null;

  db.prepare(HARNESS_LOG_INSERT).run(
    harnessId,
    params.sessionId ?? null,
    params.agentId,
    params.workspacePath ?? null,
    normalizeArtifact(params.artifact),
    params.eventType,
    payloadJson,
    params.memoryId ?? null,
    params.runId ?? null,
    now,
  );

  return harnessId;
}

/** Query harness events with optional filters. */
export function queryHarnessLog(
  db: DatabaseSync,
  params: { sessionId?: string; agentId?: string; workspacePath?: string; artifact?: string | null; eventType?: HarnessEventType; limit?: number },
): HarnessLogRow[] {
  const conditions: string[] = [];
  const bindings: (string | number)[] = [];

  if (params.sessionId !== undefined) {
    conditions.push('session_id = ?');
    bindings.push(params.sessionId);
  }
  if (params.agentId !== undefined) {
    conditions.push('agent_id = ?');
    bindings.push(params.agentId);
  }
  if (params.workspacePath !== undefined) {
    conditions.push('workspace_path = ?');
    bindings.push(params.workspacePath);
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact !== null) {
    conditions.push('artifact = ?');
    bindings.push(artifact);
  }
  if (params.eventType !== undefined) {
    conditions.push('event_type = ?');
    bindings.push(params.eventType);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';
  const limit = params.limit !== undefined ? `LIMIT ${params.limit}` : '';

  const sql = `SELECT * FROM harness_log ${where} ORDER BY created_at DESC ${limit}`.trim();
  return db.prepare(sql).all(...bindings) as unknown as HarnessLogRow[];
}
