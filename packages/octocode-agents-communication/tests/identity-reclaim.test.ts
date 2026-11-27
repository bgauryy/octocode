// A worker that slept or stalled past the presence TTL keeps its identity while its process holds the delivery-owner lock.
// Only `Date` is mocked: the monotonic clock stands still, as it does across a Linux suspend.
import assert from 'node:assert/strict';
import test from 'node:test';
import type { TestContext } from 'node:test';
import { waitFor } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';
import { only } from './helpers.ts';
import { fixture, runtime } from './store-support.ts';

const { ownerLock, dispatch, timing } = runtime;
const MINUTE = 60000;
type Fixture = ReturnType<typeof fixture>;

/** A fixture with a frozen wall clock; `jump(ms)` moves it forward. */
function clocked(t: TestContext) {
  const start = Date.now();
  t.mock.timers.enable({ apis: ['Date'], now: start });
  const f = fixture(t);
  return { ...f, jump: (ms: number) => t.mock.timers.tick(ms) };
}

const lockPath = (f: Fixture, session: string, path: string, extra: Record<string, unknown> = {}) => f.call(session, 'lock', { path, reasoning: 'Hold the path while the worker sleeps', ttlMs: 10 * MINUTE, ...extra });
const ownedLeases = (f: Fixture, session: string): string[] => f.store.db.raw.prepare('SELECT path FROM leases WHERE owner=?').all(session).map((row: JsonRecord) => row.path);
const sessionRow = (f: Fixture, session: string): JsonRecord => only(f.store.db.raw.prepare('SELECT name,expiresAt FROM sessions WHERE id=?').get(session));

test('with the owner lock, an expired session renews, keeps its leases and stage never throws', async t => {
  const f = clocked(t);
  const worker = await f.join('worker');
  const sender = await f.join('sender');
  await lockPath(f, worker, 'held.txt');
  const sent = await f.call(sender, 'send_message', {parts: [{ text: 'status?' }], replyRequired: true,  to: worker, key: 'q1' });
  await using lock = await ownerLock.claimDeliveryOwner(f.file, worker);
  f.jump(5 * MINUTE);
  assert.ok(sessionRow(f, worker).expiresAt <= Date.now(), 'the presence row expired');

  const idle = f.store.stage(worker, 'managed:test', true);
  assert.deepEqual([...idle], []);
  assert.equal(idle.reason, dispatch.SESSION_EXPIRED);
  assert.equal(f.store.hasDispatchable(worker, true), false);

  const beat = await f.call(worker, 'heartbeat', { renewLeases: true });
  assert.equal(beat.alive, true);
  assert.ok(beat.expiresAt > Date.now());
  assert.equal(ownedLeases(f, worker).length, 1, 'the lease survives');
  const listed = await f.call(worker, 'locks', { owner: worker });
  assert.ok(JSON.stringify(listed).includes('held.txt'), 'the lease is active again');
  const items = f.store.stage(worker, 'managed:test', true);
  assert.equal(items.reason, undefined);
  assert.deepEqual(items.map(item => item.id), [sent.id]);
  assert.ok(lock);
});

test('without the owner lock an expired session still fails its heartbeat and stage; only resume recovers', async t => {
  const f = clocked(t);
  const worker = await f.join('worker');
  await lockPath(f, worker, 'held.txt');
  f.jump(5 * MINUTE);
  await assert.rejects(f.call(worker, 'heartbeat', {}), /Unknown or expired session/);
  assert.throws(() => f.store.stage(worker, 'managed:test', true), /Unknown or expired session/);
  assert.throws(() => f.store.hasDispatchable(worker, true), /Unknown or expired session/);
  const resumed = await f.call(worker, 'resume', { vendor: 'generic' });
  assert.equal(resumed.id, worker);
  assert.deepEqual(ownedLeases(f, worker), [], 'resume drops the leases');
});

test('a lock held by another session does not renew this one', async t => {
  const f = clocked(t);
  const worker = await f.join('worker');
  const other = await f.join('other');
  await using lock = await ownerLock.claimDeliveryOwner(f.file, other);
  f.jump(5 * MINUTE);
  await assert.rejects(f.call(worker, 'heartbeat', {}), /Unknown or expired session/);
  assert.ok(lock);
});

test('a name taken by a newer join falls back to resume: a free name, no leases', async t => {
  const f = clocked(t);
  const worker = await f.join('alpha');
  await lockPath(f, worker, 'held.txt');
  await using lock = await ownerLock.claimDeliveryOwner(f.file, worker);
  f.jump(5 * MINUTE);
  const newer = await f.call('', 'join', { name: 'alpha', vendor: 'generic' });
  assert.equal(newer.name, 'alpha', 'the expired name was free');
  const beat = await f.call(worker, 'heartbeat', { renewLeases: true });
  assert.equal(beat.alive, true);
  assert.equal(sessionRow(f, worker).name, 'alpha-2');
  assert.equal(sessionRow(f, newer.id).name, 'alpha');
  assert.deepEqual(ownedLeases(f, worker), []);
  assert.ok(lock);
});

test('a lease another owner took while the session was expired is forgone; the rest stay', async t => {
  const f = clocked(t);
  const worker = await f.join('worker');
  await lockPath(f, worker, 'taken.txt');
  await lockPath(f, worker, 'kept.txt');
  await using lock = await ownerLock.claimDeliveryOwner(f.file, worker);
  f.jump(5 * MINUTE);
  const rival = await f.join('rival');
  assert.equal((await lockPath(f, rival, 'taken.txt')).ok, true, 'an expired owner holds nothing');
  await f.call(worker, 'heartbeat', { renewLeases: true });
  assert.deepEqual(ownedLeases(f, worker).map(path => path.split('/').at(-1)), ['kept.txt']);
  assert.equal(ownedLeases(f, rival).length, 1);
  assert.ok(lock);
});

test('a heartbeat is due when the wall clock advanced a period even though the monotonic clock did not', t => {
  t.mock.timers.enable({ apis: ['Date'], now: Date.now() });
  let beats = 0;
  const heartbeat = new timing.Heartbeat(() => { beats += 1; });
  assert.equal(heartbeat.tick(), false);
  t.mock.timers.tick(heartbeat.interval - 1);
  assert.equal(heartbeat.tick(), false);
  t.mock.timers.tick(1);
  assert.equal(heartbeat.tick(), true);
  assert.equal(beats, 1);
  assert.equal(heartbeat.tick(), false, 'the beat restarts both clocks');
  assert.equal(new timing.Heartbeat(() => undefined, true).tick(), true);
});

test('an explicit leave is never undone by the owner lock: the heartbeat still fails closed', async t => {
  const f = clocked(t);
  const worker = await f.join('worker');
  await using lock = await ownerLock.claimDeliveryOwner(f.file, worker);
  await f.call(worker, 'leave', {});
  await assert.rejects(f.call(worker, 'heartbeat', {}), /Unknown or expired session/);
  f.jump(5 * MINUTE);
  await assert.rejects(f.call(worker, 'heartbeat', {}), /Unknown or expired session/);
  assert.ok(lock);
});

/** The newest coordinate record type and id of a session: the guard `reclaim` reads. */
const lastCoordinate = (f: Fixture, session: string): JsonRecord => only(f.store.db.raw.prepare("SELECT id,type FROM records WHERE \"from\"=? AND type IN ('coordinate.in','coordinate.update','coordinate.out') ORDER BY id DESC LIMIT 1").get(session));

// The trigger that writes coordinate.* reads SQLite's own clock, so this one expires the row for real instead of mocking `Date`.
test('leaving an already expired session still ends it: the newest coordinate record is out and the heartbeat fails closed', async t => {
  const f = fixture(t);
  const worker = await f.join('worker');
  await using lock = await ownerLock.claimDeliveryOwner(f.file, worker);
  // A future expiry writes coordinate.update, as a stalled owner's presence row would have it; then the real clock expires it.
  f.store.db.raw.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now() + 300, worker);
  await waitFor(() => sessionRow(f, worker).expiresAt <= Date.now(), { message: 'The session did not expire' });
  const expired = sessionRow(f, worker).expiresAt;
  await f.call(worker, 'leave', {});
  assert.equal(lastCoordinate(f, worker).type, 'coordinate.out');
  assert.ok(sessionRow(f, worker).expiresAt <= expired, 'leave never raises an expired row');
  await assert.rejects(f.call(worker, 'heartbeat', {}), /Unknown or expired session/);
  assert.ok(sessionRow(f, worker).expiresAt <= expired, 'the refused heartbeat renewed nothing');
  assert.ok(lock);
});

test('a second leave keeps coordinate.out the newest record', async t => {
  const f = clocked(t);
  const worker = await f.join('worker');
  await using lock = await ownerLock.claimDeliveryOwner(f.file, worker);
  await f.call(worker, 'leave', {});
  f.jump(1);
  await f.call(worker, 'leave', {});
  assert.equal(lastCoordinate(f, worker).type, 'coordinate.out');
  await assert.rejects(f.call(worker, 'heartbeat', {}), /Unknown or expired session/);
  assert.ok(lock);
});

test('every leave appends coordinate.out even within the same millisecond', async t => {
  const f = clocked(t);
  const worker = await f.join('worker');
  await f.call(worker, 'leave', {});
  const first = lastCoordinate(f, worker);
  assert.equal(first.type, 'coordinate.out');
  await f.call(worker, 'leave', {});
  const second = lastCoordinate(f, worker);
  assert.equal(second.type, 'coordinate.out');
  assert.ok(second.id > first.id, 'the repeated leave wrote its own record');
});
