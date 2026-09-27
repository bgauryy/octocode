import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rmSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, tempDir, jsonCall } from './helpers.mjs';

function fixture(t) {
  const workspace = tempDir('communication-batching-');
  const database = join(workspace, 'audit.sqlite');
  const call = jsonCall(binary, workspace, database, { stdio: 'pipe', timeout: 10000 });
  const sender = call('join', { name: 'sender', vendor: 'generic' }).id;
  const receiver = call('join', { name: 'receiver', vendor: 'generic' }).id;
  call('attach', { transport: 'raw' }, receiver);
  const db = new DatabaseSync(database, { readOnly: true });
  t.after(() => { db.close(); rmSync(workspace, { recursive: true, force: true }); });
  const send = (body, to = receiver) => call('send_message', { to, body, replyRequired:false, reasoning: 'Coordinate a pending decision', conversationId: 'shared-decision' }, sender).id;
  const pending = () => db.prepare('SELECT message FROM deliveries WHERE recipient=? AND acknowledgedAt IS NULL ORDER BY message').all(receiver).map(x => x.message);
  const audits = () => db.prepare("SELECT count(*) AS n FROM audit WHERE kind='delivery.acknowledged'").get().n;
  return { call, sender, receiver, send, db, pending, audits };
}

test('batch ACK is atomic, recipient scoped, audited once and idempotent', t => {
  const f = fixture(t), ids = [f.send('first'), f.send('second')];
  const foreign = f.send('other recipient', f.sender);
  assert.throws(() => f.call('complete', { messages: [...ids, foreign] }, f.receiver), /every ID/);
  assert.throws(() => f.call('complete', { messages: [...ids, 999999] }, f.receiver), /every ID/);
  assert.deepEqual(f.pending(), ids);
  assert.equal(f.audits(), 0, 'rolled-back ACKs must leave no audit events');
  assert.deepEqual(f.call('complete', { messages: ids }, f.receiver), { completed: true, count: 2 });
  assert.deepEqual(f.pending(), []);
  assert.equal(f.audits(), 2);
  assert.deepEqual(f.call('complete', { messages: ids }, f.receiver), { completed: true, count: 2 });
  assert.equal(f.audits(), 2, 'retries must not create duplicate ACK audit events');
  assert.deepEqual(f.call('complete', { message: ids[0] }, f.receiver), { completed: true, count: 1 });
  assert.throws(() => f.call('complete', { message: foreign }, f.receiver));
});

test('ACK schema rejects ambiguous, empty, duplicate, invalid and oversized batches', t => {
  const f = fixture(t), id = f.send('retain');
  for (const input of [{}, { messages: [] }, { messages: [id, id] }, { messages: [0] },
    { messages: ['1'] }, { message: id, messages: [id] },
    { messages: Array.from({ length: 101 }, (_, i) => i + 1) }]) {
    assert.throws(() => f.call('complete', input, f.receiver));
    assert.deepEqual(f.pending(), [id]);
  }
});

test('ready bursts retain every field and ID, drain 16 at a time and never replay', t => {
  const f = fixture(t), ids = Array.from({ length: 33 }, (_, i) => f.send(`fact ${i}`));
  const batches = Array.from({ length: 3 }, () => f.call('hook', { format: 'json' }, f.receiver));
  assert.deepEqual(batches.map(x => x.items.length), [16, 16, 1]);
  assert.deepEqual(batches.flatMap(x => x.items.map(m => m.id)), ids);
  for (const batch of batches) {
    const rendered = JSON.parse(batch.context.slice(batch.context.indexOf('\n') + 1));
    assert.deepEqual(rendered.map(x => x.id), batch.items.map(x => x.id));
    assert.equal(rendered[0].sender, f.sender);
    for (const [index, message] of rendered.entries()) {
      assert.equal(message.reasoning, 'Coordinate a pending decision');
      assert.equal(message.conversationId, 'shared-decision');
      assert.equal(message.from, 'sender');
      if (index) assert.equal(message.sender, undefined);
      assert.equal(message.body, `fact ${ids.indexOf(message.id)}`);
      assert.equal(message.wake, undefined);
      assert.equal(message.dispatchToken, undefined);
    }
    f.call('complete', { messages: batch.items.map(x => x.id) }, f.receiver);
  }
  assert.equal(f.call('hook', { format: 'json' }, f.receiver).context, undefined);
  assert.deepEqual(f.pending(), []);
  assert.equal(f.audits(), 33);
});

test('larger row cap preserves byte bound and oversized-first-item progress', t => {
  const f = fixture(t);
  const ids = [f.send('a'.repeat(10000)), f.send('b'.repeat(10000)), f.send('c'.repeat(16384))];
  for (const id of ids) {
    const batch = f.call('hook', { format: 'json' }, f.receiver);
    assert.deepEqual(batch.items.map(x => x.id), [id]);
  }
  assert.deepEqual(f.call('hook', { format: 'json' }, f.receiver).items, []);
});

// Reproduces the mixed batch where an agent handled requests but overlooked an answer.
test('mixed delivered batches enumerate request and notice obligations without completing either', t => {
 const f=fixture(t);
 const request=f.call('send_message',{to:f.receiver,body:'Please review',reasoning:'Mixed batch regression'},f.sender).id;
 const answer=f.send('A received answer is still a notice to complete');
 const batch=f.call('hook',{format:'json'},f.receiver);
 assert.ok(batch.context.split('\n')[0].includes(`Requests: [${request}]. Notices/answers: [${answer}].`));
 assert.deepEqual(f.pending(),[request,answer],'Delivery instructions never complete work automatically');
 f.call('complete',{messages:[answer]},f.receiver);
 assert.deepEqual(f.pending(),[request],'A handled answer does not complete the separate request');
});
