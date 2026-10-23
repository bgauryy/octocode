/**
 * Message protocols: each send_message kind is a field named after a known protocol.
 *
 * The catalog owns every protocol's slots (send_message and complete input schemas, titles and slot order included). This
 * module owns what a schema cannot state: the cross-references to messages, leases, documents and peers, and the delivery
 * policy of each kind. It renders the stored body from the slots, so every host receives the same text.
 *
 * The kind taxonomy and each kind's delivery policy (request or FYI, wakes or passive) come from the catalog's `messageKinds`
 * markers; nothing here names a kind except `answer`, the reply of `complete`, which is an FYI that does not wake.
 */
import * as catalog from './catalog.js';
import { execute, query, rowInteger, rowOptionalText, rowText } from './database.js';
import type { CommandResults, CompleteInput, SendMessageInput } from './catalog-types.generated.js';
import { leaseKind } from './rows.js';
import type { Json, JsonObject } from './validation.js';
import { ValidationError } from './errors.js';
import { documentRecord } from './documents.js';
import { relativePath, type StoreHost } from './store-types.js';
import { codePointLength, isBlank, truncateCodePoints, truncateUtf8 } from './text.js';
import { LEASE_TTL_MAX_MS, now } from './timing.js';
import { asObject, isObject, parseObject } from './validation.js';
import { dumps } from './json-text.js';

export const MAX_RETRIES = 3;
/** An outcome that abandons the work needs no check-back; every other outcome of a check-back task does. */
const CHECKBACK_EXEMPT: ReadonlySet<string> = new Set(['refused', 'canceled']);
const REQUEST_CONTEXT_LIMIT = 200;
const REVISION = /^[0-9a-f]{64}$/;
const PIN = /^document:([^@\s]+)(?:@([0-9a-f]{64}))?$/;

const schemas = new Map<string, JsonObject>();

/** The strings of a slot that holds a list of them; anything else is no list. */
const strings = (value: Json | undefined): string[] => (Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []);
/** The numbers of a slot that holds a list of them. */
const numbers = (value: Json | undefined): number[] => (Array.isArray(value) ? value.filter((item): item is number => typeof item === 'number') : []);

/** The catalog field that carries a kind: a plain note keeps `body`. */
export function field(kind: string): string {
  const spec = catalog.messageKinds().find(entry => entry.kind === kind);
  if (!spec) throw new ValidationError('Unknown message kind: ' + kind);
  return spec.field;
}

/** The catalog schema of a kind's slots (`answer` is the `complete` reply). */
export function schema(kind: string): JsonObject {
  let result = schemas.get(kind);
  if (!result) {
    result = kind === 'answer' ? catalog.inputProperty('complete', 'reply') : catalog.inputProperty('send_message', field(kind));
    schemas.set(kind, result);
  }
  return result;
}

/** Every send_message kind, in catalog order (`body` is the `note` kind), then the `answer` kind of `complete`. */
export function messageKinds(): string[] {
  return [...catalog.messageKinds().map(entry => entry.kind), 'answer'];
}

function marker(kind: string): catalog.MessageKind | undefined {
  return catalog.messageKinds().find(entry => entry.kind === kind);
}

/** Whether a kind is a request: it owes an answer and starts with replyRequired. */
export function isRequest(kind: string): boolean {
  return marker(kind)?.request ?? false;
}

/** Whether a kind wakes its recipient by default. */
export function wakesByDefault(kind: string): boolean {
  return marker(kind)?.wake ?? false;
}

function nonblank(value: unknown, where: string): void {
  if (typeof value === 'string' && isBlank(value)) throw new ValidationError(`Invalid ${where}: blank`);
  if (Array.isArray(value)) {
    // An empty list renders a dangling header and cannot be told from "nobody checked".
    if (!value.length) throw new ValidationError(`Invalid ${where}: empty list`);
    for (const item of value) nonblank(item, where);
  } else if (typeof value === 'object' && value !== null) {
    for (const [key, item] of Object.entries(value)) nonblank(item, where + '.' + key);
  }
}

function label(key: string, sub: JsonObject): string {
  if (typeof sub.title === 'string' && sub.title) return sub.title;
  const words = key.replace(/([A-Z])/g, match => ' ' + match.toLowerCase());
  return words.charAt(0).toUpperCase() + words.slice(1).toLowerCase();
}

function item(value: unknown): string {
  return isObject(value) ? `${value.claim} \u2014 ${value.source}` : String(value);
}

/** Labelled lines in catalog slot order; `display` replaces stored IDs with readable values. */
export function render(kind: string, slots: JsonObject, display: JsonObject = {}, lead: readonly string[] = []): string {
  const spec = schema(kind);
  const lines: string[] = [String(spec.title), ...lead];
  for (const [key, sub] of Object.entries(asObject(spec.properties, `${kind} properties`))) {
    if (!(key in slots)) continue;
    const value = key in display ? display[key] : slots[key];
    if (typeof value === 'boolean') {
      if (value) lines.push(label(key, asObject(sub, key)));
    } else if (Array.isArray(value)) {
      lines.push(label(key, asObject(sub, key)) + ':');
      lines.push(...value.map(entry => '- ' + item(entry)));
    } else lines.push(`${label(key, asObject(sub, key))}: ${String(value)}`);
  }
  return lines.join('\n');
}

/** A `document:NAME` source must name a published document; it is stored pinned to its sha256. */
function pinEvidence(host: StoreHost, items: Json | undefined): JsonObject[] {
  return (Array.isArray(items) ? items : []).map(item => {
    const entry = asObject(item, 'evidence item');
    const source = String(entry.source);
    if (!source.startsWith('document:')) return entry;
    const match = PIN.exec(source);
    if (!match) {
      const revision = /^document:([^@\s]+)@(.*)$/s.exec(source);
      if (revision && !REVISION.test(revision[2]!)) throw new ValidationError(`Evidence pins document ${revision[1]} at an invalid revision; use document:${revision[1]} or document:${revision[1]}@<64 lowercase hex sha256>`);
      throw new ValidationError(`Evidence source ${dumps(source)} is not a valid document pin; use document:NAME or document:NAME@<64 lowercase hex sha256>`);
    }
    const record = documentRecord(host, match[1]!);
    if (record === null) throw new ValidationError(`Evidence cites unknown document ${match[1]}; publish it with share_document first`);
    if (match[2] && match[2] !== record.sha256) throw new ValidationError(`Evidence pins document ${match[1]} at another revision; cite document:${match[1]}`);
    return { ...entry, source: `document:${match[1]}@${record.sha256}` };
  });
}

/** Checks cross-references and returns [stored slots, rendered body, conversationId]. */
export function messageSlots(host: StoreHost, session: string, kind: string, input: SendMessageInput, target: string | null): [JsonObject | null, string, string | undefined] {
  let conversation: string | undefined = input.conversationId;
  // A kind whose catalog schema is plain text has no slots.
  if (schema(kind).type === 'string') return [null, catalog.text(input, field(kind)), conversation];
  const source: unknown = Reflect.get(input, kind);
  const slots: JsonObject = isObject(source) ? { ...source } : {};
  const display: JsonObject = {};
  nonblank(slots, kind);
  const direct = 'to' in input;
  if ('informed' in slots) {
    const ids: string[] = [];
    for (const value of strings(slots.informed)) {
      const id = host.resolvePeer(value);
      host.peer(id);
      if (!ids.includes(id) && id !== session && id !== target) ids.push(id);
    }
    if (ids.length) {
      slots.informed = ids;
      display.informed = ids.map(id => host.peer(id).name);
    } else delete slots.informed;
  }
  if (kind === 'decision') {
    const options = strings(slots.options);
    if (new Set(options).size !== options.length) throw new ValidationError('Decision options must be distinct');
    for (const key of ['recommendation', 'default']) if (!options.some(option => option === slots[key])) throw new ValidationError(`Decision ${key} must be one of its options`);
  }
  if (kind === 'checkback') {
    // A check-back restates a task's objective and Definition of Done; no other request has them.
    const rows = query(host.db, "SELECT m.sender,m.conversationId FROM messages m JOIN deliveries d ON d.message=m.id WHERE m.id=? AND d.recipient=? AND d.acknowledgedAt IS NULL AND m.kind='task'", [Number(slots.request), session]);
    if (!rows.length) throw new ValidationError('checkback.request must be a pending task you received: item.id from inbox');
    if (!direct || target !== rowText(rows[0]!, 'sender')) throw new ValidationError(`Send the checkback to the sender of request ${String(slots.request)}`);
    const parent = rowOptionalText(rows[0]!, 'conversationId');
    if (conversation !== undefined && conversation !== parent) throw new ValidationError('Checkback conversationId must match its request');
    conversation = parent;
  }
  if (kind === 'handoff') {
    if ('evidence' in slots) slots.evidence = pinEvidence(host, slots.evidence);
    if ('leases' in slots) {
      const held = numbers(slots.leases);
      if (!direct) throw new ValidationError('A handoff that moves leases needs one recipient: use to');
      const rows = query(host.db, 'SELECT id,path,kind,workspace FROM leases WHERE owner=? AND expiresAt>? AND id IN (SELECT value FROM json_each(?)) ORDER BY id', [session, now(), dumps(held)]);
      if (rows.length !== new Set(held).size) throw new ValidationError('handoff.leases must be live leases you own; check locks');
      const recipient: string = host.peer(target!).workspace;
      if (rows.some(row => rowText(row, 'workspace') !== recipient)) throw new ValidationError('Leases protect one worktree and the recipient works in another; unlock them and let it lock its own paths');
      slots.leases = rows.map(row => rowInteger(row, 'id'));
      display.leases = rows.map(row => `${rowInteger(row, 'id')} ${relativePath(rowText(row, 'path'), rowText(row, 'workspace'))} (${rowText(row, 'kind')})`);
    }
  }
  return [slots, render(kind, slots, display), conversation];
}

/** The first text slot of a request, as its headline. */
function headline(kind: string, slots: JsonObject): string | undefined {
  for (const key of Object.keys(asObject(schema(kind).properties, `${kind} properties`))) {
    const value = slots[key];
    if (typeof value === 'string') return value;
  }
  return undefined;
}

/**
 * What an informed peer needs to read an answer to a request it never received: the request's kind, ID and headline.
 * Only a request this session sent or received is looked at, and only one that names informed peers.
 */
function requestContext(host: StoreHost, session: string, parent: number): string | undefined {
  const rows = query(host.db, 'SELECT m.id,m.kind,m.slots FROM messages m WHERE m.id=? AND m.slots IS NOT NULL AND (m.sender=? OR EXISTS(SELECT 1 FROM deliveries d WHERE d.message=m.id AND d.recipient=?))', [parent, session, session]);
  if (!rows.length) return undefined;
  const slots = parseObject(rowText(rows[0]!, 'slots'), 'message slots');
  if (!strings(slots.informed).length) return undefined;
  const kind = rowText(rows[0]!, 'kind');
  const text = headline(kind, slots);
  return text === undefined ? undefined : `Re ${kind} ${parent}: ${truncateCodePoints(text, REQUEST_CONTEXT_LIMIT)}${codePointLength(text) > REQUEST_CONTEXT_LIMIT ? '\u2026' : ''}`;
}

/** A final reply is a BLUF: outcome and conclusion first, evidence pinned, gaps named; informed copies carry the request context. */
export function answerSlots(host: StoreHost, session: string, input: CompleteInput): [JsonObject, string] {
  const slots: JsonObject = asObject({ ...input.reply }, 'reply');
  nonblank(slots, 'reply');
  if ('evidence' in slots) slots.evidence = pinEvidence(host, slots.evidence);
  const context = requestContext(host, session, input.message!);
  return [slots, render('answer', slots, {}, [...(context === undefined ? [] : [context]), 'Outcome: ' + input.outcome])];
}

/** Effects a final reply has on its request, inside the reply transaction. */
export function finishRequest(host: StoreHost, session: string, parent: number, reply: number, outcome: string, at: number): Pick<Extract<CommandResults['complete'], { recipients: number }>, 'leases' | 'leasesNotMoved'> {
  if (!host.db.inTransaction) throw new Error('finishRequest must run inside the reply transaction');
  const rows = query(host.db, 'SELECT kind,slots,sender FROM messages WHERE id=?', [parent]);
  const kind = rowText(rows[0]!, 'kind');
  const slots = parseObject(rowOptionalText(rows[0]!, 'slots') ?? '{}', 'message slots');
  if (kind === 'task' && slots.checkBack && !CHECKBACK_EXEMPT.has(outcome) && !query(host.db, "SELECT 1 FROM messages WHERE sender=? AND kind='checkback' AND json_extract(slots,'$.request')=? LIMIT 1", [session, parent]).length)
    throw new ValidationError(`Task ${parent} asks for a check-back: send checkback {request:${parent},objective,done} to its sender before completing it ${outcome}`);
  for (const id of strings(slots.informed)) if (id !== session) execute(host.db, 'INSERT OR IGNORE INTO deliveries(message,recipient) VALUES(?,?)', [reply, id]);
  const handed = numbers(slots.leases);
  if (kind !== 'handoff' || outcome !== 'done' || !handed.length) return {};
  const moved: { id: number; path: string; kind: 'file' | 'tree'; expiresAt: number }[] = [];
  for (const lease of query(host.db, 'SELECT * FROM leases WHERE owner=? AND workspace=? AND expiresAt>? AND id IN (SELECT value FROM json_each(?)) ORDER BY id', [rowText(rows[0]!, 'sender'), host.workspace, at, dumps(handed)])) {
    execute(host.db, 'DELETE FROM leases WHERE id=?', [rowInteger(lease, 'id')]);
    const reasoning = truncateUtf8(`Handoff ${parent}: ${rowText(lease, 'reasoning')}`, 512);
    const expires = at + Math.min(LEASE_TTL_MAX_MS, Math.max(1000, rowInteger(lease, 'expiresAt') - rowInteger(lease, 'refreshedAt')));
    const id = host.db.run('INSERT INTO leases(workspace,path,kind,owner,acquiredAt,refreshedAt,expiresAt,reasoning,pathKey) VALUES(?,?,?,?,?,?,?,?,?)', [rowText(lease, 'workspace'), rowText(lease, 'path'), rowText(lease, 'kind'), session, at, at, expires, reasoning, rowText(lease, 'pathKey')]).lastInsertRowid;
    moved.push({ id, path: relativePath(rowText(lease, 'path'), rowText(lease, 'workspace')), kind: leaseKind(rowText(lease, 'kind')), expiresAt: expires });
  }
  return moved.length === handed.length ? { leases: moved } : { leases: moved, leasesNotMoved: 'The sender no longer holds some handed-off leases; lock those paths yourself' };
}
