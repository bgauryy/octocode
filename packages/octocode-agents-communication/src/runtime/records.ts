/** One scoped, indexed event envelope; operational transitions own their writes. */
import * as catalog from './catalog.js';
import { query, readTransaction, rowInteger, rowOptionalText, rowText, transaction, type Database, type Row, type SqlValue } from './database.js';
import type { FetchInput, RecordInput } from './catalog-types.generated.js';
import type { CommandResults, RecordData, RecordEnvelope, RecordType } from './catalog-types.generated.js';
import type { Session } from './rows.js';
import { ValidationError } from './errors.js';
import { WHITESPACE_CLASS } from './text.js';
import { now } from './timing.js';
import { jsonEqual, parseObject } from './validation.js';
import { compact } from './json-text.js';

/** A search word or a quoted phrase; words end at the catalog's whitespace. */
const WORDS = new RegExp(`"([^"]+)"|([^${WHITESPACE_CLASS}]+)`, 'g');

/** What the record operations need from the store that owns the connection and the session rules. */
export interface RecordsHost {
  readonly db: Database;
  readonly workspace: string;
  /** The caller's session row; fails for an unknown one, and for an expired one unless `active` is false. */
  known(session: string, active?: boolean): Session;
  peer(id: string): Session;
  resolvePeer(value: string): string;
  sharedWorkspaces(): string[];
}

/** Appends a record for `session`, taking path and default branch from its session row; returns the record ID. */
export function append<T extends RecordType>(db: Database, session: string, type: T, data: RecordData[T], timestamp: number, key: string | null = null, entity: string | null = null): number {
  return db.run('INSERT INTO records(path,"from","to",type,timestamp,branch,data,key,entityId) SELECT workspace,id,NULL,?,?,branch,?,?,? FROM sessions WHERE id=?', [type, timestamp, compact(data), key, entity, session]).lastInsertRowid;
}

export function envelope(row: Row): RecordEnvelope {
  // recordId is a history cursor; naming it apart from message IDs keeps it out of complete.
  const branch = rowOptionalText(row, 'branch');
  return {
    recordId: rowInteger(row, 'id'), path: rowText(row, 'path'), from: rowText(row, 'from'), type: rowText(row, 'type'), timestamp: rowInteger(row, 'timestamp'),
    to: rowOptionalText(row, 'to') ?? null, ...(branch === undefined ? {} : { branch }), data: parseObject(rowText(row, 'data'), 'record data'),
  };
}

const PAYLOAD = "CASE WHEN r.type='message' THEN json_set(r.data,'$.kind',m.kind,'$.body',m.body,'$.expiresAt',m.expiresAt,'$.ttlMs',m.ttlMs) ELSE r.data END";
export const RECORD_SELECT = 'SELECT r.id,r.path,r."from",r."to",r.type,r.timestamp,r.branch,' + PAYLOAD + " AS data FROM records r LEFT JOIN messages m ON r.type='message' AND m.id=CAST(r.entityId AS INTEGER)";
// Mail history is visible only to its author and the recipients fixed at send time.
// Custom targeted records use the same author/recipient rule; NULL means workspace-shared.
const VISIBILITY = `((r.type='message' OR r.type LIKE 'delivery.%' OR r.type LIKE 'dispatch.%')
 AND EXISTS(SELECT 1 FROM messages v WHERE v.id=CAST(r.entityId AS INTEGER)
 AND (v.sender=? OR EXISTS(SELECT 1 FROM deliveries d WHERE d.message=v.id AND d.recipient=?)))
 OR (r.type<>'message' AND r.type NOT LIKE 'delivery.%' AND r.type NOT LIKE 'dispatch.%'
 AND (r."to" IS NULL OR r."to"='*' OR r."from"=? OR r."to"=?)))`;

const DOTTED_FIELD = /^[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*$/;

export function fetch(host: RecordsHost, session: string, value: FetchInput): CommandResults['fetch'] {
  catalog.command('fetch', value);
  return readTransaction(host.db, () => {
    host.known(session, false);
    const shared = compact(host.sharedWorkspaces());
    const after: number = value.after ?? 0;
    const through: number = value.through ?? host.db.integer('SELECT coalesce(max(id),0) FROM records WHERE path IN (SELECT value FROM json_each(?))', [shared]);
    if (through < after) throw new ValidationError('through must be at least after');
    const conditions = ['r.path IN (SELECT value FROM json_each(?))', 'r.id>?', 'r.id<=?', VISIBILITY];
    const args: SqlValue[] = [shared, after, through, session, session, session, session];
    for (const [field, column] of [['recordId', 'id'], ['type', 'type'], ['from', 'from'], ['to', 'to'], ['branch', 'branch']] as const) {
      const expected = value[field];
      if (expected !== undefined) {
        conditions.push(`r."${column}"=?`);
        args.push(expected);
      }
    }
    if ('types' in value) {
      conditions.push('r.type IN (SELECT value FROM json_each(?))');
      args.push(compact(value.types));
    }
    for (const [field, operator] of [['since', '>='], ['until', '<=']] as const) {
      const bound = value[field];
      if (bound !== undefined) {
        conditions.push('r.timestamp' + operator + '?');
        args.push(bound);
      }
    }
    if (value.incoming) {
      conditions.push("r.type='message' AND r.entityId IN (SELECT CAST(d.message AS TEXT) FROM deliveries d JOIN messages i ON i.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND i.expiresAt>?)");
      args.push(session, now());
    }
    if (value.current) {
      conditions.push("r.type GLOB 'dispatch.*' AND EXISTS(SELECT 1 FROM dispatches x WHERE CAST(x.message AS TEXT)=r.entityId AND x.recipient=r.\"from\" AND x.state=substr(r.type,10) AND x.token=json_extract(r.data,'$.token'))");
      conditions.push("r.id=(SELECT max(latest.id) FROM records latest WHERE latest.path=r.path AND latest.\"from\"=r.\"from\" AND latest.entityId=r.entityId AND latest.type GLOB 'dispatch.*')");
    }
    for (const [field, expected] of Object.entries(value.where ?? {})) {
      if (!DOTTED_FIELD.test(field)) throw new ValidationError('where keys must be dotted data field names');
      // Known operational IDs use the reference index before JSON filtering.
      const stream = value.type ?? '';
      if (Number.isInteger(expected) && ((field === 'messageId' && (stream === 'message' || stream.startsWith('delivery.') || stream.startsWith('dispatch.'))) || (field === 'leaseId' && stream.startsWith('lease.')))) {
        conditions.push('r.entityId=?');
        args.push(String(expected));
      }
      // Type checks distinguish JSON false from 0, and explicit null from a missing field.
      const kind = expected === null ? 'null' : expected === true ? 'true' : expected === false ? 'false' : typeof expected === 'string' ? 'text' : null;
      conditions.push(`json_extract(${PAYLOAD},?) IS ?`);
      args.push('$.' + field, expected ?? null);
      conditions.push(`json_type(${PAYLOAD},?) ` + (kind ? '=?' : "IN ('integer','real')"));
      args.push('$.' + field);
      if (kind) args.push(kind);
    }
    if ('search' in value) {
      const words = [...String(value.search).matchAll(WORDS)];
      if (!words.length) throw new ValidationError('search requires words or phrases');
      conditions.push('r.id IN (SELECT rowid FROM records_search WHERE records_search MATCH ?)');
      args.push(words.map(([, phrase, word]) => '"' + (phrase ?? word)!.replaceAll('"', '""') + '"').join(' AND '));
    }
    const limit: number = value.limit ?? 100;
    const rows = query(host.db, RECORD_SELECT + ' WHERE ' + conditions.join(' AND ') + ' ORDER BY r.id LIMIT ?', [...args, limit + 1]);
    const result = { ...catalog.page(rows.map(envelope), false, 'fetch', { ...value, through }), through };
    // page also has a byte budget; honor the caller's smaller item budget.
    if (result.items.length > limit) {
      result.items = result.items.slice(0, limit);
      result.next = { command: 'fetch', input: { ...value, through, after: result.items.at(-1)!.recordId } };
    }
    return result;
  });
}

export function record(host: RecordsHost, session: string, value: RecordInput): CommandResults['record'] {
  catalog.command('record', value);
  const data = compact(value.data);
  if (Buffer.byteLength(data) > 16384) throw new ValidationError('Record data exceeds 16384 UTF-8 bytes');
  return transaction(host.db, () => {
    const identity = host.known(session);
    let input = value;
    if (input.to !== undefined && input.to !== '*') {
      const to = host.resolvePeer(input.to);
      input = { ...input, to };
      host.peer(to);
    }
    // An explicit null branch suppresses the session default.
    const branch: string | null = input.branch !== undefined ? input.branch : (identity.branch ?? null);
    const existing = query(host.db, 'SELECT * FROM records WHERE "from"=? AND type=? AND key=?', [session, input.type, input.key ?? null]);
    if (existing.length) {
      const row = existing[0]!;
      if (!jsonEqual(JSON.parse(rowText(row, 'data')), input.data) || (rowOptionalText(row, 'to') ?? null) !== (input.to ?? null) || (rowOptionalText(row, 'branch') ?? null) !== branch)
        throw new ValidationError('Record key reused with different data, routing or branch');
      return envelope(row);
    }
    const { lastInsertRowid } = host.db.run('INSERT INTO records(path,"from","to",type,timestamp,branch,data,key) VALUES(?,?,?,?,?,?,?,?)', [host.workspace, session, input.to ?? null, input.type, now(), branch, data, input.key ?? null]);
    return envelope(query(host.db, 'SELECT * FROM records WHERE id=?', [lastInsertRowid])[0]!);
  });
}

/** The record envelopes of the given messages (`{id}` items), in the same order. */
export function messageEnvelopes(host: RecordsHost, items: readonly { id: number }[]): RecordEnvelope[] {
  if (!items.length) return [];
  const rows = query(host.db, RECORD_SELECT + " WHERE r.path IN (SELECT value FROM json_each(?)) AND r.type='message' AND r.entityId IN (SELECT CAST(value AS TEXT) FROM json_each(?))", [compact(host.sharedWorkspaces()), compact(items.map(item => item.id))]);
  const byMessage = new Map(rows.map(envelope).map(row => [row.data.messageId, row]));
  return items.map(item => {
    const row = byMessage.get(item.id);
    if (!row) throw new ValidationError('Message envelope is missing: ' + item.id);
    return row;
  });
}
