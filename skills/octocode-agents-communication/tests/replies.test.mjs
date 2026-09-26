import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, tempWorkspace } from './helpers.mjs';

function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-replies-', { real: true });
  return { workspace, database: join(workspace, 'store.sqlite') };
}
function cli(f, args, session) {
  return JSON.parse(execFileSync(binary, [...args, '--workspace', f.workspace, '--database', f.database, ...(session ? ['--session', session] : [])], { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
}
const call = (f, session, name, data = {}) => cli(f, [name, JSON.stringify(data)], session);
const agent = (f, name) => call(f, null, 'join', { name, vendor: 'generic' }).id;
const send = (f, session, data) => call(f, session, 'send_message', { reasoning: 'Verify durable conversation correlation', ...data });
const entity = (f, session, id) => cli(f, ['entity', 'get', 'message', String(id)], session);

test('replies inherit correlation and remain visible only to participants', t => {
  const f = fixture(t), a = agent(f, 'a'), b = agent(f, 'b'), outsider = agent(f, 'outsider');
  const rootMessage = send(f, a, { to: b, body: 'Which tools do you have?', conversationId: 'task:42', key: 'question' });
  const reply = { to: a, body: 'I have code search.', replyTo: rootMessage.id, key: 'answer' };
  const response = send(f, b, reply);
  assert.deepEqual(send(f, b, reply), response);
  assert.deepEqual(send(f, b, { ...reply, conversationId: 'task:42' }), response);
  assert.equal(entity(f, b, response.id).conversationId, 'task:42');
  assert.equal(entity(f, a, response.id).replyTo, rootMessage.id);
  const inbox = call(f, a, 'inbox').items[0];
  assert.equal(inbox.conversationId, 'task:42'); assert.equal(inbox.replyTo, rootMessage.id);
  const followup = send(f, a, { to: b, body: 'Please inspect the interface.', replyTo: response.id });
  assert.equal(entity(f, b, followup.id).conversationId, 'task:42');
  assert.equal(cli(f, ['entity', 'list', 'message', '{"conversationId":"task:42"}'], a).items.length, 3);
  assert.deepEqual(cli(f, ['entity', 'list', 'message', JSON.stringify({ replyTo: rootMessage.id })], a).items.map(x => x.id), [response.id]);
  assert.equal(cli(f, ['entity', 'list', 'message', '{"conversationId":"task:42"}'], outsider).items.length, 0);
  assert.equal(entity(f, outsider, rootMessage.id), null);
  assert.throws(() => send(f, outsider, { to: a, body: 'hidden reply', replyTo: rootMessage.id }), /visible parent/);
  assert.throws(() => send(f, b, { ...reply, conversationId: 'wrong' }), /must match/);
  assert.throws(() => send(f, b, { ...reply, replyTo: response.id }), /key reused/);
  mkdirSync(join(f.workspace, 'other'));
  const other = { ...f, workspace: join(f.workspace, 'other') }, foreign = agent(other, 'foreign');
  assert.throws(() => send(other, foreign, { to: foreign, body: 'foreign reply', replyTo: rootMessage.id }), /visible parent/);
  const db = new DatabaseSync(f.database);
  const audit = JSON.parse(db.prepare("SELECT data FROM audit WHERE kind='message.created' AND entityId=?").get(String(response.id)).data);
  assert.equal(audit.replyTo, rootMessage.id); assert.equal(audit.conversationId, 'task:42');
  assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n, 3);
  db.close();
});

test('correlation validation is enforced in SQL as well as the CLI', t => {
  const f = fixture(t), a = agent(f, 'a'), b = agent(f, 'b'), outsider = agent(f, 'outsider');
  const question = send(f, a, { to: b, body: 'question', conversationId: 'case.1' });
  for (const conversationId of ['', 'x'.repeat(129), 'two words', 'é', 'a\n']) {
    assert.throws(() => send(f, a, { to: b, body: 'invalid', conversationId }));
  }
  for (const replyTo of [null, 0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => send(f, b, { to: a, body: 'invalid', replyTo }));
  }
  const db = new DatabaseSync(f.database); db.exec('PRAGMA foreign_keys=ON');
  assert.throws(() => db.prepare('UPDATE messages SET replyTo=? WHERE id=?').run(question.id, question.id), /immutable/);
  assert.throws(() => db.prepare('UPDATE messages SET conversationId=? WHERE id=?').run('changed', question.id), /immutable/);
  const insert = db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,conversationId,replyTo) VALUES(?,?,?,?,?,?,?,?)');
  assert.throws(() => insert.run(outsider, a, 'hidden', 'hidden', Date.now() + 60000, 'test', 'case.1', question.id), /visible parent/);
  assert.throws(() => insert.run(b, a, 'wrong', 'wrong', Date.now() + 60000, 'test', 'other', question.id), /visible parent/);
  assert.throws(() => insert.run(a, b, 'bad', 'bad', Date.now() + 60000, 'test', 'non ASCII 🦀', null), /CHECK constraint/);
  const valid = insert.run(b, a, 'answer', 'sql', Date.now() + 60000, 'test', 'case.1', question.id);
  assert.equal(entity(f, b, Number(valid.lastInsertRowid)).replyTo, question.id);
  db.close();
});

test('replyTo alone infers the visible parent sender without correcting explicit targets', t => {
  const f = fixture(t), a = agent(f, 'questioner'), b = agent(f, 'responder'), c = agent(f, 'observer');
  call(f, b, 'subscribe', { topics: ['requests'] });
  const question = send(f, a, { topic: 'requests', body: 'Can you review?', conversationId: 'review:inferred' });
  const payload = { body: 'Review complete.', replyTo: question.id, key: 'reply-without-uuid' };
  const reply = send(f, b, payload);
  const row = entity(f, a, reply.id);
  assert.equal(row.target, a); assert.equal(row.topic ?? null, null); assert.equal(row.wake, 'action');
  assert.equal(row.conversationId, 'review:inferred'); assert.equal(row.replyTo, question.id);
  assert.equal(reply.recipients, 1);
  assert.deepEqual(send(f, b, payload), reply);
  assert.deepEqual(send(f, b, { ...payload, to: a }), reply, 'explicit and inferred same recipient normalize to one retry');
  assert.throws(() => send(f, c, { body: 'Hidden parent', replyTo: question.id }), /visible parent/);
  assert.throws(() => send(f, b, { body: 'No target' }), /Supply exactly one/);
  assert.throws(() => send(f, b, { ...payload, key: 'invalid-target', to: 'copied-uuid-with-typo' }), /Unknown or expired session/);
  assert.throws(() => send(f, b, { ...payload, to: a, topic: 'requests' }), /Supply exactly one/);
  const explicit = send(f, b, { ...payload, key: 'explicit-peer', to: c });
  assert.equal(entity(f, c, explicit.id).target, c, 'a supplied valid target remains authoritative');
  call(f, c, 'subscribe', { topics: ['reports'] });
  const topic = send(f, b, { ...payload, key: 'explicit-topic', topic: 'reports' });
  const topicRow = entity(f, c, topic.id);
  assert.equal(topicRow.target, 'reports'); assert.equal(topicRow.topic, 'reports'); assert.equal(topicRow.wake, 'passive');
  assert.equal(entity(f, c, question.id), null, 'reply correlation grants no parent visibility');
});
