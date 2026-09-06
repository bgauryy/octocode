import { isDeepStrictEqual } from 'node:util';
import type { DatabaseSync } from 'node:sqlite';
import { parseAgentEventEnvelopeV1, type AgentEventEnvelopeV1 } from './continuity-contracts.js';

/**
 * Append one validated continuity event inside the caller's durable write
 * boundary. The caller owns its transaction so a domain row and its delivery
 * event commit together; nested callers do not begin a second transaction.
 */
export function insertOutboxEvent(db: DatabaseSync, input: AgentEventEnvelopeV1): number {
  const event = parseAgentEventEnvelopeV1(input);
  const result = db.prepare(`INSERT INTO event_outbox(
    event_id, workspace_path, event_type, aggregate_kind, aggregate_id, aggregate_revision,
    actor_json, provenance_json, payload_json, session_id, correlation_id, created_at, expires_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  ON CONFLICT(event_id) DO NOTHING`).run(
    event.eventId,
    event.workspace,
    event.type,
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
