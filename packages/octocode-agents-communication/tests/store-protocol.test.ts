// Behavior of the store (src/runtime/store.ts and its capability modules) over real temporary databases: sessions,
// opaque multipart messages, correlation, wildcard delivery and paging. Reads of previous releases are in database-legacy-compat.test.ts.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { JsonRecord } from './json-record.ts';
import { fixture, refuses, trio } from './store-support.ts';

// --- Sessions ----------------------------------------------------------------------------------------------

test('Given a workspace, when agents join, then names are unique among live peers and identity reads return the binding', async t => {
  const f = fixture(t, { git: true });
  const first = await f.call('', 'join', { name: 'agent', vendor: 'generic', vendorSession: 'native-1' });
  const second = await f.call('', 'join', { name: 'agent', vendor: 'generic' });
  assert.equal(first.name, 'agent');
  assert.equal(second.name, 'agent-2');
  assert.equal(first.vendorSession, 'native-1');
  assert.equal('vendorSession' in second, false, 'null members are stripped');
  const binding = await f.call(first.id, 'binding');
  assert.equal(binding.active, true);
  assert.equal(binding.coordinationScope, f.store.coordinationScope);
  assert.equal(binding.workspace, f.workspace);
  const peers = await f.call(first.id, 'peers');
  assert.deepEqual(peers.items.map((peer: JsonRecord) => peer.name).sort(), ['agent', 'agent-2']);
  assert.deepEqual(peers.items.map((peer: JsonRecord) => peer.id), [first.id, second.id].sort());
});

test('Given a session, when it heartbeats, updates status and leaves, then presence, status and the checkpoint follow', async t => {
  const f = fixture(t);
  const id = await f.join('agent');
  const beat = await f.call(id, 'heartbeat', { ttlMs: 120000, task: 'porting', status: 'busy' });
  assert.equal(beat.alive, true);
  assert.deepEqual(await f.call(id, 'set_status', { status: 'blocked' }), { id, task: 'porting', status: 'blocked' });
  assert.equal((await f.call(id, 'binding')).checkpoint, undefined);
  await f.call(id, 'record', { type: 'checkpoint', data: { goal: 'port', next: 'test' } });
  const latest = await f.call(id, 'record', { type: 'checkpoint', data: { goal: 'port', done: ['docs'], next: 'run tests' } });
  await assert.rejects(f.call(id, 'record', { type: 'checkpoint', data: { goal: 'No next' } }), /next/);
  assert.deepEqual((await f.call(id, 'binding')).checkpoint, { recordId: latest.recordId, timestamp: latest.timestamp, data: latest.data }, 'the latest checkpoint wins');
  assert.deepEqual(await f.call(id, 'leave'), { left: true });
  const binding = await f.call(id, 'binding');
  assert.equal(binding.active, false);
  await refuses(f.call(id, 'heartbeat'), /Unknown or expired session/);
  await refuses(f.call(id, 'resume', { vendor: 'other' }), /Vendor mismatch or session still active/);
  const resumed = await f.call(id, 'resume', { vendor: 'generic' });
  assert.equal(resumed.id, id);
  assert.deepEqual(resumed.checkpoint.data, latest.data, 'resume returns the latest checkpoint');
  assert.ok(resumed.expiresAt > Date.now());
});

test('Given a live peer name, when another session renames to it or an unknown peer is addressed, then both are refused', async t => {
  const f = await trio(t);
  await refuses(f.call(f.alice, 'heartbeat', { name: 'bob' }), /belongs to a live peer/);
  await refuses(f.call(f.alice, 'send_message', { to: 'nobody', parts: [{ text: 'hi' }] }), /Unknown peer nobody/);
  assert.equal((await f.call(f.alice, 'send_message', { to: 'bob', parts: [{ text: 'hi' }] })).recipients, 1);
  await f.call(f.alice, 'heartbeat', { name: 'alice-renamed', branch: 'topic' });
  assert.equal((await f.call(f.alice, 'binding')).name, 'alice-renamed');
});

test('Given a database file, when it is replaced or removed, then the bound store refuses to continue', async t => {
  const f = fixture(t);
  const id = await f.join('agent');
  const { renameSync, writeFileSync, rmSync } = await import('node:fs');
  renameSync(f.file, f.file + '.moved');
  writeFileSync(f.file, '');
  await refuses(f.call(id, 'binding'), /replaced|no longer a file/);
  rmSync(f.file);
  await refuses(f.call(id, 'binding'), /disappeared/);
  renameSync(f.file + '.moved', f.file);
});

test('Given an unknown operation or invalid input, when it is called, then it is refused before touching the database', async t => {
  const f = fixture(t);
  await assert.rejects(f.call('', 'nonsense'), /nonsense|Unknown/);
  await assert.rejects(f.call('', 'join', { name: 'agent' }), /vendor/);
});

// --- Messages ------------------------------------------------------------------------------------------------

test('Given arbitrary message parts, when they are sent, then policy-shaped JSON remains opaque', async t => {
  const f = await trio(t);
  const opaque = { kind: 'task', slots: { informed: ['carol'], checkBack: true, leases: [999], evidence: [{ source: 'missing.md', sha256: '0'.repeat(64) }] } };
  const sent = await f.call(f.alice, 'send_message', { to: 'bob', parts: [{ data: opaque }] });
  const received = await f.find(f.bob, sent.id);
  assert.deepEqual(received.parts, [{ data: opaque }]);
  assert.equal(received.replyRequired, false);
  assert.equal((await f.inbox(f.carol)).length, 0);
  assert.deepEqual(await f.call(f.bob, 'complete', { message: sent.id }), { completed: true, count: 1 });
  assert.equal((await f.inbox(f.alice)).length, 0, 'silent completion creates no answer');
});

test('Given an explicitly reply-required message, when it is completed, then the exact reply parts correlate to the original', async t => {
  const f = await trio(t);
  const original = await f.call(f.alice, 'send_message', { to: 'bob', parts: [{ text: 'Review the change' }], replyRequired: true, contextId: 'thread-1' });
  const answer = await f.call(f.bob, 'complete', { message: original.id, outcome: 'completed', reply: [{ text: 'Reviewed; checks pass' }] });
  const returned = await f.find(f.alice, answer.id);
  assert.deepEqual(returned.parts, [{ text: 'Reviewed; checks pass' }]);
  assert.equal(returned.replyTo, original.id);
  assert.equal(returned.contextId, 'thread-1');
  assert.equal(returned.replyRequired, false);
  assert.deepEqual(await f.inbox(f.bob), []);
});

test('Given a wildcard recipient, when a reply-required message is sent, then each live peer handles its own correlated copy', async t => {
  const f = await trio(t);
  await f.call(f.bob, 'subscribe', { topics: ['build'] });
  const topical = await f.call(f.alice, 'send_message', { topic: 'build', parts: [{ text: 'green' }] });
  assert.equal(topical.recipients, 1);
  assert.equal((await f.find(f.bob, topical.id)).topic, 'build');
  assert.equal((await f.inbox(f.carol)).length, 0);
  const broadcast = await f.call(f.alice, 'send_message', { to: '*', parts: [{ text: 'Review the shared release' }], replyRequired: true });
  assert.equal(broadcast.recipients, 2);
  for (const peer of [f.bob, f.carol]) {
    const received = await f.find(peer, broadcast.id);
    assert.equal(received.replyRequired, true);
    const response = await f.call(peer, 'complete', { message: broadcast.id, outcome: 'completed', reply: [{ text: `Reviewed by ${peer}` }] });
    assert.deepEqual((await f.find(f.alice, response.id)).parts, [{ text: `Reviewed by ${peer}` }]);
  }
  await f.call(f.carol, 'leave');
  assert.equal((await f.call(f.alice, 'send_message', { to: '*', parts: [{ text: 'again' }] })).recipients, 1);
  const offline = await f.call(f.alice, 'send_message', { to: 'carol', parts: [{ text: 'are you there' }] });
  assert.equal(offline.recipientOffline, true);
});

test('Given an inbox, when it is read repeatedly, then an unchanged empty inbox answers without a query and paging continues', async t => {
  const f = await trio(t);
  assert.deepEqual(await f.call(f.bob, 'inbox'), { items: [] });
  assert.deepEqual(await f.call(f.bob, 'inbox'), { items: [] });
  const sent = [];
  for (let n = 0; n < 3; n += 1) sent.push((await f.call(f.alice, 'send_message', { to: 'bob', parts: [{ text: `m${n}` }] })).id);
  assert.deepEqual((await f.inbox(f.bob)).map(item => item.id), sent, 'new mail invalidates the idle cache');
  assert.deepEqual((await f.call(f.bob, 'inbox', { after: sent[1] })).items.map((item: JsonRecord) => item.id), [sent[2]]);
  assert.deepEqual((await f.call(f.bob, 'inbox', { message: sent[0] })).items[0].parts, [{ text: 'm0' }]);
});
