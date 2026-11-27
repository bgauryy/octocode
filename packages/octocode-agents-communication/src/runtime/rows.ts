import { parseParts, parseOutcome } from './message-content.js';
/** Typed readings of database rows: each column is checked once, so schema drift fails loudly instead of reading `undefined`. */
import { rowInteger, rowOptionalInteger, rowOptionalText, rowText, type Row } from './database.js';
import type { CommandResults, Lease, RecordEnvelope } from './catalog-types.generated.js';

/** The identity columns of a session row. */
export type Session = Pick<CommandResults['binding'], 'id' | 'workspace' | 'name' | 'vendor' | 'expiresAt' | 'task' | 'status' | 'vendorSession' | 'branch'>;
/** One pending message as `inbox` lists it. */
export type InboxItem = CommandResults['inbox']['items'][number];
export type LeaseKind = Lease['kind'];
export type Transport = CommandResults['attach']['transport'];
/** A delivery record offered to a recipient: the inbox item, its dispatch token and the full record envelope. */
export type Offered = InboxItem & { dispatchToken: string; record: RecordEnvelope };

/** `value` when it is one of `allowed`, typed as that member. */
function oneOf<T extends string>(value: string, allowed: readonly T[], what: string): T {
  const match = allowed.find(item => item === value);
  if (match === undefined) throw new TypeError(`${what} ${value} is not one of ${allowed.join(', ')}`);
  return match;
}

const STATUSES: readonly Session['status'][] = ['available', 'busy', 'blocked', 'unknown'];
const KINDS: readonly LeaseKind[] = ['file', 'tree'];
const WAKES: readonly InboxItem['wake'][] = ['action', 'passive'];

const TRANSPORTS: readonly Transport[] = ['raw', 'claude', 'codex', 'opencode', 'grok'];

export const transportName = (value: string): Transport => oneOf(value, TRANSPORTS, 'transport');
export const leaseKind = (value: string): LeaseKind => oneOf(value, KINDS, 'lease kind');

export function sessionRow(row: Row): Session {
  return {
    id: rowText(row, 'id'), workspace: rowText(row, 'workspace'), name: rowText(row, 'name'), vendor: rowText(row, 'vendor'),
    vendorSession: rowOptionalText(row, 'vendorSession'), branch: rowOptionalText(row, 'branch'), expiresAt: rowInteger(row, 'expiresAt'),
    task: rowText(row, 'task'), status: oneOf(rowText(row, 'status'), STATUSES, 'session status'),
  };
}

/** The columns `inbox` selects: `m.id,m.sender,senderName,m.parts,m.outcome,m.reasoning,m.topic,m.expiresAt,m.wake,m.contextId,m.replyTo,m.replyRequired`. */
export function inboxItem(row: Row): InboxItem {
  return {
    id: rowInteger(row, 'id'), sender: rowText(row, 'sender'), senderName: rowText(row, 'senderName'),
    parts: parseParts(rowText(row, 'parts')), outcome: parseOutcome(rowOptionalText(row, 'outcome')), reasoning: rowText(row, 'reasoning'), topic: rowOptionalText(row, 'topic'), expiresAt: rowInteger(row, 'expiresAt'),
    wake: oneOf(rowText(row, 'wake'), WAKES, 'wake'), contextId: rowOptionalText(row, 'contextId'), replyTo: rowOptionalInteger(row, 'replyTo'),
    replyRequired: row.replyRequired === true,
  };
}
