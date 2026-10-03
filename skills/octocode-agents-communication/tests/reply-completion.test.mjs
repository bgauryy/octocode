import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from './helpers.mjs';
import { createInterface } from 'node:readline';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, tempWorkspace, jsonCall } from './helpers.mjs';

function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-completion-', { real: true });
  const database = join(workspace, 'mail.sqlite');
  const cli = jsonCall(binary, workspace, database, { stdio: 'pipe', timeout: 10000 });
  const sender = cli('join', { name: 'requester', vendor: 'raw' }).id;
  const receiver = cli('join', { name: 'worker', vendor: 'raw' }).id;
  const other = cli('join', { name: 'other', vendor: 'raw' }).id;
  const send = (input, session = receiver) => cli('send_message', input, session);
  const complete = (input, session = receiver) => cli('complete', input, session);
  const request = cli('send_message', { to: receiver, body: 'Review the change', reasoning: 'Need review before release', key: 'request' }, sender);
  const db = new DatabaseSync(database);
  t.after(() => db.close());
  const stamp = () => db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=? AND recipient=?').get(request.id, receiver).acknowledgedAt;
  const reply = { message: request.id, reply: 'Reviewed; checks pass', reasoning: 'Return completed review' };
  return { cli, send, complete, db, sender, receiver, other, request, reply, stamp, workspace, database };
}

test('complete reply commits final response and handling atomically; identical retry is idempotent', t => {
  const f = fixture(t), sent = f.complete(f.reply);
  assert.deepEqual(sent, {completed:true,count:1,id:sent.id,recipients:1});
  const at=f.stamp();assert.equal(typeof at,'number');
  assert.deepEqual(f.complete(f.reply),sent);assert.equal(f.stamp(),at);
  assert.deepEqual(f.complete({message:f.request.id}),{completed:true,count:1});
  assert.throws(()=>f.complete({...f.reply,reply:'Different result'}),/Final reply already stored with different content; use send_message for new work/);
  assert.throws(()=>f.complete({...f.reply,reasoning:'Different reason'}));
  assert.equal(f.db.prepare('SELECT count(*) n FROM messages WHERE sender=?').get(f.receiver).n,1);
  assert.equal(f.db.prepare("SELECT count(*) n FROM audit WHERE kind='delivery.acknowledged'").get().n,1);
  const response=f.db.prepare('SELECT * FROM messages WHERE id=?').get(sent.id);
  assert.equal(response.key,`complete:${f.request.id}`);assert.equal(response.target,f.sender);
  assert.equal(response.replyTo,f.request.id);assert.equal(response.wake,'action');
  assert.equal(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(sent.id).acknowledgedAt,null,'Recipient still must handle the reply');
});

test('progress FYI leaves request pending; completion uses default purpose and no follow-up reply after silent completion', t => {
  const f=fixture(t);
  f.send({to:f.sender,replyRequired:false,body:'Review ongoing',reasoning:'Progress update'});
  assert.equal(f.stamp(),null);
  const sent=f.complete({message:f.request.id,reply:'Done'});
  assert.equal(f.db.prepare('SELECT reasoning FROM messages WHERE id=?').get(sent.id).reasoning,'Complete received message');
  assert.deepEqual(f.complete({message:f.request.id,reply:'Done'}),sent);
  const second=f.cli('send_message',{to:f.receiver,body:'FYI',replyRequired:false,reasoning:'Context'},f.sender);
  assert.deepEqual(f.complete({message:second.id}),{completed:true,count:1});
  assert.deepEqual(f.complete({message:second.id}),{completed:true,count:1});
  assert.throws(()=>f.complete({message:second.id,reply:'Unrequested extra final response'}));
});

test('complete rejects missing/foreign identities, old API and unsupported routing or reply knobs', t => {
  const f=fixture(t);
  for(const input of [
    {}, {message:999999}, {message:f.request.id,reply:''}, {message:f.request.id,reasoning:'No reply'},
    {...f.reply,to:f.other}, {...f.reply,replyTo:f.request.id}, {...f.reply,body:'old'},
    {...f.reply,key:'caller-key'}, {...f.reply,wake:'passive'}, {...f.reply,ttlMs:1000},
    {...f.reply,ackReply:true}, {...f.reply,session:f.other}, {messages:[f.request.id],reply:'Invalid batch reply'},
    {messages:[f.request.id],reasoning:'Invalid batch reason'}, {message:f.request.id,messages:[f.request.id]},
  ]) assert.throws(()=>f.complete(input),JSON.stringify(input));
  assert.throws(()=>f.complete(f.reply,f.sender));
  assert.throws(()=>f.complete(f.reply,f.other));
  assert.throws(()=>f.cli('ack',{message:f.request.id},f.receiver));
  assert.throws(()=>f.send({replyTo:f.request.id,body:'old',reasoning:'old',ackReply:true}));
  assert.throws(()=>f.send({replyTo:f.request.id,body:'collision',reasoning:'collision',key:`complete:${f.request.id}`}));
  assert.equal(f.stamp(),null);assert.equal(f.db.prepare('SELECT count(*) n FROM messages').get().n,1);
  f.db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(f.receiver);
  assert.throws(()=>f.complete(f.reply));assert.equal(f.stamp(),null);
});

test('injected reply or completion failure rolls back all writes and audit rows', async t => {
  const f=fixture(t), initial=f.db.prepare('SELECT count(*) n FROM audit').get().n;
  // Validate the original schema before fault injection; new CLI processes
  // correctly reject extra triggers during database startup validation.
  const child=spawn(binary,['mcp','--tools','complete','--workspace',f.workspace,'--database',f.database,'--session',f.receiver]);
  const lines=createInterface({input:child.stdout});
  let sequence=0;
  t.after(()=>{lines.close();child.kill('SIGKILL');});
  const request=(method,params)=>new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>{lines.off('line',onLine);reject(new Error('MCP response timeout'));},10000);
    const onLine=line=>{clearTimeout(timer);resolve(JSON.parse(line));};
    lines.once('line',onLine);
    child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:++sequence,method,params})+'\n');
  });
  const initialized=await request('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'rollback-test',version:'1'}});
  assert.ok(initialized.result);
  const complete=()=>request('tools/call',{name:'complete',arguments:f.reply});
  f.db.exec("CREATE TRIGGER fail_reply BEFORE INSERT ON messages WHEN NEW.key LIKE 'complete:%' BEGIN SELECT RAISE(ABORT,'Injected reply failure'); END");
  const replyFailure=await complete();assert.equal(replyFailure.result.isError,true);
  assert.match(replyFailure.result.content[0].text,/Injected reply failure/);
  assert.equal(f.stamp(),null);assert.equal(f.db.prepare('SELECT count(*) n FROM messages').get().n,1);
  assert.equal(f.db.prepare('SELECT count(*) n FROM audit').get().n,initial);
  f.db.exec('DROP TRIGGER fail_reply');
  f.db.exec("CREATE TRIGGER fail_completion BEFORE UPDATE OF acknowledgedAt ON deliveries BEGIN SELECT RAISE(ABORT,'Injected completion failure'); END");
  const handlingFailure=await complete();assert.equal(handlingFailure.result.isError,true);
  assert.match(handlingFailure.result.content[0].text,/Injected completion failure/);
  assert.equal(f.stamp(),null);assert.equal(f.db.prepare('SELECT count(*) n FROM messages').get().n,1);
  assert.equal(f.db.prepare('SELECT count(*) n FROM audit').get().n,initial);
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
  const rows=execFileSync(binary,['mcp','--tools','send_message,complete','--workspace',f.workspace,'--database',f.database,'--session',f.receiver],{input:frames,encoding:'utf8',timeout:10000}).trim().split('\n').map(JSON.parse);
  assert.deepEqual(rows[0].result.tools.map(x=>x.name),['send_message','complete']);
  assert.equal(rows[0].result.tools[0].inputSchema.properties.ackReply,undefined);
  assert.equal(rows[0].result.tools[0].inputSchema.properties.replyRequired.type,'boolean');
  for(const row of rows.slice(1,3))assert.ok(row.error||row.result?.isError);
  assert.equal(JSON.parse(rows[3].result.content[0].text).completed,true);assert.equal(typeof f.stamp(),'number');
});

test('required answers cannot be omitted individually or through a mixed completion batch',t=>{
  const f=fixture(t);
  const notice=f.cli('send_message',{to:f.receiver,body:'FYI',reasoning:'Context only',replyRequired:false},f.sender);
  assert.throws(()=>f.complete({message:f.request.id}));
  assert.throws(()=>f.complete({messages:[notice.id,f.request.id]}));
  assert.equal(f.stamp(),null);
  assert.equal(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(notice.id).acknowledgedAt,null);
  assert.equal(f.db.prepare("SELECT count(*) n FROM audit WHERE kind='delivery.acknowledged'").get().n,0);
  f.send({to:f.sender,replyRequired:false,body:'Still reviewing',reasoning:'Partial work'});
  assert.throws(()=>f.complete({message:f.request.id}),'Partial reply is not a final answer');
  f.complete(f.reply);
  assert.deepEqual(f.complete({messages:[notice.id,f.request.id]}),{completed:true,count:2});
});

test('answers and notices cannot start reply loops; new work requires a new root request',t=>{
  const f=fixture(t),answer=f.complete(f.reply);
  assert.throws(()=>f.cli('send_message',{replyTo:answer.id,body:'Thanks; more details?',reasoning:'Reply loop'},f.sender));
  assert.throws(()=>f.cli('complete',{message:answer.id,reply:'Thanks'},f.sender));
  assert.deepEqual(f.cli('complete',{message:answer.id},f.sender),{completed:true,count:1});
  const notice=f.cli('send_message',{to:f.receiver,body:'Notice',reasoning:'FYI',replyRequired:false},f.sender);
  assert.throws(()=>f.send({replyTo:notice.id,body:'Unasked answer',reasoning:'Loop'}));
  assert.throws(()=>f.complete({message:notice.id,reply:'Unasked answer'}));
  assert.deepEqual(f.complete({message:notice.id}),{completed:true,count:1});
  const next=f.cli('send_message',{to:f.receiver,body:'Please investigate a new issue',reasoning:'New work'},f.sender);
  assert.equal(f.complete({message:next.id,reply:'Investigated'}).completed,true);
});

test('reply policy is immutable across keyed retries and replies cannot demand another answer',t=>{
  const f=fixture(t),input={to:f.receiver,body:'Required review',reasoning:'Need evidence',key:'policy'};
  const first=f.cli('send_message',input,f.sender);
  assert.equal(f.cli('send_message',{...input,replyRequired:true},f.sender).id,first.id);
  assert.throws(()=>f.cli('send_message',{...input,replyRequired:false},f.sender));
  const notice={...input,body:'FYI',key:'notice',replyRequired:false};
  assert.equal(f.cli('send_message',notice,f.sender).id,f.cli('send_message',notice,f.sender).id);
  assert.throws(()=>f.cli('send_message',{...notice,replyRequired:true},f.sender));
  assert.throws(()=>f.send({replyTo:first.id,body:'Partial',reasoning:'Progress',replyRequired:true}));
  for(const value of ['true',0,null])assert.throws(()=>f.cli('send_message',{...input,key:`invalid-${value}`,replyRequired:value},f.sender));
  const partial=f.send({to:f.sender,body:'Partial',reasoning:'Progress',replyRequired:false});
  assert.equal(f.cli('inbox',{message:partial.id},f.sender).items[0].replyRequired,false);
});

test('reply policy is explicit boolean in inbox, hooks and entity reads with route-specific defaults',t=>{
  const f=fixture(t);
  f.cli('subscribe',{topics:['facts']},f.receiver);
  const topic=f.cli('send_message',{topic:'facts',body:'Topic fact',reasoning:'Inform subscribers'},f.sender);
  const broadcast=f.cli('notify_all',{body:'Announcement',reasoning:'Inform peers'},f.sender);
  const notice=f.cli('send_message',{to:f.receiver,body:'Direct FYI',reasoning:'Context',replyRequired:false},f.sender);
  const expected=new Map([[f.request.id,true],[topic.id,false],[broadcast.id,false],[notice.id,false]]);
  const inbox=f.cli('inbox',{},f.receiver).items;
  for(const item of inbox){assert.equal(typeof item.replyRequired,'boolean');assert.equal(item.replyRequired,expected.get(item.id));}
  f.cli('attach',{transport:'raw'},f.receiver);
  const hook=f.cli('hook',{format:'json'},f.receiver);
  const offered=JSON.parse(hook.context.slice(hook.context.indexOf('\n')+1));
  for(const item of offered){assert.equal(typeof item.replyRequired,'boolean');assert.equal(item.replyRequired,expected.get(item.id));}
  for(const [id,required] of expected){
    const entity=JSON.parse(execFileSync(binary,['entity','get','message',String(id),'--workspace',f.workspace,'--database',f.database,'--session',f.receiver],{encoding:'utf8'}));
    assert.equal(typeof entity.replyRequired,'boolean');assert.equal(entity.replyRequired,required);
  }
});

test('SQL cannot bypass required answers, reply policy or immutable sender intent',t=>{
  const f=fixture(t);
  const partial=f.send({to:f.sender,replyRequired:false,body:'Still reviewing',reasoning:'Partial progress'});
  const before=f.db.prepare('SELECT count(*) n FROM audit').get().n;
  assert.throws(()=>f.db.prepare('UPDATE deliveries SET acknowledgedAt=? WHERE message=? AND recipient=?').run(Date.now(),f.request.id,f.receiver));
  assert.equal(f.stamp(),null);assert.equal(f.db.prepare('SELECT count(*) n FROM audit').get().n,before);
  const notice=f.cli('send_message',{to:f.receiver,body:'FYI only',reasoning:'Inform',replyRequired:false},f.sender);
  const count=f.db.prepare('SELECT count(*) n FROM messages').get().n;
  const audits=f.db.prepare('SELECT count(*) n FROM audit').get().n;
  const insert=f.db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,replyTo,replyRequired) VALUES(?,?,?,?,?,?,?,?)');
  assert.throws(()=>insert.run(f.receiver,f.sender,'Reply to FYI','sql-notice',Date.now()+60000,'Bypass',notice.id,0));
  assert.throws(()=>insert.run(f.receiver,f.sender,'Reply requesting reply','sql-loop',Date.now()+60000,'Bypass',f.request.id,1));
  assert.throws(()=>f.db.prepare('UPDATE messages SET replyRequired=0 WHERE id=?').run(f.request.id));
  assert.equal(f.db.prepare('SELECT count(*) n FROM messages').get().n,count);
  assert.equal(f.db.prepare('SELECT count(*) n FROM audit').get().n,audits);
  assert.equal(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(notice.id).acknowledgedAt,null);
  assert.equal(f.cli('inbox',{message:partial.id},f.sender).items[0].replyRequired,false);
});


test('broadcast cannot create a correlated reply through CLI or MCP', t => {
  const f=fixture(t), before=f.db.prepare('SELECT count(*) n FROM messages').get().n;
  const input={replyTo:f.request.id,body:'Bypass answer',reasoning:'Verify complete-only replies'};
  assert.throws(()=>f.cli('notify_all',input,f.receiver),/replyTo|complete/);
  const frames=[{id:1,method:'tools/list'},{id:2,method:'tools/call',params:{name:'notify_all',arguments:input}}].map(x=>JSON.stringify({jsonrpc:'2.0',...x})).join('\n')+'\n';
  const rows=execFileSync(binary,['mcp','--tools','notify_all','--workspace',f.workspace,'--database',f.database,'--session',f.receiver],{input:frames,encoding:'utf8',timeout:10000}).trim().split('\n').map(JSON.parse);
  assert.equal(rows[0].result.tools[0].inputSchema.properties.replyTo,undefined);
  assert.equal(rows[1].result.isError,true);
  assert.equal(f.db.prepare('SELECT count(*) n FROM messages').get().n,before);
  assert.equal(f.stamp(),null);
  assert.equal(f.complete(f.reply).completed,true);
});
