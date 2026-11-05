/** Bounded delivery diagnostics; observations never replay or acknowledge mail. */
import { query, readTransaction, rowInteger, rowOptionalInteger, rowText, type Row } from './database.js';
import type { CommandResults, HealthInput } from './catalog-types.generated.js';
import { deliveryOwnerLive } from './owner-lock.js';
import type { StoreHost } from './store-types.js';
import { now } from './timing.js';

type Issue = CommandResults['health']['issues'][number]['issue'];
const INSTRUCTIONS: Record<Issue, string> = {
  uncertain: 'Inspect the recipient and dispatch token before any explicit retry; context may have arrived.',
  expiredAction: 'Inspect delivery and the recipient before sending a new request with a new key; expired submitted context may already be in progress.',
  offlineRecipient: 'Check the recipient and its delivery owner; resume deliberately. Do not revive its expired leases.',
  stalledOffer: 'Inspect the in-flight owner and recipient before recovery; elapsed time is not permission to replay.',
  overdueHandling: "Check the recipient's progress. Submission is not handling; do not automatically complete or resend.",
};

/** Each delivery issue the query can classify, as the result schema names it. */
const ISSUES: readonly Issue[] = ['uncertain', 'expiredAction', 'offlineRecipient', 'stalledOffer', 'overdueHandling'];

function issueOf(row: Row): Issue {
  const value = rowText(row, 'issue');
  const match = ISSUES.find(issue => issue === value);
  if (match === undefined) throw new TypeError(`Unknown delivery issue ${value}`);
  return match;
}

export function health(host: StoreHost, input: HealthInput): CommandResults['health'] {
  const at = now();
  const threshold = input.staleAfterMs ?? 300000;
  const limit = input.limit ?? 25;
  const orphaned = at - Math.min(threshold, 30000);
  // One probe per recipient per call (it may spawn `ps`): the snapshot is a few ms old at most, and nothing outlives the call.
  const live = new Map<string, boolean>();
  host.db.raw.function('delivery_owner_live', { deterministic: false }, session => {
    const recipient = String(session);
    if (!live.has(recipient)) live.set(recipient, deliveryOwnerLive(host.database, recipient));
    return live.get(recipient) ? 1 : 0;
  });
  const pending = `WITH pending AS (
SELECT d.message,d.recipient,s.name,s.vendor,m.wake,m.expiresAt,
coalesce(x.state,'queued') AS state,x.attemptedAt,x.submittedAt,
CASE WHEN x.state='uncertain' THEN 'uncertain'
WHEN (m.wake='action' OR m.replyRequired=1) AND m.expiresAt<=?2 THEN 'expiredAction'
WHEN m.wake='action' AND s.expiresAt<=?2 THEN 'offlineRecipient'
WHEN x.state='staged' AND (x.attemptedAt<=?3 OR (x.attemptedAt<=${orphaned} AND NOT delivery_owner_live(x.recipient))) THEN 'stalledOffer'
WHEN x.state='submitted' AND m.wake='action' AND x.submittedAt<=?3 THEN 'overdueHandling'
ELSE NULL END AS issue FROM sessions s JOIN deliveries d ON d.recipient=s.id AND d.acknowledgedAt IS NULL
JOIN messages m ON m.id=d.message LEFT JOIN dispatches x ON x.message=d.message AND x.recipient=d.recipient WHERE s.workspace=?1) `;
  const params = [host.workspace, at, at - threshold];
  const [counts, found] = readTransaction(host.db, () => {
    const summary = query(host.db, `${pending}SELECT count(*) AS unacknowledged,
coalesce(sum(issue IS NOT NULL),0) AS attention,
coalesce(sum(state IN ('queued','ready') AND wake='action' AND expiresAt>?2),0) AS queuedAction,
coalesce(sum(state IN ('queued','ready') AND wake='passive' AND expiresAt>?2),0) AS waitingPassive,
coalesce(sum(state='staged'),0) AS staged, coalesce(sum(state='submitted'),0) AS submittedUnacknowledged,
coalesce(sum(state='uncertain'),0) AS uncertain, coalesce(sum(expiresAt<=?2),0) AS expiredUnacknowledged FROM pending`, params)[0]!;
    const cursor = input.after ?? { message: 0, recipient: '' };
    const rows = query(host.db, `${pending}SELECT message,recipient,name,vendor,state,issue,attemptedAt,submittedAt FROM pending WHERE issue IS NOT NULL AND (message>?4 OR (message=?4 AND recipient>?5)) ORDER BY message,recipient LIMIT ?6`, [...params, cursor.message, cursor.recipient, limit + 1]);
    return [summary, rows] as const;
  });
  const more = found.length > limit;
  const issues = found.slice(0, limit);
  const tally = {
    unacknowledged: rowInteger(counts, 'unacknowledged'), attention: rowInteger(counts, 'attention'), queuedAction: rowInteger(counts, 'queuedAction'), waitingPassive: rowInteger(counts, 'waitingPassive'),
    staged: rowInteger(counts, 'staged'), submittedUnacknowledged: rowInteger(counts, 'submittedUnacknowledged'), uncertain: rowInteger(counts, 'uncertain'), expiredUnacknowledged: rowInteger(counts, 'expiredUnacknowledged'),
  };
  const status = tally.attention ? 'attention' : tally.queuedAction || tally.staged || tally.submittedUnacknowledged ? 'pending' : 'clear';
  const listed = issues.map(row => ({
    message: rowInteger(row, 'message'), recipient: rowText(row, 'recipient'), name: rowText(row, 'name'), vendor: rowText(row, 'vendor'), state: rowText(row, 'state'), issue: issueOf(row),
    attemptedAt: rowOptionalInteger(row, 'attemptedAt'), submittedAt: rowOptionalInteger(row, 'submittedAt'),
  }));
  const last = listed.at(-1);
  return {
    status, workspace: host.workspace, observedAt: at, staleAfterMs: threshold, counts: tally, issues: listed, actions: Object.fromEntries(listed.map(row => [row.issue, INSTRUCTIONS[row.issue]])),
    ...(more && last ? { next: { command: 'health', input: { staleAfterMs: threshold, limit, after: { message: last.message, recipient: last.recipient } } } } : {}),
  };
}
