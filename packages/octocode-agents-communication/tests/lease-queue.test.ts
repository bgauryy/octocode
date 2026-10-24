import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { JsonRecord } from './json-record.ts';
import { binary, jsonCall, tempWorkspace, withReasoning, only } from './helpers.ts';

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-queue-', { real: true }), database = join(workspace, 'queue.sqlite');
  for (const file of ['a.ts', 'b.ts']) writeFileSync(join(workspace, file), '');
  const raw = jsonCall(binary, workspace, database);
  const call = (command: string, input: Record<string, unknown> = {}, session?: string) => raw(command, withReasoning(command, input), session);
  const join_ = (name: string) => call('join', { name, vendor: 'generic' }).id;
  const db = new DatabaseSync(database); t.after(() => db.close());
  const holder = (path: string) => db.prepare("SELECT s.name FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.path LIKE ? AND l.expiresAt>?").get('%/' + path, Date.now())?.name;
  const mail = (session: string): string[] => call('inbox', {}, session).items.map((item: JsonRecord) => item.body);
  return { call, join: join_, db, holder, mail, workspace };
}

test('a conflict suggests queueing; a queued waiter is granted on unlock and woken', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b');
  const lease = f.call('lock', { path: 'a.ts' }, a).lease;
  const conflict = f.call('lock', { path: 'a.ts' }, b);
  assert.equal(conflict.ok, false);
  assert.deepEqual(conflict.next, { command: 'lock', input: { path: 'a.ts', reasoning: conflict.next.input.reasoning, wait: true } });
  const queued = f.call(conflict.next.command, conflict.next.input, b);
  assert.equal(queued.queued, true);
  assert.match(f.mail(a)[0], /^SBAR\nSituation: b is queued for a\.ts/);
  assert.equal(f.holder('a.ts'), 'a');
  assert.equal(f.call('unlock', { leaseId: lease.id }, a).released, true);
  assert.equal(f.holder('a.ts'), 'b');
  const [granted] = f.call('inbox', {}, b).items;
  assert.match(granted.body, /^Delta update\nChanges:\n- Lease granted: a\.ts \(lease \d+\)/);
  assert.equal(granted.wake, 'action');
  assert.equal(granted.replyRequired, false);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM lease_waits').get()).n, 0);
});

test('waiters on one path are served oldest first', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b'), c = f.join('c');
  const first = f.call('lock', { path: 'a.ts' }, a).lease;
  f.call('lock', { path: 'a.ts', wait: true }, b);
  f.call('lock', { path: 'a.ts', wait: true }, c);
  f.call('unlock', { leaseId: first.id }, a);
  assert.equal(f.holder('a.ts'), 'b');
  const second = only(f.db.prepare("SELECT l.id FROM leases l JOIN sessions s ON s.id=l.owner WHERE s.name='b'").get()).id;
  f.call('unlock', { leaseId: second }, b);
  assert.equal(f.holder('a.ts'), 'c');
});

test('an expired lease is handed to the waiter by the next heartbeat', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b'), c = f.join('c');
  f.call('lock', { path: 'a.ts' }, a);
  f.call('lock', { path: 'a.ts', wait: true }, b);
  f.db.prepare("UPDATE leases SET expiresAt=? WHERE owner=?").run(Date.now() - 1, a);
  f.call('heartbeat', {}, c);
  assert.equal(f.holder('a.ts'), 'b');
});

test('a wait that would close an ownership cycle is refused, not queued', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b');
  f.call('lock', { path: 'a.ts' }, a); f.call('lock', { path: 'b.ts' }, b);
  assert.equal(f.call('lock', { path: 'b.ts', wait: true }, a).queued, true);
  const refused = f.call('lock', { path: 'a.ts', wait: true }, b);
  assert.equal(refused.queued, undefined);
  assert.match(refused.guidance, /would deadlock/);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM lease_waits WHERE owner=?').get(b)).n, 0);
});

test('a set wait holds no lease and is granted only when every path is free', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b'), c = f.join('c');
  const held = f.call('lock', { path: 'a.ts' }, a).lease;
  const other = f.call('lock', { path: 'b.ts' }, c).lease;
  assert.equal(f.call('lock_many', { paths: [{ path: 'a.ts' }, { path: 'b.ts' }], wait: true }, b).queued, true);
  assert.equal(only(f.db.prepare("SELECT count(*) n FROM leases l JOIN sessions s ON s.id=l.owner WHERE s.name='b'").get()).n, 0, 'waiting holds no lease');
  f.call('unlock', { leaseId: held.id }, a);
  assert.equal(f.holder('a.ts'), undefined, 'no partial grant while b.ts is held');
  f.call('unlock', { leaseId: other.id }, c);
  assert.deepEqual([f.holder('a.ts'), f.holder('b.ts')], ['b', 'b']);
});

test('a queued set wait reserves its paths: a third agent cannot take one as it frees', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob'), carol = f.join('carol');
  const x = f.call('lock', { path: 'a.ts' }, bob).lease;
  assert.equal(f.call('lock_many', { paths: [{ path: 'a.ts' }, { path: 'b.ts' }], wait: true }, alice).queued, true);
  // b.ts is free, but alice is next in line for it.
  const refused = f.call('lock', { path: 'b.ts' }, carol);
  assert.equal(refused.ok, false);
  assert.equal(refused.owner.name, 'alice');
  assert.equal(refused.conflict.path, 'b.ts');
  assert.equal(typeof refused.conflict.waitId, 'number');
  assert.equal(refused.next.input.wait, true);
  assert.equal(f.holder('b.ts'), undefined, 'a reservation is not a lease');
  assert.equal(f.call('lock', { path: 'a.ts' }, carol).ok, false);
  f.call('unlock', { leaseId: x.id }, bob);
  assert.deepEqual([f.holder('a.ts'), f.holder('b.ts')], ['alice', 'alice']);
  const lastMail = f.mail(alice).at(-1);
  assert.ok(lastMail, 'alice has mail');
  assert.match(lastMail, /^Delta update\nChanges:\n- Lease granted: a\.ts \(lease \d+\), b\.ts/);
});

test('the reservation is first-come: later waiters queue behind it and unrelated paths stay free', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob'), carol = f.join('carol');
  writeFileSync(join(f.workspace, 'c.ts'), '');
  const x = f.call('lock', { path: 'a.ts' }, bob).lease;
  f.call('lock_many', { paths: [{ path: 'a.ts' }, { path: 'b.ts' }], wait: true }, alice);
  assert.equal(f.call('lock', { path: 'c.ts' }, carol).ok, true, 'a path no wait covers is unaffected');
  assert.equal(f.call('lock', { path: 'b.ts', wait: true }, carol).queued, true);
  assert.equal(f.mail(alice).some((body: string) => /carol is queued/.test(body)), false, 'a wait ahead of you is not asked to unlock');
  f.call('unlock', { leaseId: x.id }, bob);
  assert.deepEqual([f.holder('a.ts'), f.holder('b.ts')], ['alice', 'alice'], 'the older wait is served first');
  const lease = only(f.db.prepare("SELECT id FROM leases WHERE path LIKE '%/b.ts'").get()).id;
  f.call('unlock', { leaseId: lease }, alice);
  assert.equal(f.holder('b.ts'), 'carol');
});

test('a wait never reserves against its own owner', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob');
  f.call('lock', { path: 'a.ts' }, bob);
  f.call('lock_many', { paths: [{ path: 'a.ts' }, { path: 'b.ts' }], wait: true }, alice);
  assert.equal(f.call('lock', { path: 'b.ts' }, alice).ok, true, 'the waiter may lock a path it also waits for');
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM lease_waits WHERE owner=?').get(alice)).n, 0, 'a direct grant ends the wait');
});

test('waiting on a reservation checks for deadlock through the reserving wait', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob');
  f.call('lock', { path: 'a.ts' }, bob);
  f.call('lock_many', { paths: [{ path: 'a.ts' }, { path: 'b.ts' }], wait: true }, alice);
  // bob holds a.ts, alice waits for it; bob now needs b.ts, which alice's wait reserves.
  const refused = f.call('lock', { path: 'b.ts', wait: true }, bob);
  assert.equal(refused.queued, undefined);
  assert.match(refused.guidance, /would deadlock/);
});

test('leave, a direct lock and own overlapping leases end or refuse waits; an owner that leaves hands its lease to the waiter', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b');
  f.call('lock', { path: 'a.ts' }, a);
  f.call('lock', { path: 'a.ts', wait: true }, b);
  f.call('leave', {}, b);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM lease_waits').get()).n, 0);
  const c = f.join('c');
  f.call('lock', { path: 'a.ts', wait: true }, c);
  f.call('lock', { path: 'b.ts' }, c);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM lease_waits').get()).n, 1, 'an unrelated lock keeps the wait');
  assert.match(f.call('lock', { path: 'a.ts', wait: true }, a).guidance, /You hold an overlapping lease/);
  f.call('leave', {}, a);
  assert.equal(f.holder('a.ts'), 'c', 'the owner leaving grants its lease to the waiter');
  assert.match(f.mail(c).at(-1) ?? '', /^Delta update\nChanges:\n- Lease granted: a\.ts \(lease \d+\)/);
});
