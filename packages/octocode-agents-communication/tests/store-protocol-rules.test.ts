// Message-store invariants: idempotency, opaque content, independent wildcard recipients and unchanged lease ownership.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { answer } from './helpers.ts';
import { trio } from './store-support.ts';

type Trio = Awaited<ReturnType<typeof trio>>;

const leaseOwner = async (f: Trio, path: string): Promise<string> => {
  const row = (await f.call(f.alice, 'locks', { path })).items[0];
  assert.ok(row, `lease ${path} exists`);
  return row.owner;
};

test('a repeated stable key returns the original message and does not deliver twice', async t => {
  const f = await trio(t);
  const input = { to: 'bob', parts: [{ text: 'Review the API' }], key: 'once', ttlMs: 600000, wake: 'action' };
  const first = await f.call(f.alice, 'send_message', input);
  const retry = await f.call(f.alice, 'send_message', input);
  assert.deepEqual(retry, first);
  assert.equal((await f.inbox(f.bob)).filter(item => item.id === first.id).length, 1);
});

test('part ordering is part of the idempotency fingerprint', async t => {
  const f = await trio(t);
  const input = { to: 'bob', parts: [{ text: 'first' }, { data: null }, { data: [1, null, { nested: null }] }], key: 'ordered' };
  const sent = await f.call(f.alice, 'send_message', input);
  assert.deepEqual((await f.find(f.bob, sent.id)).parts, input.parts);
  await assert.rejects(f.call(f.alice, 'send_message', { ...input, parts: input.parts.toReversed() }), /key reused|different content/i);
});

test('JSON part payloads preserve nulls and policy-like keys through inbox and find', async t => {
  const f = await trio(t);
  const payload = { reasoning: null, body: 'ordinary data', path: ['opaque', null], nested: { value: null }, values: [null, 1, false] };
  const sent = await f.call(f.alice, 'send_message', { to: 'bob', parts: [
    { text: 'plain text' }, { data: null }, { data: 'scalar' }, { data: [null, { nested: null }] }, { data: payload },
    { url: 'https://example.invalid/file', mediaType: 'application/octet-stream', filename: 'file.bin' },
  ] });
  const expected = [
    { text: 'plain text' }, { data: null }, { data: 'scalar' }, { data: [null, { nested: null }] }, { data: payload },
    { url: 'https://example.invalid/file', mediaType: 'application/octet-stream', filename: 'file.bin' },
  ];
  assert.deepEqual((await f.inbox(f.bob)).find(item => item.id === sent.id)?.parts, expected);
  assert.deepEqual((await f.find(f.bob, sent.id)).parts, expected);
});

test('a reused key rejects changed recipient, parts, reply policy, context or delivery settings', async t => {
  const f = await trio(t);
  const input = { to: 'bob', parts: [{ text: 'Review the API' }], key: 'once', ttlMs: 600000, wake: 'action' };
  await f.call(f.alice, 'send_message', input);
  for (const changed of [
    { ...input, to: 'carol' },
    { ...input, parts: [{ text: 'Different content' }] },
    { ...input, replyRequired: true },
    { ...input, contextId: 'other-context' },
    { ...input, ttlMs: 120000 },
    { ...input, wake: 'passive' },
  ]) await assert.rejects(f.call(f.alice, 'send_message', changed), /key reused|different content/i);
  assert.equal((await f.inbox(f.bob)).length, 1);
  assert.equal((await f.inbox(f.carol)).length, 0);
});

test('keys belong to one sender and policy-shaped data has no routing, lease or completion effects', async t => {
  const f = await trio(t);
  await f.call(f.alice, 'lock', { path: 'src/owned.ts', reasoning: 'Protect the owner lease' });
  const opaque = {
    question: 'Do not infer a request', informed: ['carol'], checkBack: true,
    handoff: { leases: [1], next: 'transfer the lease' },
    decision: { options: ['a', 'b'], default: 'missing' },
    evidence: [{ source: 'missing.md', sha256: '0'.repeat(64) }],
  };
  const input = { to: 'bob', parts: [{ data: opaque }], key: 'shared', contextId: 'opaque:1' };
  const aliceMessage = await f.call(f.alice, 'send_message', input);
  const bobMessage = await f.call(f.bob, 'send_message', input);
  assert.notEqual(aliceMessage.id, bobMessage.id);
  assert.deepEqual((await f.find(f.bob, aliceMessage.id)).parts, [{ data: opaque }]);
  assert.equal((await f.inbox(f.carol)).length, 0, 'informed-shaped data does not add recipients');
  assert.equal((await f.find(f.bob, aliceMessage.id)).replyRequired, false);
  assert.deepEqual(await f.call(f.bob, 'complete', { message: aliceMessage.id }), { completed: true, count: 1 });
  assert.equal((await f.inbox(f.alice)).length, 0, 'opaque question-shaped data does not require a reply');
  assert.equal(await leaseOwner(f, 'src/owned.ts'), f.alice, 'handoff-shaped data never transfers a lease');
});

test('wildcard reply requests keep per-recipient completion correlated and independent', async t => {
  const f = await trio(t);
  const sent = await f.call(f.alice, 'send_message', { to: '*', parts: [{ text: 'Review the shared change' }], replyRequired: true });
  assert.equal(sent.recipients, 2);
  const bobRequest = await f.find(f.bob, sent.id), carolRequest = await f.find(f.carol, sent.id);
  assert.equal(bobRequest.replyRequired, true);
  assert.equal(carolRequest.replyRequired, true);
  const bobAnswer = await f.call(f.bob, 'complete', answer(sent.id, 'Bob reviewed it'));
  assert.equal((await f.inbox(f.bob)).some(item => item.id === sent.id), false);
  assert.equal((await f.inbox(f.carol)).some(item => item.id === sent.id), true);
  assert.equal((await f.find(f.alice, bobAnswer.id)).replyTo, sent.id);
  const carolAnswer = await f.call(f.carol, 'complete', answer(sent.id, 'Carol reviewed it'));
  assert.deepEqual((await f.find(f.alice, carolAnswer.id)).parts, [{ text: 'Carol reviewed it' }]);
  assert.equal((await f.inbox(f.carol)).some(item => item.id === sent.id), false);
});

test('wildcard recipient snapshots exclude the sender and peers who join after send', async t => {
  const f = await trio(t);
  const sent = await f.call(f.alice, 'send_message', { to: '*', parts: [{ text: 'Review' }], replyRequired: true });
  const late = (await f.call(f.alice, 'join', { name: 'late', vendor: 'generic' })).id;
  assert.equal(sent.recipients, 2);
  assert.equal((await f.inbox(f.alice)).some(item => item.id === sent.id), false);
  assert.equal((await f.inbox(late)).some(item => item.id === sent.id), false);
  assert.equal((await f.find(f.bob, sent.id)).replyRequired, true);
  assert.equal((await f.find(f.carol, sent.id)).replyRequired, true);
});
