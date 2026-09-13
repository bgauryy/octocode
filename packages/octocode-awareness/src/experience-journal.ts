import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import type { DatabaseSync } from 'node:sqlite';
import { appendDomainEvent, type OutboxRow } from './event-outbox.js';
import type { ExperienceBinding, ExperienceEvent } from './experience-contract.js';

export const experienceHash = (value: unknown): string => createHash('sha256').update(JSON.stringify(value)).digest('hex');
export const experienceEventId = (workspace: string, trace: string, event: string): string =>
  `experience_${experienceHash([workspace, trace, event])}`;
export function experienceTransaction<T>(db: DatabaseSync, task: () => T): T {
  db.exec('BEGIN IMMEDIATE');
  try { const result = task(); db.exec('COMMIT'); return result; }
  catch (error) { db.exec('ROLLBACK'); throw error; }
}
export function traceRows(db: DatabaseSync, workspace: string, trace: string, snapshot = Number.MAX_SAFE_INTEGER): OutboxRow[] {
  const rows = db.prepare(`SELECT * FROM event_outbox WHERE workspace_path = ? AND aggregate_kind = 'experience'
    AND aggregate_id = ? AND event_type IN ('experience.record', 'experience.sealed', 'experience.archive')
    AND sequence <= ? ORDER BY sequence LIMIT 131`).all(workspace, trace, snapshot) as unknown as OutboxRow[];
  if (rows.length > 130) throw Object.assign(new Error('EXPERIENCE_JOURNAL_LIMIT: trace exceeds the 130-row journal bound'), {
    code: 'EXPERIENCE_JOURNAL_LIMIT', terminal_limit: { kind: 'trace-journal', maximum: 130 },
  });
  return rows;
}
export function eventFromRow(row: OutboxRow): ExperienceEvent {
  return { ...JSON.parse(row.payload_json) as ExperienceEvent, sequence: Number(row.sequence),
    actor_id: (JSON.parse(row.actor_json) as { id: string }).id,
    ...(row.session_id ? { session_id: row.session_id } : {}), created_at: row.created_at };
}
export function traceEvents(rows: OutboxRow[]): ExperienceEvent[] {
  return rows.filter(row => row.event_type === 'experience.record').map(eventFromRow);
}
export function requireTrace(rows: OutboxRow[]): void {
  if (!rows.some(row => row.event_type === 'experience.record')) throw new Error('EXPERIENCE_NOT_FOUND: trace does not exist in this workspace');
}
export function appendExperience(db: DatabaseSync, binding: ExperienceBinding, trace: string,
  key: string, type: string, payload: unknown): OutboxRow {
  const eventId = experienceEventId(binding.workspace, trace, key);
  const existing = db.prepare('SELECT * FROM event_outbox WHERE event_id = ?').get(eventId) as unknown as OutboxRow | undefined;
  if (existing) {
    if (existing.event_type !== type || !isDeepStrictEqual(JSON.parse(existing.payload_json), payload)
      || JSON.parse(existing.actor_json).id !== binding.actorId || existing.session_id !== (binding.sessionId ?? null)) {
      throw new Error('EXPERIENCE_ID_CONFLICT: event ID conflicts with immutable content or provenance');
    }
    return existing;
  }
  appendDomainEvent(db, { workspace: binding.workspace, actorId: binding.actorId, sessionId: binding.sessionId,
    aggregateKind: 'experience', aggregateId: trace, eventType: type, retentionClass: 'audit',
    createdAt: new Date().toISOString(), eventId, payload });
  return db.prepare('SELECT * FROM event_outbox WHERE event_id = ?').get(eventId) as unknown as OutboxRow;
}
interface Cursor { after: number; snapshot: number; scope: string }
export function experienceCursor(raw: string | undefined, scope: unknown, snapshot: number): Cursor {
  if (!raw) return { after: 0, snapshot, scope: experienceHash(scope) };
  let cursor: Cursor;
  try { cursor = JSON.parse(Buffer.from(raw, 'base64url').toString('utf8')) as Cursor; }
  catch { throw new Error('EXPERIENCE_CURSOR_INVALID: malformed cursor'); }
  if (!cursor || typeof cursor !== 'object' || cursor.scope !== experienceHash(scope)
    || !Number.isSafeInteger(cursor.after) || cursor.after < 0 || cursor.after > cursor.snapshot
    || !Number.isSafeInteger(cursor.snapshot) || cursor.snapshot < 0 || cursor.snapshot > snapshot) {
    throw new Error('EXPERIENCE_CURSOR_INVALID: cursor does not match this query or workspace');
  }
  return cursor;
}
export const encodeExperienceCursor = (cursor: Cursor): string => Buffer.from(JSON.stringify(cursor)).toString('base64url');
