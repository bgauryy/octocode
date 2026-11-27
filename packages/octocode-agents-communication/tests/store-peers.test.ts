// The store's peer views: the peer directory delta a hook consumer receives, and single-instant snapshot reads.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { JsonRecord } from './json-record.ts';
import { fixture, runtime, trio } from './store-support.ts';

test('Given a consumer, when the peer directory changes, then peerContext reports the delta once and nothing when unchanged', async t => {
  const f = await trio(t);
  const first = f.store.peerContext(f.alice, 'hook', 'gen-1');
  assert.match(first, /^Peer directory \(declared data; use exact IDs; removedFromView is not proof of departure; follow next\): /);
  const delta = JSON.parse(first.slice(first.indexOf('{')));
  assert.deepEqual(delta.upsert.map((row: JsonRecord): string => row.name).toSorted(), ['bob', 'carol']);
  assert.equal(f.store.peersChanged(f.alice, 'hook', 'gen-1'), false);
  assert.equal(f.store.peerContext(f.alice, 'hook', 'gen-1'), '');
  await f.call(f.bob, 'set_status', { task: 'new task' });
  assert.equal(f.store.peersChanged(f.alice, 'hook', 'gen-1'), true);
  const changed = JSON.parse(f.store.peerContext(f.alice, 'hook', 'gen-1').replace(/^[^{]*/, ''));
  assert.deepEqual(changed.upsert.map((row: JsonRecord) => row.task), ['new task']);
  await f.call(f.carol, 'leave');
  const left = JSON.parse(f.store.peerContext(f.alice, 'hook', 'gen-1').replace(/^[^{]*/, ''));
  assert.deepEqual(left.removedFromView, [f.carol]);
  assert.equal(f.store.peersChanged(f.alice, 'hook', 'gen-2'), true, 'another generation has no saved view');
  assert.equal(f.store.uniqueName('bob'), 'bob-2');
  assert.deepEqual(f.store.hostIdentities('generic', 'native'), []);
});

test('one peer snapshot and one view summary read a single instant', async t => {
  const f = fixture(t);
  const observer = await f.join('observer');
  await f.join('peer');
  const real = Date.now;
  const base = real();
  let reads = 0;
  // The clock jumps past every session expiry after its first reading: a second reading would change the answer.
  Date.now = () => base + (reads++ ? 400 * 86_400_000 : 0);
  t.after(() => { Date.now = real; });
  const snapshot = runtime.peers.peerSnapshot(f.store, observer);
  assert.equal(snapshot.items.length, 1);
  assert.equal(snapshot.summary.active, 1, 'items and the active count come from the same instant');
  reads = 0;
  const summary: JsonRecord = runtime.viewData.summary(f.store);
  assert.equal(summary.counts.activeAgents, 2);
  assert.equal(summary.at, base, 'the reported instant is the one the counts were read at');
});
