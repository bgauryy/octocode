/** One bounded host Stop check, without auto-completion or delivery ownership. */
import { query, readTransaction, rowInteger } from './database.js';
import type { CommandResults, CompletionCheckInput } from './catalog-types.generated.js';
import { ValidationError } from './errors.js';
import { realpath } from './paths.js';
import type { AttachmentHost } from './store-types.js';
import { now } from './timing.js';
import { compactAscii } from './json-text.js';

export function completionCheck(host: AttachmentHost, session: string, input: CompletionCheckInput): CommandResults['completion-check'] {
  if (input.hook_event_name !== 'Stop' || input.stop_hook_active === true) return {};
  const pending = readTransaction(host.db, () => {
    const identity = host.known(session);
    if (identity.vendorSession !== input.session_id || realpath(input.cwd ?? '', true) !== host.workspace) throw new ValidationError('Completion check requires the bound native session and workspace');
    const attachment = host.attachment(session);
    if (attachment.transport !== 'claude' && !(identity.vendor === 'pi' && attachment.transport === 'raw')) throw new ValidationError('Completion check requires a Claude native binding or Pi raw binding');
    return query(host.db, "SELECT DISTINCT m.id FROM deliveries d JOIN messages m ON m.id=d.message JOIN dispatches x ON x.message=d.message AND x.recipient=d.recipient WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND x.state='submitted' ORDER BY m.id LIMIT 17", [session, now()]);
  });
  if (!pending.length) return {};
  const ids = pending.slice(0, 16).map(row => rowInteger(row, 'id'));
  const reason = `Peer work is still unacknowledged: IDs ${compactAscii(ids)}${pending.length > 16 ? ' (more pending; use inbox recovery as needed)' : ''}. Handle these IDs from existing context; only if a body is missing, recover it with inbox(message:ID). Finish requests with complete {message:ID,outcome,reply:{conclusion}}; handled FYIs/answers with complete {messages:[IDs]} without a reply. Leave unfinished work pending. If genuinely blocked, explain why and stop; this check will not block the recovery turn again.`;
  return { decision: 'block', pending: ids, reason };
}
