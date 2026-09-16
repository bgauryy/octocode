import type { DatabaseSync } from './sqlite.js';
import { signalExpiresAt } from './message-lifecycle.js';

export interface MigrationEvent {
  event_id: string; workspace_path: string; event_type: string;
  aggregate_kind: string | null; aggregate_id: string | null; aggregate_revision: string | null;
  actor_json: string; provenance_json: string; payload_json: string;
  session_id: string | null; correlation_id: string | null; created_at: string; expires_at: null;
  schema_version: 1; retention_class: 'delivery' | 'operational';
}

function hasTable(db: DatabaseSync, table: string): boolean {
  return Boolean(db.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?").get(table));
}
function required(row: Record<string, unknown>, field: string): string {
  const value = row[field];
  if (typeof value !== 'string' || !value) throw new Error(`unsupported source row: handoffs.${field} is required`);
  return value;
}

export function legacyHandoffEvents(source: DatabaseSync): MigrationEvent[] {
  if (!hasTable(source, 'handoffs')) return [];
  const rows = source.prepare('SELECT * FROM handoffs ORDER BY created_at, handoff_id').all() as Array<Record<string, unknown>>;
  return rows.map(row => {
    const id = required(row, 'handoff_id');
    const agentId = required(row, 'agent_id');
    const summary = required(row, 'summary');
    let files: unknown;
    try { files = JSON.parse(required(row, 'files_json')); }
    catch { throw new Error(`unsupported source row: handoffs.files_json is invalid JSON for ${id}`); }
    if (!Array.isArray(files) || files.some(file => typeof file !== 'string')) {
      throw new Error(`unsupported source row: handoffs.files_json is invalid for ${id}`);
    }
    return {
      event_id: `legacy.handoff:${id}`, workspace_path: required(row, 'workspace_path'),
      event_type: 'peer.message', aggregate_kind: 'message', aggregate_id: id, aggregate_revision: null,
      actor_json: JSON.stringify({ kind: 'agent', id: agentId }),
      provenance_json: JSON.stringify({ source: 'peer', trust: 'attributed-data' }),
      payload_json: JSON.stringify({ messageId: id, fromAgentId: agentId, toAgentId: null,
        signalKind: 'handoff', topic: summary, text: summary, files }),
      session_id: null, correlation_id: null, created_at: required(row, 'created_at'), expires_at: null,
      schema_version: 1 as const, retention_class: 'delivery' as const,
    };
  });
}

export function copyLegacyHandoffSignals(source: DatabaseSync, destination: DatabaseSync): void {
  if (!hasTable(source, 'handoffs')) return;
  const rows = source.prepare('SELECT * FROM handoffs ORDER BY created_at, handoff_id').all() as Array<Record<string, unknown>>;
  const insert = destination.prepare(`INSERT INTO signals(signal_id,workspace_path,from_agent,to_agent,kind,subject,
    body,files_json,refs_json,thread_id,reply_to,importance,status,resolved_at,created_at,expires_at)
    VALUES (?,?,?,NULL,'handoff',?,NULL,?,'[]',?,NULL,5,?,?,?,?)`);
  for (const row of rows) {
    const id = required(row, 'handoff_id');
    const clearedAt = typeof row.cleared_at === 'string' ? row.cleared_at : null;
    const createdAt = required(row, 'created_at');
    insert.run(id, required(row, 'workspace_path'), required(row, 'agent_id'), required(row, 'summary'),
      required(row, 'files_json'), id, clearedAt ? 'resolved' : 'open', clearedAt, createdAt,
      signalExpiresAt('handoff', createdAt));
  }
}
