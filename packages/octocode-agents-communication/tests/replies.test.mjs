import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.mjs';
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
const entity = (f, session, id) => cli(f, ['fetch', JSON.stringify({type:'message',where:{messageId:id}})], session).items[0]?.data ?? null;

test('conflicting routes explain how to recover a new direct request without storing malformed mail', t => {
  const f = fixture(t), a = agent(f, 'a'), b = agent(f, 'b');
  const input = {to:b,topic:'review-challenge',replyTo:1,replyRequired:true,body:'Review the change'};
  assert.throws(() => send(f,a,input), /Replies use complete/);
  const {topic,replyTo,...direct} = input;
  const sent = send(f,a,direct);
  assert.equal(sent.id,1,'Rejected routes must not create messages');
  assert.equal(entity(f,b,sent.id).replyRequired,true);
});

test('replies inherit correlation and remain visible only to participants', t => {
  const f = fixture(t), a = agent(f, 'a'), b = agent(f, 'b'), outsider = agent(f, 'outsider');
  const rootMessage = send(f, a, { to: b, body: 'Which tools do you have?', conversationId: 'task:42', key: 'question' });
  const reply = { message: rootMessage.id, reply: 'I have code search.' };
  const response = call(f, b, 'complete', reply);
  assert.deepEqual(call(f, b, 'complete', reply), response);
  assert.equal(entity(f, b, response.id).conversationId, 'task:42');
  assert.equal(entity(f, a, response.id).replyTo, rootMessage.id);
  const inbox = call(f, a, 'inbox').items[0];
  assert.equal(inbox.conversationId, 'task:42'); assert.equal(inbox.replyTo, rootMessage.id);
  const followup = send(f, a, { to: b, body: 'Please inspect the interface.', conversationId: 'task:42' });
  assert.equal(entity(f, b, followup.id).conversationId, 'task:42');
  assert.equal(cli(f, ['fetch', '{"type":"message","where":{"conversationId":"task:42"}}'], a).items.length, 3);
  assert.deepEqual(cli(f, ['fetch', JSON.stringify({type:'message',where:{replyTo:rootMessage.id}})], a).items.map(x => x.data.messageId), [response.id]);
  assert.equal(cli(f, ['fetch', '{"type":"message","where":{"conversationId":"task:42"}}'], outsider).items.length, 0);
  assert.equal(entity(f, outsider, rootMessage.id), null);
  assert.throws(() => call(f, outsider, 'complete', reply));
  assert.throws(() => call(f, b, 'complete', {...reply,reply:'changed'}));
  mkdirSync(join(f.workspace, 'other'));
  const other = { ...f, workspace: join(f.workspace, 'other') }, foreign = agent(other, 'foreign');
  assert.throws(() => call(other, foreign, 'complete', reply));
  const db = new DatabaseSync(f.database);
  const audit = JSON.parse(db.prepare("SELECT data FROM records WHERE type='message' AND entityId=?").get(String(response.id)).data);
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
  const insert = db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,conversationId,replyTo,replyRequired) VALUES(?,?,?,?,?,?,?,?,0)');
  assert.throws(() => insert.run(outsider, a, 'hidden', 'hidden', Date.now() + 60000, 'test', 'case.1', question.id), /visible parent/);
  assert.throws(() => insert.run(b, a, 'wrong', 'wrong', Date.now() + 60000, 'test', 'other', question.id), /visible parent/);
  assert.throws(() => insert.run(a, b, 'bad', 'bad', Date.now() + 60000, 'test', 'non ASCII 🦀', null), /CHECK constraint/);
  const valid = insert.run(b, a, 'answer', 'sql', Date.now() + 60000, 'test', 'case.1', question.id);
  assert.equal(entity(f, b, Number(valid.lastInsertRowid)).replyTo, question.id);
  db.close();
});

test('complete alone infers the recipient and prevents the duplicate send-then-complete reply path', t => {
 const f=fixture(t),a=agent(f,'questioner'),b=agent(f,'responder'),c=agent(f,'observer');
 call(f,b,'subscribe',{topics:['requests']});
 const question=send(f,a,{topic:'requests',body:'Review?',replyRequired:true,conversationId:'review:inferred'});
 const payload={message:question.id,reply:'Reviewed'};
 assert.throws(()=>send(f,b,{replyTo:question.id,body:'Reviewed'}),/Replies use complete/);
 const reply=call(f,b,'complete',payload),row=entity(f,a,reply.id);
 assert.equal(cli(f,['fetch',JSON.stringify({type:'message',where:{messageId:reply.id}})],a).items[0].to,a);assert.equal(row.replyTo,question.id);assert.equal(row.conversationId,'review:inferred');
 assert.deepEqual(call(f,b,'complete',payload),reply);
 assert.throws(()=>call(f,c,'complete',payload));
 assert.throws(()=>send(f,b,{body:'No target'}),/Supply exactly one/);
 assert.equal(cli(f,['schema','send_message']).inputSchema.properties.replyTo,undefined);
 assert.equal(cli(f,['fetch',JSON.stringify({type:'message',where:{replyTo:question.id}})],a).items.length,1);
});
