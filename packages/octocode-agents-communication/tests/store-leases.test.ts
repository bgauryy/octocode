// Leases of the store over real temporary databases: conflicts, the wait queue, deadlock refusal, atomic lock_many,
// coverage checks and release on leave.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { JsonRecord } from './json-record.ts';
import { refuses, trio } from './store-support.ts';

// --- Leases --------------------------------------------------------------------------------------------------

test('Given overlapping paths, when leases are requested, then conflicts name the owner and the queue grants in order on unlock', async t => {
  const f = await trio(t);
  f.mkdir('src');
  const held = await f.call(f.alice, 'lock', { path: 'src', kind: 'tree' });
  assert.equal(held.ok, true);
  const refused = await f.call(f.bob, 'lock', { path: 'src/a.ts' });
  assert.equal(refused.ok, false);
  assert.equal(refused.conflict.path, 'src');
  assert.equal(refused.owner.id, f.alice);
  assert.deepEqual(refused.next, { command: 'lock', input: { path: 'src/a.ts', reasoning: refused.next.input.reasoning, wait: true } });
  const queued = await f.call(f.bob, 'lock', { ...refused.next.input });
  assert.equal(queued.queued, true);
  assert.equal((await f.call(f.bob, 'lock', { ...refused.next.input })).wait.id, queued.wait.id, 'a retried wait keeps its place');
  assert.match((await f.inbox(f.alice))[0].body, /^SBAR\nSituation: bob is queued for src\/a\.ts/);
  // carol queues behind bob for the same path and may not jump ahead of it
  const behind = await f.call(f.carol, 'lock', { path: 'src/a.ts', wait: true });
  assert.equal(behind.queued, true);
  assert.deepEqual(await f.call(f.alice, 'unlock', { leaseId: held.lease.id }), { released: true });
  const granted = (await f.inbox(f.bob)).find(item => item.kind === 'delta');
  assert.ok(granted);
  assert.match(granted.body, /Lease granted: src\/a\.ts \(lease \d+\)/);
  const locks = await f.call(f.bob, 'locks', { owner: f.bob });
  assert.equal(locks.items.length, 1);
  assert.equal(locks.items[0].active, 1);
  assert.equal((await f.call(f.carol, 'lock', { path: 'src/a.ts' })).ok, false, 'carol waits for bob, who now holds the path');
});

test('Given two owners waiting on each other, when the second would deadlock, then it is not queued', async t => {
  const f = await trio(t);
  const a = await f.call(f.alice, 'lock', { path: 'a.txt' });
  const b = await f.call(f.bob, 'lock', { path: 'b.txt' });
  assert.equal((await f.call(f.alice, 'lock', { path: 'b.txt', wait: true })).queued, true);
  const deadlock = await f.call(f.bob, 'lock', { path: 'a.txt', wait: true });
  assert.equal(deadlock.ok, false);
  assert.equal(deadlock.queued, undefined);
  assert.match(deadlock.guidance, /would deadlock/);
  assert.ok(a.lease.id && b.lease.id);
});

test('Given lock_many, when two requested paths overlap or any is held, then nothing is acquired', async t => {
  const f = await trio(t);
  f.mkdir('d');
  await refuses(f.call(f.alice, 'lock_many', { paths: [{ path: 'd', kind: 'tree' }, { path: 'd/x' }] }), /overlap/);
  await f.call(f.bob, 'lock', { path: 'b.txt' });
  const blocked = await f.call(f.alice, 'lock_many', { paths: [{ path: 'a.txt' }, { path: 'b.txt' }] });
  assert.equal(blocked.ok, false);
  assert.equal((await f.call(f.alice, 'locks', { owner: f.alice })).items.length, 0);
  const both = await f.call(f.alice, 'lock_many', { paths: [{ path: 'a.txt' }, { path: 'c.txt' }] });
  assert.deepEqual(both.leases.map((lease: JsonRecord) => lease.path), ['a.txt', 'c.txt']);
  await refuses(f.call(f.alice, 'lock', { path: '../outside' }), /escapes workspace/);
});

test('Given leases, when paths and writes are checked, then coverage, conflicts and expiry are reported', async t => {
  const f = await trio(t);
  f.mkdir('src');
  const held = await f.call(f.alice, 'lock', { path: 'src', kind: 'tree' });
  const write = await f.call(f.alice, 'check_write', { paths: [{ path: 'src/a.ts' }, { path: 'other.ts' }] });
  assert.equal(write.ok, false);
  assert.deepEqual(write.checks.map((check: JsonRecord) => check.covered), [true, false]);
  assert.equal(write.checks[0].lease.id, held.lease.id);
  await refuses(f.call(f.alice, 'check_write', { paths: [{ path: 'src' }] }), /file targets only/);
  const conflicts = await f.call(f.bob, 'check_paths', { paths: [{ path: 'src/a.ts' }] });
  assert.equal(conflicts.ok, false);
  assert.equal(conflicts.conflicts[0].path, 'src');
  assert.equal((await f.call(f.alice, 'check_paths', { paths: [{ path: 'src/a.ts' }] })).ok, true, 'own leases never conflict');
  assert.deepEqual(await f.call(f.alice, 'renew', { leaseId: held.lease.id, ttlMs: 120000 }).then(r => r.renewed), true);
  assert.equal((await f.call(f.alice, 'renew', { leaseId: 999999 })).renewed, false);
  assert.deepEqual(await f.call(f.alice, 'prune'), { removed: 0 });
});

test('Given a session that left, when its leases are listed, then they are gone and waiters are granted', async t => {
  const f = await trio(t);
  await f.call(f.alice, 'lock', { path: 'x.txt' });
  assert.equal((await f.call(f.bob, 'lock', { path: 'x.txt', wait: true })).queued, true);
  await f.call(f.alice, 'leave');
  assert.equal((await f.call(f.bob, 'locks', { owner: f.bob })).items.length, 1);
  const notice = (await f.inbox(f.bob)).find(item => item.kind === 'delta');
  assert.ok(notice);
  assert.match(notice.body, /Lease granted: x\.txt/);
});
