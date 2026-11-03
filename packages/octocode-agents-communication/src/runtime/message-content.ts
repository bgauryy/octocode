/** One opaque content envelope for peer messages and runtime notices. */
import * as catalog from './catalog.js';
import type { SendMessageInput, Outcome } from './catalog-types.generated.js';
import { execute, type Database } from './database.js';
import { compact } from './json-text.js';
import { HOUR_MS } from './timing.js';
import { validate } from './validation.js';

export function parseParts(value: string): SendMessageInput['parts'] {
  const parts: unknown = JSON.parse(value);
  validateParts(parts);
  return parts;
}

function validateParts(parts: unknown): asserts parts is SendMessageInput['parts'] {
  validate(catalog.inputProperty('send_message', 'parts'), parts);
}

export function parseOutcome(value: unknown): Outcome | undefined {
  if (value === undefined) return value;
  validateOutcome(value);
  return value;
}

function validateOutcome(value: unknown): asserts value is Outcome {
  validate(catalog.inputProperty('complete', 'outcome'), value);
}

/** Caller owns the transaction. Notices never create a request or mutate other domain state. */
export function notice(db: Database, input: { sender: string; target: string; parts: SendMessageInput['parts']; key: string; wake: string }, at: number): void {
  const { sender, target, parts, key, wake } = input;
  validate(catalog.inputProperty('send_message', 'parts'), parts);
  const message = db.run('INSERT INTO messages(sender,target,parts,key,expiresAt,reasoning,wake,ttlMs,replyRequired) VALUES(?,?,?,?,?,?,?,?,0)', [sender, target, compact(parts), key, at + HOUR_MS, 'Runtime notice', wake, HOUR_MS]).lastInsertRowid;
  execute(db, 'INSERT INTO deliveries(message,recipient) VALUES(?,?)', [message, target]);
}
