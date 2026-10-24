import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from './helpers.ts';
import { createInterface } from 'node:readline';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { JsonRecord } from './json-record.ts';
import { binary, only, tempWorkspace, jsonCall } from './helpers.ts';

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-completion-', { real: true });
  const database = join(workspace, 'mail.sqlite');
  const cli = jsonCall(binary, workspace, database, { stdio: 'pipe', timeout: 10000 });
  const sender = cli('join', { name: 'requester', vendor: 'raw' }).id;
  const receiver = cli('join', { name: 'worker', vendor: 'raw' }).id;
  const other = cli('join', { name: 'other', vendor: 'raw' }).id;
  const send = (input: Record<string, unknown>, session = receiver) => cli('send_message', input, session);
  const complete = (input: Record<string, unknown>, session = receiver) => cli('complete', input, session);
  const request = cli('send_message', { to: receiver, question: 'Review the change', reasoning: 'Need review before release', key: 'request' }, sender);
  const db = new DatabaseSync(database);
  t.after(() => db.close());
  const stamp = () => only(db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=? AND recipient=?').get(request.id, receiver)).acknowledgedAt;
  const reply = { message: request.id, outcome:'done',reply:{conclusion:'Reviewed; checks pass'}, reasoning: 'Return completed review' };
  return { cli, send, complete, db, sender, receiver, other, request, reply, stamp, workspace, database };
}

test('complete reply commits final response and handling atomically; identical retry is idempotent', t => {
  const f = fixture(t), sent = f.complete(f.reply);
  assert.deepEqual(sent, {completed:true,count:1,id:sent.id,recipients:1});
  const at=f.stamp();assert.equal(typeof at,'number');
  assert.deepEqual(f.complete(f.reply),sent);assert.equal(f.stamp(),at);
  assert.deepEqual(f.complete({message:f.request.id}),{completed:true,count:1});
  assert.throws(()=>f.complete({...f.reply,outcome:'done',reply:{conclusion:'Different result'}}),/Final reply already stored with different content; use send_message for new work/);
  assert.throws(()=>f.complete({...f.reply,reasoning:'Different reason'}));
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages WHERE sender=?').get(f.receiver)).n,1);
  assert.equal(only(f.db.prepare("SELECT count(*) n FROM records WHERE type='delivery.acknowledged'").get()).n,1);
  const response=only(f.db.prepare('SELECT * FROM messages WHERE id=?').get(sent.id));
  assert.equal(response.key,`complete:${f.request.id}`);assert.equal(response.target,f.sender);
  assert.equal(response.replyTo,f.request.id);assert.equal(response.wake,'action');
  assert.equal(only(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(sent.id)).acknowledgedAt,null,'Recipient still must handle the reply');
});

test('complete stores a BLUF answer with its outcome in reasoning, idempotent, and only valid with message, outcome and reply', t => {
  const f = fixture(t), body = (id: number) => only(f.db.prepare('SELECT body FROM messages WHERE id=?').get(id)).body, why = (id: number) => only(f.db.prepare('SELECT reasoning FROM messages WHERE id=?').get(id)).reasoning;
  for (const input of [
    { ...f.reply, outcome: 'needs-input' }, { ...f.reply, outcome: 'Done' }, { ...f.reply, outcome: 1 },
  ]) assert.throws(() => f.complete(input), /not an allowed value|invalid type/, JSON.stringify(input));
  assert.throws(() => f.complete({ message: f.request.id, outcome: 'done' }), /needs one message, its outcome and reply/);
  assert.throws(() => f.complete({ messages: [f.request.id], outcome: 'done' }), /needs one message, its outcome and reply/);
  assert.throws(() => f.complete({ messages: [f.request.id], outcome: 'done', reply:{conclusion:'Batch'} }), /needs one message, its outcome and reply/);
  assert.throws(() => f.complete({ message: f.request.id, reply:{conclusion:'No outcome'} }), /needs one message, its outcome and reply/);
  assert.throws(() => f.complete({ ...f.reply, outcome: 'done', reply:{conclusion:'  '} }), /Invalid reply\.conclusion: blank/);
  assert.throws(() => f.complete({ ...f.reply, reply:{conclusion:'Fine',evidence:[{claim:'checked',source:'document:missing.md'}]} }), /cites unknown document missing\.md/);
  assert.equal(f.stamp(), null);
  const input = { ...f.reply, outcome: 'partial' }, sent = f.complete(input);
  assert.deepEqual(sent, { completed: true, count: 1, id: sent.id, recipients: 1 });
  assert.equal(body(sent.id), 'BLUF\nOutcome: partial\nConclusion: Reviewed; checks pass', 'the body is the rendered BLUF answer');
  assert.equal(why(sent.id), 'Outcome: partial; Return completed review');
  assert.deepEqual(f.complete(input), sent, 'identical retry returns the stored result');
  assert.throws(() => f.complete({ ...input, outcome: 'done' }), /Final reply already stored with different content/);
  assert.throws(() => f.complete(f.reply), /Final reply already stored with different content/);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages WHERE sender=?').get(f.receiver)).n, 1);
  const why2 = fixture(t), withWhy = why2.complete({ ...why2.reply, outcome: 'done', reasoning: 'Checks ran' });
  assert.equal(only(why2.db.prepare('SELECT reasoning FROM messages WHERE id=?').get(withWhy.id)).reasoning, 'Outcome: done; Checks ran');
  const full = fixture(t), rich = full.complete({ message: full.request.id, outcome: 'done', reply: { conclusion: 'Ship it', confidence: 'medium', assumptions: ['CI is green'], unknowns: ['Windows'], next: 'Tag the release' } });
  assert.equal(only(full.db.prepare('SELECT body FROM messages WHERE id=?').get(rich.id)).body, 'BLUF\nOutcome: done\nConclusion: Ship it\nConfidence: medium\nAssumptions:\n- CI is green\nUnknowns:\n- Windows\nNext: Tag the release');
  assert.equal(only(full.db.prepare('SELECT kind FROM messages WHERE id=?').get(rich.id)).kind, 'answer');
});

test('progress FYI leaves request pending; completion reasoning is its outcome and no follow-up reply after silent completion', t => {
  const f=fixture(t);
  f.send({to:f.sender,body:'Review ongoing',reasoning:'Progress update'});
  assert.equal(f.stamp(),null);
  const sent=f.complete({message:f.request.id,outcome:'done',reply:{conclusion:'Done'}});
  assert.equal(only(f.db.prepare('SELECT reasoning FROM messages WHERE id=?').get(sent.id)).reasoning,'Outcome: done');
  assert.deepEqual(f.complete({message:f.request.id,outcome:'done',reply:{conclusion:'Done'}}),sent);
  const second=f.cli('send_message',{to:f.receiver,body:'FYI',reasoning:'Context'},f.sender);
  assert.deepEqual(f.complete({message:second.id}),{completed:true,count:1});
  assert.deepEqual(f.complete({message:second.id}),{completed:true,count:1});
  assert.throws(()=>f.complete({message:second.id,outcome:'done',reply:{conclusion:'Unrequested extra final response'}}));
});

test('complete rejects missing/foreign identities, old API and unsupported routing or reply knobs', t => {
  const f=fixture(t);
  for(const input of [
    {}, {message:999999}, {message:f.request.id,outcome:'done',reply:{conclusion:''}}, {message:f.request.id,reasoning:'No reply'},
    {...f.reply,to:f.other}, {...f.reply,replyTo:f.request.id}, {...f.reply,body:'old'},
    {...f.reply,key:'caller-key'}, {...f.reply,wake:'passive'}, {...f.reply,ttlMs:1000},
    {...f.reply,ackReply:true}, {...f.reply,session:f.other}, {messages:[f.request.id],outcome:'done',reply:{conclusion:'Invalid batch reply'}},
    {messages:[f.request.id],reasoning:'Invalid batch reason'}, {message:f.request.id,messages:[f.request.id]},
  ]) assert.throws(()=>f.complete(input),JSON.stringify(input));
  assert.throws(()=>f.complete(f.reply,f.sender));
  assert.throws(()=>f.complete(f.reply,f.other));
  assert.throws(()=>f.cli('ack',{message:f.request.id},f.receiver));
  assert.throws(()=>f.send({replyTo:f.request.id,body:'old',reasoning:'old',ackReply:true}));
  assert.throws(()=>f.send({replyTo:f.request.id,body:'collision',reasoning:'collision',key:`complete:${f.request.id}`}));
  assert.equal(f.stamp(),null);assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n,1);
  f.db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(f.receiver);
  assert.throws(()=>f.complete(f.reply));assert.equal(f.stamp(),null);
});

test('injected reply or completion failure rolls back all writes and audit rows', async t => {
  const f=fixture(t), initial=only(f.db.prepare('SELECT count(*) n FROM records').get()).n;
  // Validate the original schema before fault injection; new CLI processes
  // correctly reject extra triggers during database startup validation.
  const child=spawn(binary,['mcp','--tools','complete','--workspace',f.workspace,'--database',f.database,'--session',f.receiver]);
  const lines=createInterface({input:child.stdout});
  let sequence=0;
  t.after(()=>{lines.close();child.kill('SIGKILL');});
  const request=(method: string,params: JsonRecord)=>new Promise<JsonRecord>((resolve,reject)=>{
    const timer=setTimeout(()=>{lines.off('line',onLine);reject(new Error('MCP response timeout'));},10000);
    const onLine=(line: string)=>{clearTimeout(timer);resolve(JSON.parse(line));};
    lines.once('line',onLine);
    child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:++sequence,method,params})+'\n');
  });
  const initialized=await request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'rollback-test',version:'1'}});
  assert.deepEqual(initialized.result.capabilities, { tools: {} });
  const complete=()=>request('tools/call',{name:'complete',arguments:f.reply});
  f.db.exec("CREATE TRIGGER fail_reply BEFORE INSERT ON messages WHEN NEW.key LIKE 'complete:%' BEGIN SELECT RAISE(ABORT,'Injected reply failure'); END");
  const replyFailure=await complete();assert.equal(replyFailure.result.isError,true);
  assert.match(replyFailure.result.content[0].text,/Injected reply failure/);
  assert.equal(f.stamp(),null);assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n,1);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM records').get()).n,initial);
  f.db.exec('DROP TRIGGER fail_reply');
  f.db.exec("CREATE TRIGGER fail_completion BEFORE UPDATE OF acknowledgedAt ON deliveries BEGIN SELECT RAISE(ABORT,'Injected completion failure'); END");
  const handlingFailure=await complete();assert.equal(handlingFailure.result.isError,true);
  assert.match(handlingFailure.result.content[0].text,/Injected completion failure/);
  assert.equal(f.stamp(),null);assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n,1);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM records').get()).n,initial);
  f.db.exec('DROP TRIGGER fail_completion');
  assert.equal(JSON.parse((await complete()).result.content[0].text).completed,true);
  child.stdin.end();
});

test('bound MCP exposes complete and rejects old tool/ackReply while completing one received request', t => {
  const f=fixture(t);
  const frames=[
    {id:1,method:'tools/list'},
    {id:2,method:'tools/call',params:{name:'ack',arguments:{message:f.request.id}}},
    {id:3,method:'tools/call',params:{name:'send_message',arguments:{replyTo:f.request.id,body:'old',reasoning:'old',ackReply:true}}},
    {id:4,method:'tools/call',params:{name:'complete',arguments:f.reply}},
  ].map(x=>JSON.stringify({jsonrpc:'2.0',...x})).join('\n')+'\n';
  const rows=execFileSync(binary,['mcp','--tools','send_message,complete','--workspace',f.workspace,'--database',f.database,'--session',f.receiver],{input:frames,encoding:'utf8',timeout:10000}).trim().split('\n').map((line): JsonRecord=>JSON.parse(line));
  assert.deepEqual(rows[0].result.tools.map((x: JsonRecord)=>x.name),['send_message','complete']);
  assert.equal(rows[0].result.tools[0].inputSchema.properties.ackReply,undefined);
  assert.equal(rows[0].result.tools[0].inputSchema.properties.replyRequired,undefined,'the message kind decides whether an answer is owed');
  assert.equal(rows[0].result.tools[0].inputSchema.properties.question.type,'string');
  for(const row of rows.slice(1,3))assert.ok(row.error||row.result?.isError);
  assert.equal(JSON.parse(rows[3].result.content[0].text).completed,true);assert.equal(typeof f.stamp(),'number');
});

test('required answers cannot be omitted individually or through a mixed completion batch',t=>{
  const f=fixture(t);
  const notice=f.cli('send_message',{to:f.receiver,body:'FYI',reasoning:'Context only'},f.sender);
  assert.throws(()=>f.complete({message:f.request.id}));
  assert.throws(()=>f.complete({messages:[notice.id,f.request.id]}));
  assert.equal(f.stamp(),null);
  assert.equal(only(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(notice.id)).acknowledgedAt,null);
  assert.equal(only(f.db.prepare("SELECT count(*) n FROM records WHERE type='delivery.acknowledged'").get()).n,0);
  f.send({to:f.sender,body:'Still reviewing',reasoning:'Partial work'});
  assert.throws(()=>f.complete({message:f.request.id}),'Partial reply is not a final answer');
  f.complete(f.reply);
  assert.deepEqual(f.complete({messages:[notice.id,f.request.id]}),{completed:true,count:2});
});

test('answers and notices cannot start reply loops; new work requires a new root request',t=>{
  const f=fixture(t),answer=f.complete(f.reply);
  assert.throws(()=>f.cli('send_message',{replyTo:answer.id,body:'Thanks; more details?',reasoning:'Reply loop'},f.sender));
  assert.throws(()=>f.cli('complete',{message:answer.id,outcome:'done',reply:{conclusion:'Thanks'}},f.sender));
  assert.deepEqual(f.cli('complete',{message:answer.id},f.sender),{completed:true,count:1});
  const notice=f.cli('send_message',{to:f.receiver,body:'Notice',reasoning:'FYI'},f.sender);
  assert.throws(()=>f.send({replyTo:notice.id,body:'Unasked answer',reasoning:'Loop'}));
  assert.throws(()=>f.complete({message:notice.id,outcome:'done',reply:{conclusion:'Unasked answer'}}));
  assert.deepEqual(f.complete({message:notice.id}),{completed:true,count:1});
  const next=f.cli('send_message',{to:f.receiver,question:'Please investigate a new issue',reasoning:'New work'},f.sender);
  assert.equal(f.complete({message:next.id,outcome:'done',reply:{conclusion:'Investigated'}}).completed,true);
});

test('reply policy is immutable across keyed retries and replies cannot demand another answer',t=>{
  const f=fixture(t),input={to:f.receiver,question:'Required review',reasoning:'Need evidence',key:'policy'};
  const first=f.cli('send_message',input,f.sender);
  assert.equal(f.cli('send_message',{...input},f.sender).id,first.id);
  const {question,...rest}=input;
  assert.throws(()=>f.cli('send_message',{...rest,body:question},f.sender),/Message key reused with different content/,'the same key as an FYI is not the request already stored');
  const notice={...rest,body:'FYI',key:'notice'};
  assert.equal(f.cli('send_message',notice,f.sender).id,f.cli('send_message',notice,f.sender).id);
  const {body:_fyi,...unbodied}=notice;
  assert.throws(()=>f.cli('send_message',{...unbodied,question:'FYI'},f.sender),/Message key reused with different content/);
  assert.throws(()=>f.send({replyTo:first.id,question:'Partial',reasoning:'Progress'}));
  for(const value of [true,false,'true',0,null])assert.throws(()=>f.cli('send_message',{...input,key:`invalid-${value}`,replyRequired:value},f.sender),/additional properties/);
  const partial=f.send({to:f.sender,body:'Partial',reasoning:'Progress'});
  assert.equal(f.cli('inbox',{message:partial.id},f.sender).items[0].replyRequired,false);
});

test('reply policy is explicit boolean in inbox, hooks and entity reads; only an explicit request requires an answer',t=>{
  const f=fixture(t);
  f.cli('subscribe',{topics:['facts']},f.receiver);
  const topic=f.cli('send_message',{topic:'facts',body:'Topic fact',reasoning:'Inform subscribers'},f.sender);
  const broadcast=f.cli('notify_all',{body:'Announcement',reasoning:'Inform peers'},f.sender);
  const notice=f.cli('send_message',{to:f.receiver,body:'Direct FYI',reasoning:'Context'},f.sender);
  const expected=new Map([[f.request.id,true],[topic.id,false],[broadcast.id,false],[notice.id,false]]);
  const inbox=f.cli('inbox',{},f.receiver).items;
  for(const item of inbox){assert.equal(typeof item.replyRequired,'boolean');assert.equal(item.replyRequired,expected.get(item.id));}
  f.cli('attach',{transport:'raw'},f.receiver);
  const hook=f.cli('hook',{format:'json'},f.receiver);
  const offered=JSON.parse(hook.context.slice(hook.context.indexOf('\n')+1));
  for(const item of offered){assert.equal(typeof item.data.replyRequired,'boolean');assert.equal(item.data.replyRequired,expected.get(item.data.messageId));}
  for(const [id,required] of expected){
    const entity=JSON.parse(execFileSync(binary,['fetch',JSON.stringify({type:'message',where:{messageId:id}}),'--workspace',f.workspace,'--database',f.database,'--session',f.receiver],{encoding:'utf8'}));
    assert.equal(typeof entity.items[0].data.replyRequired,'boolean');assert.equal(entity.items[0].data.replyRequired,required);
  }
});

test('SQL cannot bypass required answers, reply policy or immutable sender intent',t=>{
  const f=fixture(t);
  const partial=f.send({to:f.sender,body:'Still reviewing',reasoning:'Partial progress'});
  const before=only(f.db.prepare('SELECT count(*) n FROM records').get()).n;
  assert.throws(()=>f.db.prepare('UPDATE deliveries SET acknowledgedAt=? WHERE message=? AND recipient=?').run(Date.now(),f.request.id,f.receiver));
  assert.equal(f.stamp(),null);assert.equal(only(f.db.prepare('SELECT count(*) n FROM records').get()).n,before);
  const notice=f.cli('send_message',{to:f.receiver,body:'FYI only',reasoning:'Inform'},f.sender);
  const count=only(f.db.prepare('SELECT count(*) n FROM messages').get()).n;
  const audits=only(f.db.prepare('SELECT count(*) n FROM records').get()).n;
  const insert=f.db.prepare("INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,replyTo,replyRequired,kind) VALUES(?,?,?,?,?,?,?,?,'answer')");
  assert.throws(()=>insert.run(f.receiver,f.sender,'Reply to FYI','sql-notice',Date.now()+60000,'Bypass',notice.id,0));
  assert.throws(()=>insert.run(f.receiver,f.sender,'Reply requesting reply','sql-loop',Date.now()+60000,'Bypass',f.request.id,1));
  assert.throws(()=>f.db.prepare('UPDATE messages SET replyRequired=0 WHERE id=?').run(f.request.id));
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n,count);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM records').get()).n,audits);
  assert.equal(only(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(notice.id)).acknowledgedAt,null);
  assert.equal(f.cli('inbox',{message:partial.id},f.sender).items[0].replyRequired,false);
});


test('broadcast cannot create a correlated reply through CLI or MCP', t => {
  const f=fixture(t), before=only(f.db.prepare('SELECT count(*) n FROM messages').get()).n;
  const input={replyTo:f.request.id,body:'Bypass answer',reasoning:'Verify complete-only replies'};
  assert.throws(()=>f.cli('notify_all',input,f.receiver),/replyTo|complete/);
  const frames=[{id:1,method:'tools/list'},{id:2,method:'tools/call',params:{name:'notify_all',arguments:input}}].map(x=>JSON.stringify({jsonrpc:'2.0',...x})).join('\n')+'\n';
  const rows=execFileSync(binary,['mcp','--tools','notify_all','--workspace',f.workspace,'--database',f.database,'--session',f.receiver],{input:frames,encoding:'utf8',timeout:10000}).trim().split('\n').map((line): JsonRecord=>JSON.parse(line));
  assert.equal(rows[0].result.tools[0].inputSchema.properties.replyTo,undefined);
  assert.equal(rows[1].result.isError,true);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n,before);
  assert.equal(f.stamp(),null);
  assert.equal(f.complete(f.reply).completed,true);
});
