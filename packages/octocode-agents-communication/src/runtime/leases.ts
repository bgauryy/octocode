/** Atomic, overlap-aware advisory leases and read-only write checks. */
import * as catalog from './catalog.js';
import { execute, query, readTransaction, rowInteger, rowText, transaction, type Database, type Row, type SqlValue } from './database.js';
import type { CheckPathsInput, CheckWriteInput, CommandResults, Lease, LockInput, LockManyInput, LockRefused, LocksInput, PathTarget, RenewInput, UnlockInput } from './catalog-types.generated.js';
import { leaseKind, type LeaseKind } from './rows.js';
import { parseObjects, type JsonObject } from './validation.js';
import { ValidationError } from './errors.js';
import { ancestors, isDirectory, keysOverlap, leaseKey, resolvePath } from './paths.js';
import { render } from './protocols.js';
import { isWithin, relativePath, type StoreHost } from './store-types.js';
import { compareCodePoints } from './text.js';
import { HOUR_MS, LEASE_TTL_DEFAULT_MS, LEASE_TTL_MAX_MS, now } from './timing.js';
import { compact, dumps } from './json-text.js';
import { scopeWorkspaces } from './workspace.js';

const OVERLAPS = `WITH hits(id) AS (
SELECT id FROM leases WHERE workspace IN (SELECT value FROM json_each(?1)) AND pathKey=?2
UNION ALL SELECT id FROM leases WHERE workspace IN (SELECT value FROM json_each(?1)) AND kind='tree' AND pathKey IN (SELECT value FROM json_each(?3))
UNION ALL SELECT id FROM leases WHERE ?4='tree' AND workspace IN (SELECT value FROM json_each(?1)) AND pathKey>?5 AND pathKey<?6)
SELECT l.id,l.path,l.kind,l.owner,l.acquiredAt,l.refreshedAt,l.expiresAt,l.reasoning,s.name AS ownerName,s.vendor AS ownerVendor,s.expiresAt AS ownerExpiresAt
FROM hits h JOIN leases l ON l.id=h.id JOIN sessions s ON s.id=l.owner
WHERE l.expiresAt>?7 AND s.expiresAt>?7 AND l.owner<>?8 AND l.id>?10 ORDER BY l.id LIMIT ?9`;

const WAIT_TTL = LEASE_TTL_MAX_MS;
const QUEUED = 'Queued; owners were notified. Keep the request that needs this lease pending and end your turn. A "Lease granted" message wakes you with the lease ID.';
/** Beyond every row ID: "no upper bound" for an ordering cursor. */
const NO_BOUND = 4611686018427387904n;

/** Runtime-authored FYI in a message protocol (blocker or delta); one key per event keeps it idempotent. */
export function notice(db: Database, sender: string, target: string, kind: string, slots: JsonObject, key: string, wake: string, at: number): void {
  const message = db.run('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,wake,ttlMs,replyRequired,kind,slots) VALUES(?,?,?,?,?,?,?,?,0,?,?)', [sender, target, render(kind, slots), key, at + HOUR_MS, 'Runtime notice', wake, HOUR_MS, kind, dumps(slots, { ensureAscii: false })]).lastInsertRowid;
  execute(db, 'INSERT INTO deliveries(message,recipient) VALUES(?,?)', [message, target]);
}

/** A lease request resolved to a physical path, its caseless key and its kind. */
class Target {
  readonly path: string;
  readonly key: string;

  constructor(path: string, readonly kind: LeaseKind) {
    this.path = path;
    this.key = leaseKey(path);
  }

  keyArgs(): SqlValue[] {
    return [this.key, dumps(ancestors(this.key)), this.kind, this.key + '/', this.key + '0'];
  }
}

function integer(value: unknown): number {
  const text = String(value).trim();
  if (!/^[+-]?\d+$/.test(text)) throw new ValidationError(`invalid literal for int() with base 10: '${String(value)}'`);
  return Number(text);
}

function relative(host: StoreHost, path: string, workspace?: string): string {
  return relativePath(path, workspace ?? host.workspace);
}

/** JSON array of this repository scope's workspaces, or of `workspace`'s own scope. */
function scopeWorkspacesJson(host: StoreHost, workspace?: string): string {
  if (workspace === undefined) return dumps(scopeWorkspaces(host.db, host.coordinationScope, host.workspace));
  const scope = query(host.db, 'SELECT coordinationScope FROM workspaces WHERE workspace=?', [workspace]);
  return dumps(scope.length ? scopeWorkspaces(host.db, rowText(scope[0]!, 'coordinationScope'), workspace) : [workspace]);
}

/** A live lease, or the queued wait that is next in line for a path, as the owner of a conflict. */
interface Blocker {
  id?: number;
  waitId?: number;
  path: string;
  kind: LeaseKind;
  owner: string;
  acquiredAt: number;
  refreshedAt: number;
  expiresAt: number;
  reasoning: string;
  ownerName: string;
  ownerVendor: string;
  ownerExpiresAt: number;
}

/** The lease paths a wait queues, as stored in `lease_waits.targets`. */
function waitTargets(text: string): Target[] {
  return parseObjects(text, 'queued targets').map(item => {
    if (typeof item.path !== 'string' || typeof item.kind !== 'string') throw new TypeError('Queued target needs a path and a kind');
    return new Target(item.path, leaseKind(item.kind));
  });
}

function blocker(row: Row): Blocker {
  return {
    id: rowInteger(row, 'id'), path: rowText(row, 'path'), kind: leaseKind(rowText(row, 'kind')), owner: rowText(row, 'owner'), acquiredAt: rowInteger(row, 'acquiredAt'),
    refreshedAt: rowInteger(row, 'refreshedAt'), expiresAt: rowInteger(row, 'expiresAt'), reasoning: rowText(row, 'reasoning'), ownerName: rowText(row, 'ownerName'),
    ownerVendor: rowText(row, 'ownerVendor'), ownerExpiresAt: rowInteger(row, 'ownerExpiresAt'),
  };
}

function leaseTarget(host: StoreHost, request: { path: string; kind?: 'file' | 'tree' }): Target {
  const path = resolvePath(host.workspace, catalog.text(request, 'path'));
  if (!isWithin(path, host.workspace)) throw new ValidationError('Path escapes workspace');
  return new Target(path, request.kind ?? (isDirectory(path) ? 'tree' : 'file'));
}

export function locks(host: StoreHost, session: string, input: LocksInput): CommandResults['locks'] {
  host.known(session, false);
  // A path query is a conflict check, so it spans the repository scope.
  const workspaces = 'path' in input ? scopeWorkspacesJson(host) : dumps([host.workspace]);
  const args: SqlValue[] = [workspaces, now(), integer(input.after ?? '0')];
  const conditions = ['l.workspace IN (SELECT value FROM json_each(?1))', 'l.id>?3'];
  const active = input.presence ?? 'active';
  if (active !== 'all') {
    conditions.push('(l.expiresAt>?2 AND s.expiresAt>?2)=?');
    args.push(active === 'active');
  }
  if (input.owner !== undefined) {
    conditions.push('l.owner=?');
    args.push(input.owner);
  }
  if ('kind' in input && !('path' in input)) throw new ValidationError('kind requires a path conflict query');
  if (input.path !== undefined) {
    const target = leaseTarget(host, { path: input.path, kind: input.kind });
    conditions.push("(l.pathKey=? OR (l.kind='tree' AND l.pathKey IN (SELECT value FROM json_each(?))) OR (?='tree' AND l.pathKey>? AND l.pathKey<?))");
    args.push(...target.keyArgs());
  }
  const rows = query(host.db, 'SELECT l.*,l.expiresAt>?2 AND s.expiresAt>?2 AS active FROM leases l JOIN sessions s ON s.id=l.owner WHERE ' + conditions.join(' AND ') + ' ORDER BY l.id LIMIT 101', args);
  const items = rows.map(row => ({
    id: rowInteger(row, 'id'), workspace: rowText(row, 'workspace'), path: rowText(row, 'path'), kind: leaseKind(rowText(row, 'kind')), owner: rowText(row, 'owner'),
    acquiredAt: rowInteger(row, 'acquiredAt'), refreshedAt: rowInteger(row, 'refreshedAt'), expiresAt: rowInteger(row, 'expiresAt'), reasoning: rowText(row, 'reasoning'),
    active: rowInteger(row, 'active') === 1 ? (1 as const) : (0 as const),
  }));
  return catalog.page(items, true, 'locks', input);
}

/** Live leases of other owners that overlap `target`. Lease keys are absolute, so every workspace bound to this repository
 * (nested directories and linked worktrees) shares one conflict space. */
function overlapping(host: StoreHost, target: Target, at: number, excludeOwner: string, limit: number, after = 0, workspaces?: string): Blocker[] {
  return query(host.db, OVERLAPS, [workspaces ?? scopeWorkspacesJson(host), ...target.keyArgs(), at, excludeOwner, limit, after]).map(blocker);
}

export function checkPaths(host: StoreHost, session: string, input: CheckPathsInput): CommandResults['check_paths'] {
  const targets = input.paths.map(request => leaseTarget(host, request));
  return readTransaction(host.db, () => {
    host.known(session);
    const at = now();
    let candidates = new Map<number, Blocker>();
    let blocked = false;
    const after = input.after ?? 0;
    for (const target of targets) {
      const rows = overlapping(host, target, at, session, 101, after);
      blocked = blocked || rows.length > 0;
      if (after && !blocked) blocked = overlapping(host, target, at, session, 1).length > 0;
      for (const row of rows) candidates.set(row.id!, row);
      // Each target's first 101 rows contain the union's first 101.
      candidates = new Map([...candidates].sort(([a], [b]) => a - b).slice(0, 101));
    }
    const conflicts: (Lease & { expiresAt: number })[] = [...candidates.values()].map(lease => ({
      id: lease.id!, path: relative(host, lease.path), kind: lease.kind, owner: lease.owner, acquiredAt: lease.acquiredAt, refreshedAt: lease.refreshedAt, expiresAt: lease.expiresAt, reasoning: lease.reasoning,
    }));
    const metadataBytes = Buffer.byteLength(compact(input)) + 100;
    const { items, next, budget } = catalog.page(conflicts, false, 'check_paths', input, metadataBytes);
    const result = { ok: !blocked, conflicts: items, checkedAt: at, ...(next === undefined ? {} : { next }) };
    const oversized = Buffer.byteLength(compact({ ...result, ...(budget === undefined ? {} : { budget }) })) > catalog.PAGE_BYTES;
    return oversized ? { ...result, budget: { targetBytes: catalog.PAGE_BYTES, reason: 'One conflict and its full continuation query exceed the page target; returned intact to preserve evidence and cursor progress' } } : result;
  });
}

export function lock(host: StoreHost, session: string, input: LockInput): CommandResults['lock'] {
  const granted = acquire(host, session, input, [input]);
  return granted.ok ? { ok: true, lease: { ...granted.leases[0]!, expiresAt: granted.expiresAt } } : granted;
}

export function lockMany(host: StoreHost, session: string, input: LockManyInput): CommandResults['lock_many'] {
  return acquire(host, session, input, input.paths);
}

/** The leases a lock request granted, or why it was refused. */
type Acquired = { ok: true; expiresAt: number; leases: Lease[] } | LockRefused;

function acquire(host: StoreHost, session: string, input: LockInput | LockManyInput, requests: readonly PathTarget[]): Acquired {
  const reasoning = catalog.text(input, 'reasoning');
  const targets: Target[] = [];
  for (const request of requests) {
    const target = leaseTarget(host, request);
    if (targets.some(other => keysOverlap(other.key, other.kind, target.key, target.kind))) throw new ValidationError('Requested paths overlap; use one covering tree lease or distinct paths');
    targets.push(target);
  }
  targets.sort((a, b) => compareCodePoints(a.path, b.path) || compareCodePoints(a.kind, b.kind));
  const duration = catalog.ttl(input, LEASE_TTL_DEFAULT_MS);
  return transaction(host.db, () => {
    host.known(session);
    const at = now();
    // Hand expired or released paths to earlier waiters before a newcomer competes.
    grantWaits(host, at);
    for (const target of targets) {
      const rows = overlapping(host, target, at, '', 1);
      if (rows.length) return lockConflict(host, session, input, targets, rows[0]!, reasoning, duration, at);
    }
    // A queued wait is next in line for its paths: a newcomer queues behind it.
    const reserved = reservingWaits(host, targets, scopeWorkspacesJson(host), at, session);
    if (reserved.length) return lockConflict(host, session, input, targets, reserved[0]!, reasoning, duration, at);
    const leases = insertLeases(host, host.workspace, session, targets, reasoning, at, at + duration);
    dropSatisfiedWait(host, session, targets);
    return { ok: true, expiresAt: at + duration, leases };
  });
}

/** A direct grant of a queued path ends that wait; unrelated locks keep it. */
function dropSatisfiedWait(host: StoreHost, session: string, acquired: readonly Target[]): void {
  for (const wait of query(host.db, 'SELECT id,targets FROM lease_waits WHERE owner=?', [session])) {
    if (waitTargets(rowText(wait, 'targets')).some(t => acquired.some(a => keysOverlap(t.key, t.kind, a.key, a.kind)))) execute(host.db, 'DELETE FROM lease_waits WHERE id=?', [rowInteger(wait, 'id')]);
  }
}

function insertLeases(host: StoreHost, workspace: string, owner: string, targets: readonly Target[], reasoning: string, at: number, expires: number): Lease[] {
  const leases: Lease[] = [];
  for (const target of targets) {
    const lease = host.db.run('INSERT INTO leases(workspace,path,kind,owner,acquiredAt,refreshedAt,expiresAt,reasoning,pathKey) VALUES(?,?,?,?,?,?,?,?,?)', [workspace, target.path, target.kind, owner, at, at, expires, reasoning, target.key]).lastInsertRowid;
    leases.push({ id: lease, path: relative(host, target.path, workspace), kind: target.kind, owner, reasoning, acquiredAt: at, refreshedAt: at });
  }
  return leases;
}

function lockConflict(host: StoreHost, session: string, input: LockInput | LockManyInput, targets: readonly Target[], row: Blocker, reasoning: string, duration: number, at: number): LockRefused {
  const owner = row.owner;
  const held = query(host.db, 'SELECT id FROM leases WHERE owner=? AND workspace=? AND expiresAt>? ORDER BY id', [session, host.workspace, at]).map(r => rowInteger(r, 'id'));
  const result: LockRefused = {
    ok: false,
    conflict: {
      ...(row.id === undefined ? {} : { id: row.id }), ...(row.waitId === undefined ? {} : { waitId: row.waitId }),
      kind: row.kind, acquiredAt: row.acquiredAt, refreshedAt: row.refreshedAt, expiresAt: row.expiresAt, reasoning: row.reasoning, path: relative(host, row.path),
    },
    owner: { id: owner, name: row.ownerName, vendor: row.ownerVendor, expiresAt: row.ownerExpiresAt },
    retryAfterMs: Math.max(0, Math.min(row.expiresAt, row.ownerExpiresAt) - at),
    heldLeaseIds: held,
  };
  if (owner === session) return { ...result, guidance: 'You hold an overlapping lease: reuse it, or unlock it and lock the complete set.' };
  if (!input.wait) {
    return {
      ...result, next: { command: 'paths' in input ? 'lock_many' : 'lock', input: { ...input, wait: true } },
      guidance: 'Nothing acquired. Run next to queue: the runtime grants the lease when it frees and wakes you with a message. Keep the request that needs it pending.',
    };
  }
  const queued = dumps(targets.map(t => ({ path: t.path, kind: t.kind })));
  const same = query(host.db, 'SELECT id,expiresAt FROM lease_waits WHERE owner=? AND targets=? AND reasoning=? AND expiresAt>?', [session, queued, reasoning, at]);
  if (same.length) {
    // A retried wait keeps its place and notifies nobody twice.
    return { ...result, queued: true, wait: { id: rowInteger(same[0]!, 'id'), expiresAt: rowInteger(same[0]!, 'expiresAt') }, guidance: QUEUED };
  }
  const workspaces = scopeWorkspacesJson(host);
  const holders = blockingOwners(host, targets, workspaces, at, session);
  const blockers = new Set([...holders, ...reservingWaits(host, targets, workspaces, at, session).map(wait => wait.owner)]);
  if (waitsOn(host, blockers, session, at)) {
    return { ...result, guidance: 'Not queued: waiting would deadlock, because an owner you wait for is waiting for a lease you hold. Release your held leases, then queue.' };
  }
  execute(host.db, 'DELETE FROM lease_waits WHERE owner=?', [session]);
  const wait = host.db.run('INSERT INTO lease_waits(workspace,owner,targets,reasoning,leaseTtlMs,createdAt,expiresAt) VALUES(?,?,?,?,?,?,?)', [host.workspace, session, queued, reasoning, duration, at, at + WAIT_TTL]).lastInsertRowid;
  const paths = targets.map(t => relative(host, t.path)).join(', ');
  const name = rowText(query(host.db, 'SELECT name FROM sessions WHERE id=?', [session])[0]!, 'name');
  for (const holder of [...holders].sort(compareCodePoints)) {
    notice(host.db, session, holder, 'blocker', { situation: `${name} is queued for ${paths}`, background: reasoning, assessment: 'Your lease blocks it', recommendation: 'Unlock when done; the runtime hands the lease over. No reply needed' }, `lease-wait:${wait}:${holder}`, 'passive', at);
  }
  return { ...result, queued: true, wait: { id: wait, expiresAt: at + WAIT_TTL }, guidance: QUEUED };
}

/**
 * A session renewed after it expired keeps its unexpired leases except those another owner took over meanwhile
 * (an expired owner's leases are inactive, so a rival could lock the same path). Returns how many it dropped.
 */
export function forgoTakenLeases(host: StoreHost, session: string, at: number): number {
  let dropped = 0;
  for (const lease of query(host.db, 'SELECT id,path,kind,workspace FROM leases WHERE owner=? AND expiresAt>?', [session, at])) {
    if (!overlapping(host, new Target(rowText(lease, 'path'), leaseKind(rowText(lease, 'kind'))), at, session, 1, 0, scopeWorkspacesJson(host, rowText(lease, 'workspace'))).length) continue;
    execute(host.db, 'DELETE FROM leases WHERE id=?', [rowInteger(lease, 'id')]);
    dropped += 1;
  }
  return dropped;
}

/** Owners of live leases that overlap targets. */
function blockingOwners(host: StoreHost, targets: readonly Target[], workspaces: string, at: number, exclude: string): Set<string> {
  return new Set(targets.flatMap(target => overlapping(host, target, at, exclude, 100, 0, workspaces).map(row => row.owner)));
}

/**
 * Live waits of other owners, older than `before` (all when omitted), whose paths overlap targets.
 * A wait holds no lease but is next in line for its paths, so nothing may jump ahead of it.
 */
function reservingWaits(host: StoreHost, targets: readonly Target[], workspaces: string, at: number, exclude: string, before?: number): Blocker[] {
  const scope = new Set<string>(JSON.parse(workspaces));
  const ahead: Blocker[] = [];
  for (const wait of query(host.db, 'SELECT * FROM lease_waits WHERE owner<>? AND expiresAt>? AND id<? ORDER BY id', [exclude, at, before || NO_BOUND])) {
    if (!scope.has(rowText(wait, 'workspace'))) continue;
    for (const theirs of waitTargets(rowText(wait, 'targets'))) {
      if (targets.some(mine => keysOverlap(theirs.key, theirs.kind, mine.key, mine.kind))) {
        const owner = query(host.db, 'SELECT name,vendor,expiresAt FROM sessions WHERE id=?', [rowText(wait, 'owner')])[0]!;
        ahead.push({
          waitId: rowInteger(wait, 'id'), path: theirs.path, kind: theirs.kind, owner: rowText(wait, 'owner'), acquiredAt: rowInteger(wait, 'createdAt'), refreshedAt: rowInteger(wait, 'createdAt'),
          expiresAt: rowInteger(wait, 'expiresAt'), reasoning: rowText(wait, 'reasoning'), ownerName: rowText(owner, 'name'), ownerVendor: rowText(owner, 'vendor'), ownerExpiresAt: rowInteger(owner, 'expiresAt'),
        });
        break;
      }
    }
  }
  return ahead;
}

/** True when some owner transitively waits for a lease that session holds. */
function waitsOn(host: StoreHost, owners: Iterable<string>, session: string, at: number): boolean {
  const frontier = [...owners];
  const seen = new Set<string>();
  while (frontier.length) {
    const owner = frontier.pop()!;
    if (owner === session) return true;
    if (seen.has(owner)) continue;
    seen.add(owner);
    for (const wait of query(host.db, 'SELECT id,workspace,targets FROM lease_waits WHERE owner=? AND expiresAt>?', [owner, at])) {
      const targets = waitTargets(rowText(wait, 'targets'));
      const workspaces = scopeWorkspacesJson(host, rowText(wait, 'workspace'));
      frontier.push(...blockingOwners(host, targets, workspaces, at, owner));
      frontier.push(...reservingWaits(host, targets, workspaces, at, owner, rowInteger(wait, 'id')).map(row => row.owner));
    }
  }
  return false;
}

/** Grants each live wait, oldest first, whose every target is free now and not reserved by an older wait; wakes its owner. */
export function grantWaits(host: StoreHost, at: number): void {
  execute(host.db, 'DELETE FROM lease_waits WHERE expiresAt<=? OR owner IN (SELECT id FROM sessions WHERE expiresAt<=?)', [at, at]);
  for (const wait of query(host.db, 'SELECT * FROM lease_waits ORDER BY id')) {
    const targets = waitTargets(rowText(wait, 'targets'));
    const [id, owner, workspace] = [rowInteger(wait, 'id'), rowText(wait, 'owner'), rowText(wait, 'workspace')];
    const workspaces = scopeWorkspacesJson(host, workspace);
    if (blockingOwners(host, targets, workspaces, at, owner).size || reservingWaits(host, targets, workspaces, at, owner, id).length) continue;
    execute(host.db, 'DELETE FROM lease_waits WHERE id=?', [id]);
    const ttl = rowInteger(wait, 'leaseTtlMs');
    const leases = insertLeases(host, workspace, owner, targets, rowText(wait, 'reasoning'), at, at + ttl);
    const granted = leases.map(lease => `${lease.path} (lease ${lease.id})`).join(', ');
    notice(host.db, owner, owner, 'delta', { changes: [`Lease granted: ${granted}, expires in ${Math.floor(ttl / 1000)}s`], next: 'Continue the work you queued it for; unlock when done' }, `lease-wait:${id}`, 'action', at);
  }
}

export function renew(host: StoreHost, session: string, input: RenewInput): CommandResults['renew'] {
  return transaction(host.db, () => {
    host.known(session);
    const at = now();
    const expires = at + catalog.ttl(input, LEASE_TTL_DEFAULT_MS);
    const count = execute(host.db, 'UPDATE leases SET expiresAt=?,refreshedAt=? WHERE id=? AND owner=? AND expiresAt>?', [expires, at, input.leaseId, session, at]);
    return count === 1 ? { renewed: true, refreshedAt: at, expiresAt: expires } : { renewed: false, guidance: 'No live owned lease covers this ID. Stop writing; acquire a fresh lock before resuming.' };
  });
}

export function unlock(host: StoreHost, session: string, input: UnlockInput): CommandResults['unlock'] {
  return transaction(host.db, () => {
    host.known(session);
    const at = now();
    const released = execute(host.db, 'DELETE FROM leases WHERE id=? AND owner=? AND expiresAt>?', [input.leaseId, session, at]) === 1;
    grantWaits(host, at);
    return { released };
  });
}

export function checkWrite(host: StoreHost, session: string, input: CheckWriteInput): CommandResults['check_write'] {
  const targets: string[] = [];
  for (const request of input.paths) {
    const target = leaseTarget(host, request);
    if (target.kind !== 'file' || isDirectory(target.path)) throw new ValidationError('check_write accepts file targets only, not directories');
    targets.push(target.path);
  }
  return readTransaction(host.db, () => {
    const identity = host.known(session);
    if ('vendorSession' in input) {
      catalog.text(input, 'vendorSession');
      if (identity.vendorSession !== input.vendorSession) throw new ValidationError('Native session does not match the bound DB identity');
    }
    const at = now();
    const presence = identity.expiresAt;
    if (presence <= at) throw new ValidationError('Session expired before checking file coverage');
    const checks: CommandResults['check_write']['checks'] = [];
    for (const path of targets) {
      const rows = query(host.db, "SELECT id,expiresAt FROM leases WHERE owner=? AND workspace=? AND expiresAt>? AND lease_overlap(path,kind,?,'file') ORDER BY id LIMIT 1", [session, host.workspace, at, path]);
      checks.push({ path, covered: rows.length > 0, ...(rows.length ? { lease: { id: rowInteger(rows[0]!, 'id'), expiresAt: Math.min(rowInteger(rows[0]!, 'expiresAt'), presence) } } : {}) });
    }
    return { ok: checks.every(check => check.covered), checkedAt: at, advisory: true, checks };
  });
}
