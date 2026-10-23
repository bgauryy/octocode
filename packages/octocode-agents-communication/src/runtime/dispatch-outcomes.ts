/** Offer outcomes: submitted, uncertain or released dispatch rows, bounded retry and the usage audit. */
import * as catalog from './catalog.js';
import type { CommandResults, RecordUsageInput, RetryDeliveryInput } from './catalog-types.generated.js';
import { execute, query, rowInteger, rowText, transaction } from './database.js';
import type { Dispatched } from './delivery-types.js';
import { ValidationError } from './errors.js';
import { retryKey } from './expiry.js';
import { notice } from './leases.js';
import { MAX_RETRIES } from './protocols.js';
import { append } from './records.js';
import type { StoreHost } from './store-types.js';
import { now } from './timing.js';
import { jsonEqual } from './validation.js';

/** A budget deferral reoffers mail at a later hook event; it is not a retry. */
export const BUDGET_DEFERRAL = 'Host context budget; offered at a later hook event';
const RETRY_BACKOFF_MS = 2000;

/** Records the outcome of an offer: submitted, or uncertain when `error` is given. Staged rows are never replayed. */
export function finishDispatch(host: StoreHost, session: string, items: readonly Dispatched[], error: string | null = null): void {
  transaction(host.db, () => {
    host.known(session, false);
    for (const item of items) {
      const rows = query(host.db, 'SELECT state FROM dispatches WHERE message=? AND recipient=? AND token=?', [item.id, session, item.dispatchToken]);
      const state = rows.length ? rowText(rows[0], 'state') : null;
      if (state === 'submitted' && error === null) continue;
      if (state !== 'staged') throw new ValidationError('Unknown or superseded dispatch token; inspect before retrying');
      execute(host.db, "UPDATE dispatches SET state=?,submittedAt=?,error=? WHERE message=? AND recipient=? AND token=? AND state='staged'", [error !== null ? 'uncertain' : 'submitted', error !== null ? null : now(), error, item.id, session, item.dispatchToken]);
    }
  });
}

export function releaseDispatch(host: StoreHost, session: string, items: readonly Dispatched[], reason: string): void {
  transaction(host.db, () => {
    for (const item of items) execute(host.db, "UPDATE dispatches SET state='ready',error=? WHERE message=? AND recipient=? AND token=? AND state='staged'", [reason, item.id, session, item.dispatchToken]);
  });
}

/**
 * Bounded retry: the first reoffer is immediate, later ones back off exponentially, and after MAX_RETRIES the
 * runtime stops and tells the message sender with an SBAR blocker.
 */
export function retryDelivery(host: StoreHost, session: string, input: RetryDeliveryInput): CommandResults['retry_delivery'] {
  catalog.command('retry_delivery', input);
  return transaction(host.db, () => {
    const identity = host.known(session, true);
    if (query(host.db, "SELECT 1 FROM dispatches WHERE recipient=? AND message=? AND state='submitted'", [session, input.message]).length) throw new ValidationError(`Message ${input.message} was already submitted: the host accepted it, so it is never resent. Check the recipient, wait for complete, or send a new request with a new key`);
    const retries = query(host.db, "SELECT timestamp FROM records WHERE path=? AND \"from\"=? AND entityId=? AND type GLOB 'dispatch.*' AND type='dispatch.ready' AND json_extract(data,'$.error') IS NOT ? ORDER BY id", [host.workspace, session, String(input.message), BUDGET_DEFERRAL]).map(row => rowInteger(row, 'timestamp'));
    const at = now();
    if (retries.length >= MAX_RETRIES) {
      const sender = query(host.db, 'SELECT sender FROM messages WHERE id=?', [input.message]);
      const key = retryKey(input.message, session);
      if (sender.length && rowText(sender[0], 'sender') !== session && !query(host.db, 'SELECT 1 FROM messages WHERE sender=? AND key=?', [session, key]).length) {
        notice(host.db, session, rowText(sender[0], 'sender'), 'blocker', {
          situation: `Message ${input.message} failed delivery to ${identity.name} after ${retries.length} retries`,
          background: input.reason,
          assessment: 'More retries could duplicate work; the message stays pending',
          recommendation: 'Check the recipient, then send a new request with a new key or let this one expire',
        }, key, 'action', at);
      }
      return { ready: false, exhausted: true, retries: retries.length, guidance: 'Retry budget spent and the sender was told; leave the message pending' };
    }
    if (retries.length) {
      const wait = RETRY_BACKOFF_MS * 2 ** (retries.length - 1) - (at - retries[retries.length - 1]);
      if (wait > 0) return { ready: false, retries: retries.length, retryAfterMs: wait };
    }
    const changed = execute(host.db, "UPDATE dispatches SET state='ready',error=? WHERE recipient=? AND message=? AND state NOT IN ('ready','submitted') AND EXISTS(SELECT 1 FROM deliveries d JOIN messages m ON m.id=d.message WHERE d.message=dispatches.message AND d.recipient=dispatches.recipient AND d.acknowledgedAt IS NULL AND m.expiresAt>?)", [input.reason, session, input.message, at]);
    return { ready: changed === 1, retries: retries.length + changed };
  });
}

export function recordUsage(host: StoreHost, session: string, input: RecordUsageInput): CommandResults['record_usage'] {
  catalog.command('record_usage', input);
  return transaction(host.db, () => {
    host.known(session, false);
    const rows = query(host.db, "SELECT data FROM records WHERE [from]=? AND type='usage' AND key=?", [session, input.key]);
    if (rows.length) {
      if (!jsonEqual(JSON.parse(rowText(rows[0], 'data')), input)) throw new ValidationError('Usage key already exists with different data');
      return { recorded: false };
    }
    append(host.db, session, 'usage', input, now(), input.key);
    return { recorded: true };
  });
}
