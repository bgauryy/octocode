/** Read-only workspace operator views; no synthetic agent identity. */
import { query, type Row, type SqlValue } from './database.js';
import type { RecordEnvelope } from './catalog-types.generated.js';
import { ValidationError } from './errors.js';
import { SELECT as RECORD_SELECT, envelope } from './records.js';
import type { StoreHost } from './store-types.js';
import { now } from './timing.js';
import { isObject, isUuid, jsonEqual, type Json } from './validation.js';
import { compact, parseJsonBytes } from './json-text.js';

const ENTITIES = ['session', 'message', 'lease', 'records', 'delivery', 'dispatch', 'document', 'attachment', 'subscriptions'];
const PAGE_ROWS = 50;
const VIEW_PAGE_BYTES = 128 * 1024;
const FILTER_BYTES = 256;

/** `[sql, the column an agent filter matches, whether rows are keyed by (message, recipient)]`. */
const QUERIES: Record<string, readonly [string, string, boolean]> = {
  session: ['SELECT s.*,s.expiresAt>?2 AS active FROM sessions s WHERE s.workspace=?1', 'id', false],
  message: ['SELECT m.*,m.expiresAt-m.ttlMs AS createdAt,s.name AS senderName,s.vendor AS senderVendor,t.name AS targetName,t.vendor AS targetVendor,(SELECT count(*) FROM deliveries d WHERE d.message=m.id AND d.acknowledgedAt IS NULL) AS pending,(SELECT count(*) FROM deliveries d WHERE d.message=m.id) AS recipients FROM messages m JOIN sessions s ON s.id=m.sender LEFT JOIN sessions t ON t.id=m.target WHERE s.workspace=?1 AND ?2>0', 'sender', false],
  lease: ['SELECT l.id,l.workspace,l.path,l.kind,l.owner,s.name AS ownerName,l.reasoning,l.acquiredAt,l.refreshedAt,l.expiresAt FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=?1 AND s.workspace=?1 AND l.expiresAt>?2 AND s.expiresAt>?2', 'owner', false],
  records: [RECORD_SELECT + ' WHERE r.path=?1 AND ?2>0', '[from]', false],
  delivery: ["SELECT CAST(d.message AS TEXT)||':'||d.recipient AS id,d.*,s.name AS recipientName FROM deliveries d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND ?2>0", 'recipient', true],
  dispatch: ["SELECT CAST(d.message AS TEXT)||':'||d.recipient AS id,d.message,d.recipient,s.name AS recipientName,d.transport,d.state,d.attemptedAt,d.submittedAt,d.error FROM dispatches d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND ?2>0", 'recipient', true],
  document: ['SELECT d.id,d.name,a.[from] AS session,s.name AS agentName,a.timestamp AS at,a.data FROM documents d JOIN records a ON a.id=d.id JOIN sessions s ON s.id=a.[from] WHERE d.workspace=?1 AND s.workspace=?1 AND ?2>0', 'session', false],
  attachment: ['SELECT a.session AS id,a.session,a.transport,a.updatedAt,s.name AS agentName FROM attachments a JOIN sessions s ON s.id=a.session WHERE s.workspace=?1 AND ?2>0', 'session', false],
  subscriptions: ["SELECT s.id,s.id AS session,s.name AS agentName,coalesce((SELECT json_group_array(topic) FROM subscriptions WHERE session=s.id),'[]') AS topics FROM sessions s WHERE s.workspace=?1 AND ?2>0", 'session', false],
};

/** One row of an operator view: its columns, with JSON columns parsed. Record rows are full envelopes. */
export type ViewItem = Record<string, Json | undefined> | RecordEnvelope;
/** The message filters of a view request, by name. */
export type ViewFilters = Record<string, string>;

export interface ViewSummary {
  workspace: string;
  database: string;
  at: number;
  counts: Row | undefined;
  entities: readonly string[];
}

export interface ViewPage {
  items: ViewItem[];
  next: string | null;
  at: number;
}

export function summary(store: StoreHost): ViewSummary {
  store.checkDatabase();
  const at = now();
  // One statement gives a consistent read snapshot without acquiring a writer lock.
  const counts = query(store.db, `SELECT
      (SELECT count(*) FROM sessions WHERE workspace=?1 AND expiresAt>?2) AS activeAgents,
      (SELECT count(*) FROM messages m JOIN sessions s ON s.id=m.sender WHERE s.workspace=?1) AS messages,
      (SELECT count(*) FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=?1 AND s.workspace=?1 AND l.expiresAt>?2 AND s.expiresAt>?2) AS activeLocks,
      (SELECT count(*) FROM deliveries d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND d.acknowledgedAt IS NULL) AS pending,
      (SELECT count(*) FROM dispatches d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND d.state='uncertain') AS uncertain`, [store.workspace, at]);
  return { workspace: store.workspace, database: store.database, at, counts: counts[0], entities: ENTITIES };
}

/** A cursor is base64url, and also accepts the standard alphabet's `+` and `/`. */
const BASE64URL = /^[A-Za-z0-9_+/-]*$/;

function decodeCursor(after: string): unknown {
  if (!BASE64URL.test(after) || after.length % 4 === 1) throw new ValidationError('Invalid cursor');
  return parseJsonBytes(Buffer.from(after, 'base64url'));
}

export function page(store: StoreHost, entity: string, after: string | null = null, agent: string | null = null, filters: ViewFilters = {}): ViewPage {
  store.checkDatabase();
  const definition = QUERIES[entity];
  if (!Object.hasOwn(QUERIES, entity) || !definition) throw new ValidationError('Unknown view entity');
  const [sql, agentField, pair] = definition;
  const at = now();
  const args: SqlValue[] = [store.workspace, at];
  const conditions = ['1=1'];
  if (agent !== null) {
    if (!isUuid(agent)) throw new ValidationError('badly formed hexadecimal UUID string');
    args.push(agent);
    const n = args.length;
    conditions.push(entity === 'message' ? `(sender=?${n} OR id IN (SELECT message FROM deliveries WHERE recipient=?${n}))` : `${agentField}=?${n}`);
  }
  if (!isObject(filters)) throw new ValidationError('Invalid message filters');
  for (const [key, value] of Object.entries(filters)) {
    if (entity !== 'message') throw new ValidationError('Message filters require the Messages view');
    if (typeof value !== 'string') throw new ValidationError('Invalid filter');
    if (!value || Buffer.byteLength(value) > FILTER_BYTES) throw new ValidationError('Filter must contain 1–256 bytes');
    if (key === 'q') {
      args.push(value);
      const n = args.length;
      conditions.push(`(instr(lower(body),lower(?${n}))>0 OR instr(lower(senderName),lower(?${n}))>0 OR instr(lower(targetName),lower(?${n}))>0 OR instr(lower(conversationId),lower(?${n}))>0 OR CAST(id AS TEXT)=?${n})`);
    } else if (key === 'conversation') {
      args.push(value);
      conditions.push(`conversationId=?${args.length}`);
    } else if (key === 'status') {
      if (value !== 'pending' && value !== 'handled') throw new ValidationError('Unknown message status');
      conditions.push(value === 'pending' ? 'pending>0' : 'pending=0 AND recipients>0');
    } else throw new ValidationError('Unknown message filter');
  }
  if (after !== null) {
    const cursor = decodeCursor(after);
    if (!isObject(cursor) || cursor.entity !== entity || (cursor.agent ?? null) !== agent || !jsonEqual(cursor.filters, filters)) throw new ValidationError('Cursor belongs to another view; refresh the first page');
    if (pair) {
      const { message, recipient } = cursor;
      if (typeof message !== 'number' || !Number.isInteger(message) || typeof recipient !== 'string') throw new ValidationError('Invalid cursor');
      const n = args.length + 1;
      conditions.push(`(message<?${n} OR (message=?${n} AND recipient<?${n + 1}))`);
      args.push(message, recipient);
    } else {
      const identity = cursor.id;
      if (!(typeof identity === 'number' && Number.isInteger(identity)) && typeof identity !== 'string') throw new ValidationError('Invalid cursor');
      args.push(identity);
      conditions.push(`id<?${args.length}`);
    }
  }
  const order = pair ? 'message DESC,recipient DESC' : 'id DESC';
  const found = query(store.db, `SELECT * FROM (${sql}) WHERE ${conditions.join(' AND ')} ORDER BY ${order} LIMIT ${PAGE_ROWS + 1}`, args);
  let size = 0;
  let count = 0;
  for (const row of found.slice(0, PAGE_ROWS)) {
    const length = Buffer.byteLength(compact(row));
    if (size && size + length > VIEW_PAGE_BYTES) break;
    size += length;
    count += 1;
  }
  let next: string | null = null;
  if (found.length > count && count) {
    const row = found[count - 1]!;
    const cursor = { entity, agent, filters, id: row.id ?? null, message: row.message ?? null, recipient: row.recipient ?? null };
    next = Buffer.from(compact(cursor)).toString('base64url');
  }
  const items = found.slice(0, count).map((row): ViewItem => {
    if (entity === 'records') return envelope(row);
    const item: Record<string, Json | undefined> = { ...row };
    for (const field of ['data', 'topics']) {
      const column = row[field];
      if (typeof column === 'string') item[field] = JSON.parse(column);
    }
    return item;
  });
  return { items, next, at };
}
