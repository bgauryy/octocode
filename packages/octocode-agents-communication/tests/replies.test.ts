import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.ts';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { JsonRecord } from './json-record.ts';
import { binary, tempWorkspace, only } from './helpers.ts';

interface Fixture { workspace: string; database: string }


function fixture(t: TestContext): Fixture {
  const workspace = tempWorkspace(t, 'communication-replies-', { real: true });
  return { workspace, database: join(workspace, 'store.sqlite') };
}
function cli(f: Fixture, args: string[], session?: string | null): JsonRecord {
  return JSON.parse(execFileSync(binary, [...args, '--workspace', f.workspace, '--database', f.database, ...(session ? ['--session', session] : [])], { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
}
const call = (f: Fixture, session: string | null, name: string, data: Record<string, unknown> = {}) => cli(f, [name, JSON.stringify(data)], session);
const agent = (f: Fixture, name: string) => call(f, null, 'join', { name, vendor: 'generic' }).id;
const send = (f: Fixture, session: string, data: Record<string, unknown>) => call(f, session, 'send_message', { reasoning: 'Verify durable conversation correlation', ...data });
const entity = (f: Fixture, session: string, id: number) => cli(f, ['fetch', JSON.stringify({type:'message',where:{messageId:id}})], session).items[0]?.data ?? null;

test('conflicting routes explain how to recover a new direct request without storing malformed mail', t => {
  const f = fixture(t), a = agent(f, 'a'), b = agent(f, 'b');
  const input = {to:b,topic:'review-challenge',replyTo:1,parts:[{text:'Review the change'}]};
  assert.throws(() => send(f,a,input), /Replies use complete/);
  const {topic,replyTo,...direct} = input;
  const sent = send(f,a,{ ...direct, parts: [{ text: direct.parts[0].text }], replyRequired: true });
  assert.equal(sent.id,1,'Rejected routes must not create messages');
  assert.equal(entity(f,b,sent.id).replyRequired,true);
});

test('replies inherit correlation and remain visible only to participants', t => {
  const f = fixture(t), a = agent(f, 'a'), b = agent(f, 'b'), outsider = agent(f, 'outsider');
  const rootMessage = send(f, a, { to: b, parts: [{ text: 'Which tools do you have?' }], replyRequired: true, contextId: 'task:42', key: 'question' });
  const reply = { message: rootMessage.id, outcome:'completed', reply:[{text:'I have code search.'}] };
  const response = call(f, b, 'complete', reply);
  assert.deepEqual(call(f, b, 'complete', reply), response);
  assert.equal(entity(f, b, response.id).contextId, 'task:42');
  assert.equal(entity(f, a, response.id).replyTo, rootMessage.id);
  const inbox = call(f, a, 'inbox').items[0];
  assert.equal(inbox.contextId, 'task:42'); assert.equal(inbox.replyTo, rootMessage.id);
  const followup = send(f, a, { to: b, parts: [{ text: 'Please inspect the interface.' }], contextId: 'task:42' });
  assert.equal(entity(f, b, followup.id).contextId, 'task:42');
  assert.equal(cli(f, ['fetch', '{"type":"message","where":{"contextId":"task:42"}}'], a).items.length, 3);
  assert.deepEqual(cli(f, ['fetch', JSON.stringify({type:'message',where:{replyTo:rootMessage.id}})], a).items.map((x: JsonRecord) => x.data.messageId), [response.id]);
  assert.equal(cli(f, ['fetch', '{"type":"message","where":{"contextId":"task:42"}}'], outsider).items.length, 0);
  assert.equal(entity(f, outsider, rootMessage.id), null);
  assert.throws(() => call(f, outsider, 'complete', reply));
  assert.throws(() => call(f, b, 'complete', {...reply,outcome:"completed",reply:[{ data: {conclusion:'changed'} }]}));
  mkdirSync(join(f.workspace, 'other'));
  const other = { ...f, workspace: join(f.workspace, 'other') }, foreign = agent(other, 'foreign');
  assert.throws(() => call(other, foreign, 'complete', reply));
  const db = new DatabaseSync(f.database);
  const audit = JSON.parse(only<JsonRecord>(db.prepare("SELECT data FROM records WHERE type='message' AND entityId=?").get(String(response.id))).data);
  assert.deepEqual(audit, { messageId: response.id }, 'audit rows retain the message reference; fetch hydrates authoritative metadata');
  assert.equal(only(db.prepare('SELECT count(*) n FROM messages').get()).n, 3);
  db.close();
});

test('correlation validation is enforced in SQL as well as the CLI', t => {
  const f = fixture(t), a = agent(f, 'a'), b = agent(f, 'b'), outsider = agent(f, 'outsider');
  const question = send(f, a, { to: b, parts: [{ text: 'question' }], replyRequired: true, contextId: 'case.1' });
  for (const contextId of ['', 'x'.repeat(257), 'two words', 'é', 'a\n']) {
    assert.throws(() => send(f, a, { to: b, parts: [{ text: 'invalid' }], contextId }));
  }
  for (const replyTo of [null, 0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => send(f, b, { to: a, parts: [{ text: 'invalid' }], replyTo }));
  }
  const db = new DatabaseSync(f.database); db.exec('PRAGMA foreign_keys=ON');
  assert.throws(() => db.prepare('UPDATE messages SET replyTo=? WHERE id=?').run(question.id, question.id), /immutable/);
  assert.throws(() => db.prepare('UPDATE messages SET contextId=? WHERE id=?').run('changed', question.id), /immutable/);
  const insert = db.prepare("INSERT INTO messages(sender,target,parts,key,expiresAt,reasoning,contextId,replyTo,replyRequired,outcome) VALUES(?,?,?,?,?,?,?,?,0,'completed')");
  const textParts = JSON.stringify([{text:'answer'}]);
  assert.throws(() => insert.run(outsider, a, textParts, 'hidden', Date.now() + 60000, 'test', 'case.1', question.id), /visible parent/);
  assert.throws(() => insert.run(b, a, textParts, 'wrong', Date.now() + 60000, 'test', 'other', question.id), /visible parent/);
  assert.throws(() => db.prepare('INSERT INTO messages(sender,target,parts,key,expiresAt,reasoning,contextId) VALUES(?,?,?,?,?,?,?)').run(a, b, textParts, 'bad', Date.now() + 60000, 'test', 'non ASCII 🦀'), /CHECK constraint/);
  const valid = insert.run(b, a, textParts, 'sql', Date.now() + 60000, 'test', 'case.1', question.id);
  assert.equal(entity(f, b, Number(valid.lastInsertRowid)).replyTo, question.id);
  db.close();
});

test('complete alone infers the recipient and prevents the duplicate send-then-complete reply path', t => {
 const f=fixture(t),a=agent(f,'questioner'),b=agent(f,'responder'),c=agent(f,'observer');
 call(f,b,'subscribe',{topics:['requests']});
 const question=send(f,a,{topic:'requests',parts:[{text:'Review?'}],replyRequired:true,contextId:'review:inferred'});
 const payload={message:question.id,outcome:'completed',reply:[{text:'Reviewed'}]};
 assert.throws(()=>send(f,b,{replyTo:question.id,parts:[{text:'Reviewed'}]}),/Replies use complete/);
 const reply=call(f,b,'complete',payload),row=entity(f,a,reply.id);
 assert.equal(cli(f,['fetch',JSON.stringify({type:'message',where:{messageId:reply.id}})],a).items[0].to,a);assert.equal(row.replyTo,question.id);assert.equal(row.contextId,'review:inferred');
 assert.deepEqual(call(f,b,'complete',payload),reply);
 assert.throws(()=>call(f,c,'complete',payload));
 assert.throws(()=>send(f,b,{parts:[{text:'No target'}]}),/Supply exactly one/);
 assert.equal(cli(f,['schema','send_message']).inputSchema.properties.replyTo,undefined);
 assert.equal(cli(f,['fetch',JSON.stringify({type:'message',where:{replyTo:question.id}})],a).items.length,1);
});
