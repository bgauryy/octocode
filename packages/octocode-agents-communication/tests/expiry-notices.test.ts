import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, jsonCall, only, tempWorkspace, withReasoning, messageText } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-expiry-', { real: true }), database = join(workspace, 'expiry.sqlite');
  const raw = jsonCall(binary, workspace, database, { stdio: 'pipe' });
  const call = (command: string, input: Record<string, unknown>, session?: string) => raw(command, withReasoning(command, input), session);
  const join_ = (name: string) => call('join', { name, vendor: 'generic' }).id;
  const db = new DatabaseSync(database); t.after(() => db.close());
  const expire = (id: number) => db.prepare('UPDATE messages SET expiresAt=1 WHERE id=?').run(id);
  const notices = (session: string) => call('inbox', {}, session).items.filter((item: JsonRecord) => messageText(item.parts).includes('expired before'));
  return { call, join: join_, db, expire, notices };
}

test('an action message that expires unhandled notifies its sender once', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob');
  const sent = f.call('send_message', {parts: [{ text: 'Please review' }], replyRequired: true,  to: bob, wake: 'action', ttlMs: 1000 }, alice);
  assert.deepEqual(f.notices(alice), [], 'nothing before expiry');
  f.expire(sent.id);
  f.call('heartbeat', {}, alice);
  const [notice] = f.notices(alice);
  assert.ok(Array.isArray(notice.parts));
  assert.equal(notice.wake, 'action');
  assert.equal(notice.replyRequired, false);
  assert.deepEqual(f.call('inbox', {}, bob).items, [], 'the expired request itself is never delivered');
  f.call('heartbeat', {}, alice); f.call('heartbeat', {}, bob);
  assert.equal(f.notices(alice).length, 1, 'one notice per expired message');
  assert.equal(only(f.db.prepare("SELECT count(*) n FROM messages WHERE key=?").get(`expired:${sent.id}`)).n, 1);
});

test('the notice names every recipient that never acknowledged', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob'), carol = f.join('carol');
  f.call('subscribe', { topics: ['review'] }, bob); f.call('subscribe', { topics: ['review'] }, carol);
  const sent = f.call('send_message', {parts: [{ text: 'Anyone free?' }],  topic: 'review', wake: 'action', ttlMs: 1000 }, alice);
  assert.equal(sent.recipients, 2);
  f.call('complete', { messages: [sent.id] }, carol);
  f.expire(sent.id);
  f.call('heartbeat', {}, alice);
  const [notice] = f.notices(alice);
  assert.match(messageText(notice.parts), /bob/);
  assert.doesNotMatch(messageText(notice.parts), /carol/, 'an acknowledged recipient is not reported');
});

test('expiry after host submission reports unconfirmed handling and preserves a late answer', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob');
  f.call('attach', { transport: 'raw' }, bob);
  const sent = f.call('send_message', {parts: [{ text: 'Please review' }], replyRequired: true,  to: bob, }, alice);
  const delivered = f.call('hook', { format: 'json' }, bob);
  assert.deepEqual(delivered.items, [{ id: sent.id }]);
  assert.equal(only(f.db.prepare('SELECT state FROM dispatches WHERE message=? AND recipient=?').get(sent.id, bob)).state, 'submitted');
  f.expire(sent.id);
  f.call('heartbeat', {}, alice);
  const [notice] = f.notices(alice);
  assert.ok(Array.isArray(notice.parts));
  assert.deepEqual(f.call('hook', { format: 'json' }, bob).items, [], 'expiry does not replay submitted context');
  const reply = f.call('complete', { message: sent.id, outcome: 'completed', reply:[{ text:'Reviewed before expiry; completion arrived later.' }] }, bob);
  assert.equal(f.call('inbox', {}, alice).items.find((item: JsonRecord) => item.id === reply.id).replyTo, sent.id);
  f.call('heartbeat', {}, alice);
  assert.equal(f.notices(alice).length, 1, 'late completion creates no second notice');
});

test('handled, passive and self-addressed messages stay silent', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob');
  const handled = f.call('send_message', {parts: [{ text: 'Done soon' }],  to: bob, wake: 'action', ttlMs: 1000 }, alice);
  f.call('complete', { messages: [handled.id] }, bob);
  const passive = f.call('send_message', {parts: [{ text: 'FYI' }],  to: bob, wake: 'passive', ttlMs: 1000 }, alice);
  f.expire(handled.id); f.expire(passive.id);
  f.call('heartbeat', {}, alice);
  assert.deepEqual(f.notices(alice), []);
  // A runtime notice that expires unread is not itself reported: no notice chain.
  const sent = f.call('send_message', {parts: [{ text: 'Please review' }], replyRequired: true,  to: bob, wake: 'action', ttlMs: 1000 }, alice);
  f.expire(sent.id);
  f.call('heartbeat', {}, alice);
  const [notice] = f.notices(alice);
  f.expire(notice.id);
  f.call('heartbeat', {}, alice);
  assert.equal(only(f.db.prepare("SELECT count(*) n FROM messages WHERE key LIKE 'expired:%'").get()).n, 1);
});

test('a resumed sender learns of requests that expired while it was away', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob');
  const sent = f.call('send_message', {parts: [{ text: 'Please review' }], replyRequired: true,  to: bob, wake: 'action', ttlMs: 1000 }, alice);
  f.expire(sent.id);
  f.db.prepare('UPDATE sessions SET expiresAt=1 WHERE id=?').run(alice);
  f.call('heartbeat', {}, bob);
  assert.equal(only(f.db.prepare("SELECT count(*) n FROM messages WHERE key LIKE 'expired:%'").get()).n, 0, 'only the sender\'s own heartbeat reports its messages');
  f.call('resume', { vendor: 'generic' }, alice);
  f.call('heartbeat', {}, alice);
  assert.equal(f.notices(alice).length, 1);
});

test('keys beginning expired: are reserved for runtime notices', t => {
  const f = fixture(t), alice = f.join('alice'), bob = f.join('bob');
  assert.throws(() => f.call('send_message', {parts: [{ text: 'spoof' }],  to: bob, key: 'expired:1' }, alice), /reserved/);
});
