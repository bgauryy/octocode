/** Session lifecycle: join, resume, presence heartbeats, status, subscriptions, leave and lease pruning. */
import { randomUUID } from 'node:crypto';
import * as catalog from './catalog.js';
import type { CommandResults, HeartbeatInput, JoinInput, ResumeInput, SetStatusInput, SubscribeInput } from './catalog-types.generated.js';
import { execute, query, rowInteger, rowText, transaction } from './database.js';
import type { DeliveryApi } from './delivery-types.js';
import { ValidationError } from './errors.js';
import { notifyExpired } from './expiry.js';
import { dumps } from './json-text.js';
import { forgoTakenLeases, grantWaits } from './leases.js';
import { ownedHere } from './owner-lock.js';
import { sessionRow, type Session } from './rows.js';
import type { StoreHost } from './store-types.js';
import { LEASE_TTL_MAX_MS, SESSION_TTL_MS, now } from './timing.js';
import { parseObject } from './validation.js';
import { currentBranch, registerWorkspace } from './workspace.js';

/** What the lifecycle commands need from the bound store. */
type SessionHost = StoreHost & Pick<DeliveryApi, 'ensureBindingChangeAllowed'>;

/** Names of live peers in this repository scope, other than `exclude`. */
export function liveNames(host: SessionHost, exclude = ''): Set<string> {
  return new Set(query(host.db, 'SELECT name FROM sessions WHERE workspace IN (SELECT value FROM json_each(?)) AND expiresAt>? AND id<>?', [dumps(host.sharedWorkspaces()), now(), exclude]).map(row => rowText(row, 'name')));
}

export function join(host: SessionHost, input: JoinInput): CommandResults['join'] {
  const branch: string | null = input.branch !== undefined ? input.branch : currentBranch(host.workspace);
  return transaction(host.db, () => {
    registerWorkspace(host.db, host.workspace, host.coordinationScope);
    const id = randomUUID();
    const name = host.uniqueName(catalog.text(input, 'name'));
    const vendor = catalog.text(input, 'vendor');
    if ('vendorSession' in input) catalog.text(input, 'vendorSession');
    const expires = now() + SESSION_TTL_MS;
    execute(host.db, 'INSERT INTO sessions(id,workspace,name,vendor,vendorSession,branch,expiresAt,task,status) VALUES(?,?,?,?,?,?,?,?,?)', [id, host.workspace, name, vendor, input.vendorSession ?? null, branch, expires, input.task ?? '', input.status ?? 'unknown']);
    return { id, name, vendor, vendorSession: input.vendorSession, expiresAt: expires };
  });
}

export function resume(host: SessionHost, session: string, input: ResumeInput): CommandResults['resume'] {
  return transaction(host.db, () => {
    const row = host.known(session, false);
    if (row.vendor !== catalog.text(input, 'vendor') || row.expiresAt > now()) throw new ValidationError('Vendor mismatch or session still active');
    revive(host, session, row);
    return withCheckpoint(host, session, host.known(session));
  });
}

/** Recovers an expired identity as a new presence: no leases, no claims, a free name. */
function revive(host: SessionHost, session: string, row: Session): void {
  execute(host.db, 'DELETE FROM leases WHERE owner=?', [session]);
  execute(host.db, 'DELETE FROM lease_waits WHERE owner=?', [session]);
  execute(host.db, 'UPDATE deliveries SET claimUntil=0,claimedBy=NULL WHERE recipient=? AND acknowledgedAt IS NULL', [session]);
  execute(host.db, 'UPDATE sessions SET expiresAt=?,name=? WHERE id=?', [now() + SESSION_TTL_MS, host.uniqueName(row.name, session), session]);
  grantWaits(host, now());
}

/**
 * An expired row whose owner process holds the live delivery-owner lock was not crashed: the process slept or stalled past
 * the presence TTL. Such a session renews itself and keeps its leases, except those another owner took meanwhile. If its
 * name was taken by a newer join the identity is revived as `resume` would (no leases, a free name). Without the lock
 * nothing changes: expiry still means the owner died. An explicit `leave` (the row's latest coordination event is
 * `coordinate.out`) is an end the owner chose, not a stall: it is never undone.
 */
function reclaim(host: SessionHost, session: string): void {
  const rows = query(host.db, 'SELECT * FROM sessions WHERE id=? AND workspace=?', [session, host.workspace]);
  const at = now();
  if (!rows.length || rowInteger(rows[0], 'expiresAt') > at || !ownedHere(host.database, session)) return;
  const last = query(host.db, "SELECT type FROM records WHERE \"from\"=? AND type IN ('coordinate.in','coordinate.update','coordinate.out') ORDER BY id DESC LIMIT 1", [session])[0];
  if (last !== undefined && rowText(last, 'type') === 'coordinate.out') return;
  const row = sessionRow(rows[0]);
  if (liveNames(host, session).has(row.name)) return revive(host, session, row);
  forgoTakenLeases(host, session, at);
  execute(host.db, 'UPDATE sessions SET expiresAt=? WHERE id=?', [at + SESSION_TTL_MS, session]);
}

export function heartbeat(host: SessionHost, session: string, input: HeartbeatInput): CommandResults['heartbeat'] {
  return transaction(host.db, () => {
    reclaim(host, session);
    const identity = host.known(session);
    if (input.vendorSession !== undefined) {
      catalog.text(input, 'vendorSession');
      validateVendorSessionUpdate(host, session, identity, input.vendorSession);
    }
    const at = now();
    if (identity.expiresAt <= at) throw new ValidationError('Session expired before heartbeat; stop this worker');
    const expires = at + catalog.ttl(input, SESSION_TTL_MS, LEASE_TTL_MAX_MS);
    execute(host.db, 'UPDATE sessions SET expiresAt=?,vendorSession=coalesce(?,vendorSession),task=coalesce(?,task),status=coalesce(?,status) WHERE id=?', [expires, input.vendorSession ?? null, input.task ?? null, input.status ?? null, session]);
    if (input.name !== undefined) {
      if (liveNames(host, session).has(catalog.text(input, 'name'))) throw new ValidationError(`Name ${input.name} belongs to a live peer; choose another name`);
      execute(host.db, 'UPDATE sessions SET name=? WHERE id=?', [input.name, session]);
    }
    if (input.branch !== undefined) execute(host.db, 'UPDATE sessions SET branch=? WHERE id=?', [input.branch, session]);
    const renewedLeases = input.renewLeases ? execute(host.db, 'UPDATE leases SET expiresAt=?,refreshedAt=? WHERE owner=? AND workspace=? AND expiresAt>? AND expiresAt<?', [expires, at, session, host.workspace, at, expires]) : undefined;
    // Periodic heartbeats hand silently expired leases to waiters and tell the sender of any request that expired unacknowledged.
    grantWaits(host, at);
    notifyExpired(host, session, at);
    return { alive: true, expiresAt: expires, renewedLeases };
  });
}

export function setStatus(host: SessionHost, session: string, input: SetStatusInput): CommandResults['set_status'] {
  return transaction(host.db, () => {
    host.known(session);
    execute(host.db, 'UPDATE sessions SET task=coalesce(?,task),status=coalesce(?,status) WHERE id=?', [input.task ?? null, input.status ?? null, session]);
    const row = host.known(session);
    return { id: row.id, task: row.task, status: row.status };
  });
}

export function leave(host: SessionHost, session: string): CommandResults['leave'] {
  return transaction(host.db, () => {
    host.known(session, false);
    execute(host.db, 'DELETE FROM leases WHERE owner=?', [session]);
    execute(host.db, 'DELETE FROM lease_waits WHERE owner=?', [session]);
    execute(host.db, 'DELETE FROM subscriptions WHERE session=?', [session]);
    execute(host.db, 'UPDATE deliveries SET claimUntil=0,claimedBy=NULL WHERE recipient=? AND acknowledgedAt IS NULL', [session]);
    // Never raises expiresAt, so the trigger's `NEW<OLD AND NEW<=now` arm always appends coordinate.out, which `reclaim` reads as the end.
    execute(host.db, 'UPDATE sessions SET expiresAt=min(?,expiresAt-1) WHERE id=?', [now(), session]);
    grantWaits(host, now());
    return { left: true };
  });
}

export function subscribe(host: SessionHost, session: string, input: SubscribeInput): CommandResults['subscribe'] {
  return transaction(host.db, () => {
    host.known(session);
    const topics = input.topics;
    for (const topic of topics) catalog.text({ topic }, 'topic');
    execute(host.db, 'DELETE FROM subscriptions WHERE session=? AND topic NOT IN (SELECT value FROM json_each(?))', [session, dumps(topics)]);
    for (const topic of topics) execute(host.db, 'INSERT OR IGNORE INTO subscriptions VALUES(?,?)', [session, topic]);
    return { subscribed: true };
  });
}

export function prune(host: SessionHost): CommandResults['prune'] {
  return transaction(host.db, () => {
    const at = now();
    const removed = execute(host.db, 'DELETE FROM leases WHERE id IN (SELECT l.id FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=? AND (l.expiresAt<=? OR s.expiresAt<=?) ORDER BY l.id LIMIT 100)', [host.workspace, at, at]);
    grantWaits(host, at);
    return removed === 100 ? { removed, next: { command: 'prune', input: {} } } : { removed };
  });
}

/** Checkpoint-resume: an identity read returns the agent's latest checkpoint record. */
export function withCheckpoint(host: SessionHost, session: string, identity: Session & { coordinationScope?: string; active?: boolean }): CommandResults['binding'] {
  const rows = query(host.db, "SELECT id,timestamp,data FROM records WHERE \"from\"=? AND type='checkpoint' ORDER BY id DESC LIMIT 1", [session]);
  if (!rows.length) return identity;
  return { ...identity, checkpoint: { recordId: rowInteger(rows[0], 'id'), timestamp: rowInteger(rows[0], 'timestamp'), data: parseObject(rowText(rows[0], 'data'), 'checkpoint data') } };
}

function validateVendorSessionUpdate(host: SessionHost, session: string, identity: Session, value: string): void {
  if (identity.vendorSession !== value) {
    host.ensureBindingChangeAllowed(session);
    if (query(host.db, "SELECT 1 FROM attachments WHERE session=? AND transport<>'raw'", [session]).length) throw new ValidationError("Use attach to change a native receiver's vendorSession");
  }
}
