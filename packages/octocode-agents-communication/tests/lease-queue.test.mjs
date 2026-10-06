import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, jsonCall, tempWorkspace, withReasoning } from './helpers.mjs';

function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-queue-', { real: true }), database = join(workspace, 'queue.sqlite');
  for (const file of ['a.ts', 'b.ts']) writeFileSync(join(workspace, file), '');
  const raw = jsonCall(binary, workspace, database);
  const call = (command, input, session) => raw(command, withReasoning(command, input), session);
  const join_ = name => call('join', { name, vendor: 'generic' }).id;
  const db = new DatabaseSync(database); t.after(() => db.close());
  const holder = path => db.prepare("SELECT s.name FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.path LIKE ? AND l.expiresAt>?").get('%/' + path, Date.now())?.name;
  const mail = session => call('inbox', {}, session).items.map(item => item.body);
  return { call, join: join_, db, holder, mail };
}

test('a conflict suggests queueing; a queued waiter is granted on unlock and woken', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b');
  const lease = f.call('lock', { path: 'a.ts' }, a).lease;
  const conflict = f.call('lock', { path: 'a.ts' }, b);
  assert.equal(conflict.ok, false);
  assert.deepEqual(conflict.next, { command: 'lock', input: { path: 'a.ts', reasoning: conflict.next.input.reasoning, wait: true } });
  const queued = f.call(conflict.next.command, conflict.next.input, b);
  assert.equal(queued.queued, true);
  assert.match(f.mail(a)[0], /b is queued for a\.ts/);
  assert.equal(f.holder('a.ts'), 'a');
  assert.equal(f.call('unlock', { leaseId: lease.id }, a).released, true);
  assert.equal(f.holder('a.ts'), 'b');
  const [granted] = f.call('inbox', {}, b).items;
  assert.match(granted.body, /^Lease granted: a\.ts \(lease \d+\)/);
  assert.equal(granted.wake, 'action');
  assert.equal(granted.replyRequired, false);
  assert.equal(f.db.prepare('SELECT count(*) n FROM lease_waits').get().n, 0);
});

test('waiters on one path are served oldest first', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b'), c = f.join('c');
  const first = f.call('lock', { path: 'a.ts' }, a).lease;
  f.call('lock', { path: 'a.ts', wait: true }, b);
  f.call('lock', { path: 'a.ts', wait: true }, c);
  f.call('unlock', { leaseId: first.id }, a);
  assert.equal(f.holder('a.ts'), 'b');
  const second = f.db.prepare("SELECT l.id FROM leases l JOIN sessions s ON s.id=l.owner WHERE s.name='b'").get().id;
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
  assert.equal(f.db.prepare('SELECT count(*) n FROM lease_waits WHERE owner=?').get(b).n, 0);
});

test('a set wait holds nothing and is granted only when every path is free', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b'), c = f.join('c');
  const held = f.call('lock', { path: 'a.ts' }, a).lease;
  assert.equal(f.call('lock_many', { paths: [{ path: 'a.ts' }, { path: 'b.ts' }], wait: true }, b).queued, true);
  assert.equal(f.holder('b.ts'), undefined, 'waiting reserves nothing');
  const other = f.call('lock', { path: 'b.ts' }, c).lease;
  f.call('unlock', { leaseId: held.id }, a);
  assert.equal(f.holder('a.ts'), undefined, 'no partial grant while b.ts is held');
  f.call('unlock', { leaseId: other.id }, c);
  assert.deepEqual([f.holder('a.ts'), f.holder('b.ts')], ['b', 'b']);
});

test('leave, a direct lock and own overlapping leases end or refuse waits', t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b');
  const held = f.call('lock', { path: 'a.ts' }, a).lease;
  f.call('lock', { path: 'a.ts', wait: true }, b);
  f.call('leave', {}, b);
  assert.equal(f.db.prepare('SELECT count(*) n FROM lease_waits').get().n, 0);
  const c = f.join('c');
  f.call('lock', { path: 'a.ts', wait: true }, c);
  f.call('lock', { path: 'b.ts' }, c);
  assert.equal(f.db.prepare('SELECT count(*) n FROM lease_waits').get().n, 1, 'an unrelated lock keeps the wait');
  assert.match(f.call('lock', { path: 'a.ts', wait: true }, a).guidance, /You hold an overlapping lease/);
  f.call('unlock', { leaseId: held.id }, a);
});
