import type { DatabaseSync } from '../sqlite.js';
import type { LiteMessage } from '../entities.js';
import { type CanonicalMessageRow, messageFromCanonicalSignalRow } from './coordination-shared.js';
import { decodeSignalCursor, encodeSignalCursor } from '../signal-pagination.js';
import { repositoryWorkspacePaths } from '../git.js';

export interface MessageListParams {
  agentId?: string | null;
  includeRead?: boolean;
  topic?: string | null;
  limit?: number;
  cursor?: string;
}

export interface MessagePage {
  messages: LiteMessage[];
  partial: boolean;
  partialReasons: Array<'limit'>;
  next?: { list: { method: 'listMessagesPage'; params: MessageListParams } };
}

function messageFilter(workspace: string, params: MessageListParams) {
  const agentId = params.agentId?.trim();
  const topic = params.topic?.trim();
  const clauses: string[] = ['s.workspace_path IN (SELECT value FROM json_each(?))'];
  const values: string[] = [JSON.stringify(repositoryWorkspacePaths(workspace))];
  if (agentId) {
    clauses.push('s.from_agent != ?');
    values.push(agentId);
    clauses.push('(s.to_agent IS NULL OR s.to_agent = ?)');
    values.push(agentId);
    if (!params.includeRead) {
      clauses.push('NOT EXISTS (SELECT 1 FROM signal_reads r WHERE r.signal_id = s.signal_id AND r.agent_id = ?)');
      values.push(agentId);
    }
  }
  if (topic) {
    clauses.push('s.subject = ?');
    values.push(topic);
  }
  return { agentId, clauses, values };
}

/** Exact count uses the same recipient, topic, and read filters as the inbox. */
export function countInboxMessages(db: DatabaseSync, workspace: string, params: Omit<MessageListParams, 'cursor' | 'limit'> = {}): number {
  const { clauses, values } = messageFilter(workspace, params);
  return (db.prepare(`SELECT COUNT(*) AS count FROM signals s WHERE ${clauses.join(' AND ')}`)
  .get(...values) as { count: number }).count;
}

export function listInboxMessagesPage(db: DatabaseSync, workspace: string, params: MessageListParams = {}): MessagePage {
  const limit = Math.min(100, Math.max(1, Math.floor(Number.isFinite(params.limit) ? params.limit! : 20)));
  const { agentId, clauses, values } = messageFilter(workspace, params);
  const cursor = decodeSignalCursor(params.cursor);
  if (cursor) {
    clauses.push('(s.created_at < ? OR (s.created_at = ? AND s.signal_id < ?))');
    values.push(cursor.createdAt, cursor.createdAt, cursor.signalId);
  }
  const readAt = agentId
    ? '(SELECT r.read_at FROM signal_reads r WHERE r.signal_id = s.signal_id AND r.agent_id = ?) AS read_at'
    : 'NULL AS read_at';
  const rows = db.prepare(`SELECT s.*, ${readAt} FROM signals s WHERE ${clauses.join(' AND ')} ORDER BY s.created_at DESC, s.signal_id DESC LIMIT ?`)
    .all(...(agentId ? [agentId, ...values] : values), limit + 1) as unknown as CanonicalMessageRow[];
  const messages = rows.slice(0, limit).map(messageFromCanonicalSignalRow);
  const partial = rows.length > limit;
  const last = rows[Math.min(rows.length, limit) - 1];
  return {
    messages, partial, partialReasons: partial ? ['limit'] : [],
    ...(partial && last ? { next: { list: { method: 'listMessagesPage' as const,
      params: { ...params, limit, cursor: encodeSignalCursor(last.created_at, last.signal_id) },
    } } } : {}),
  };
}
