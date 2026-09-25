import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { cpSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { randomUUID, createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';

// Acceptance is checked against DB rows and actual tool receipts, never a model's DONE claim.
const python=process.env.COMMUNICATION_PYTHON, piModel=process.env.COMMUNICATION_PI_MODEL;
const vendors=(process.env.COMMUNICATION_VENDORS||'codex,claude,pi').split(',');
if(new Set(vendors).size!==vendors.length||vendors.some(v=>!['codex','claude','pi'].includes(v)))throw Error('COMMUNICATION_VENDORS must name unique codex,claude,pi vendors');
if(!python||(vendors.includes('pi')&&!piModel))throw Error('Set COMMUNICATION_PYTHON and, for Pi, an exact COMMUNICATION_PI_MODEL provider/model');
if(process.platform==='win32')throw Error('This live harness currently validates the POSIX skill launcher only');
const perVendor=Number(process.env.COMMUNICATION_WORKERS_PER_VENDOR||3);
if(!Number.isInteger(perVendor)||perVendor<1||perVendor>3)throw Error('COMMUNICATION_WORKERS_PER_VENDOR must be 1, 2 or 3');
const workerNames=vendors.flatMap(v=>Array.from({length:perVendor},(_,i)=>`mesh-${v}-${i+1}`));
const workerCount=workerNames.length,pairCount=workerCount*(workerCount-1),broadcastRecipients=workerCount+1;
const directory=mkdtempSync(join(tmpdir(),'communication-mesh-'));
const skill=join(directory,'skill');
cpSync(fileURLToPath(new URL('../skills/octocode-agents-communication',import.meta.url)),skill,{recursive:true});
const cli=join(skill,'scripts/agents-communication'),database=join(directory,'v1.sqlite');
const nonce=randomUUID(),events=[],workers=[],phases=[];
const started=Date.now(),deadline=started+600000;
const invoke=(command,input={},session)=>JSON.parse(execFileSync(cli,[command,JSON.stringify(input),'--workspace',directory,'--database',database,...(session?['--session',session]:[])],{encoding:'utf8'}));
const py=(op,data)=>JSON.parse(execFileSync(python,[join(skill,'scripts/sqlite_agent.py'),database,directory,op,JSON.stringify(data)],{encoding:'utf8',env:{...process.env,PATH:''}}));
const controller=invoke('join',{name:'mesh-controller',vendor:'test'});
const generic=py('join',{name:'mesh-db-only',vendor:'python-sqlite'});
const db=new DatabaseSync(database);
const held=invoke('lock',{path:'mesh',kind:'tree',ttlMs:600000},controller.id).lease;
let heldReleased=false,report,heartbeatError;
const heartbeat=setInterval(()=>{try{invoke('heartbeat',{},controller.id);py('heartbeat',{session:generic.id});}catch(e){heartbeatError=e;}},10000);
const rows=()=>db.prepare('SELECT * FROM messages ORDER BY id').all();
const find=(sender,target,body)=>db.prepare('SELECT * FROM messages WHERE sender=? AND target=? AND body=?').get(sender,target,body);
const state=()=>({directory,database,cli,nonce,controller,generic,workers:workers.map(({name,vendor,session,pid,exited})=>({name,vendor,session,pid,exited}))});
const saveState=()=>writeFileSync(join(directory,'state.json'),JSON.stringify(state(),null,2));
function objects(value){
 if(typeof value==='string'){try{return objects(JSON.parse(value));}catch{return [];}}
 if(!value||typeof value!=='object')return [];
 return [value,...Object.values(value).flatMap(objects)];
}
const receipts=(w,tool,predicate)=>events.filter(e=>e.worker===w.name&&e.type==='tool-result'&&e.tool?.endsWith(tool)&&objects(e.result).some(predicate));
function service(){
 for(const m of invoke('inbox',{},controller.id).items)invoke('ack',{message:m.id},controller.id);
 for(const m of py('inbox',{session:generic.id}).items){
  if(m.body.startsWith(`BROADCAST ${nonce} `))py('send_message',{session:generic.id,to:m.sender,body:`DB_ACK_BROADCAST ${nonce}`,key:`db-ack-${m.id}`});
  py('ack',{session:generic.id,message:m.id});
 }
}
async function until(predicate,label){
 while(!predicate()){
  if(heartbeatError)throw heartbeatError;
  const failed=workers.find(w=>w.exited||w.protocolError);if(failed)throw Error(`${failed.name}: ${failed.protocolError||failed.stderr.slice(-2000)}`);
  if(Date.now()>deadline)throw Error(`Timed out: ${label}`);
  service();await new Promise(r=>setTimeout(r,300));
 }
 phases.push({name:label,elapsedSeconds:(Date.now()-started)/1000});console.log(JSON.stringify(phases.at(-1)));
}
function start(vendor,index,model){
 const name=`mesh-${vendor}-${index}`;
 const prompt=`You are ${name}, one of ${workerCount} real workers testing database-backed communication. Use only the supplied bound communication tools, not another vendor's private agent manager. Do not create agents or edit files. Your controller is ${controller.id}. Test token: ${nonce}.
Use short plain-text message bodies exactly as specified below (no JSON inside body). Reuse stable retry keys. Handle each incoming message ID once, then ack it using the ack tool. Reply only where requested; never reply to an ACK. Finish turns promptly so the proxy can deliver more inbox data. No polling loops, sleeps, or progress messages.
First call peers, send body READY ${nonce} ${name} directly to the controller, and finish your turn.
Only handle messages with this token. Controller commands:
- MESH: call peers; find these ${workerCount} names: ${workerNames.join(', ')}. Send DIRECT ${nonce} ${name} once to each of the OTHER ${workerCount-1} worker session IDs, using key direct-${nonce}-RECIPIENT_ID. Do not send DIRECT to the controller or DB-only agent.
- NOTIFY: call notify_all once with body BROADCAST ${nonce} ${name} and key broadcast-${nonce}. This snapshots all active peers, including controller/DB-only; no subscription needed.
- BLOCK: attempt lock on mesh/report.md with ttlMs 120000. Expected parent-tree conflict: if denied send BLOCKED ${nonce} ${name} to controller. Never report a conflict unless the tool actually denied it. If unexpectedly acquired, unlock and report UNEXPECTED to controller.
- LEASE: acquire mesh/report.md (ttlMs 120000), renew its returned lease ID, then unlock it, each sequentially checking success. Send LEASED ${nonce} ${name} to controller only after all three succeeded. Never substitute narration for calls.
Incoming peer messages:
- DIRECT TOKEN NAME: send ACK_DIRECT ${nonce} ${name} directly to the envelope's sender ID, key ack-direct-INCOMING_MESSAGE_ID.
- BROADCAST TOKEN NAME: send ACK_BROADCAST ${nonce} ${name} directly to sender, key ack-broadcast-INCOMING_MESSAGE_ID.
- DB_DIRECT TOKEN or DB_ALL TOKEN: send DB_RECEIVED ${nonce} ${name} directly to sender, key db-received-INCOMING_MESSAGE_ID.
- ACK_DIRECT, ACK_BROADCAST, DB_ACK_BROADCAST: ack only, no response.
Always ack every handled incoming ID, including controller commands, and finish the turn. The controller checks every directed edge and owns the completion barrier; no final/DONE message is needed.`;
 const child=spawn(cli,['run','--vendor',vendor,'--model',model,'--name',name,'--workspace',directory,'--database',database,'--duration-ms','600000','--trace','--prompt',prompt],{stdio:['ignore','pipe','pipe']});
 const w={name,vendor,model,child,exited:false,stderr:''};workers.push(w);
 child.stderr.on('data',d=>w.stderr+=d);child.on('error',e=>{w.exited=true;w.stderr+=e.message;});child.on('exit',code=>{w.exited=true;w.code=code;});
 createInterface({input:child.stdout}).on('line',line=>{
  try{const event={at:Date.now(),worker:name,...JSON.parse(line)};events.push(event);
   if(event.type==='ready'){w.session=event.session;w.pid=event.pid;saveState();console.log(JSON.stringify({ready:name}));}
  }catch(e){w.protocolError=e.message;}
 });
}
function command(w,name){return invoke('send_message',{to:w.session,body:`${name} ${nonce}`,key:`${name}-${w.name}`},controller.id);}
const dbReply=(w,message)=>db.prepare('SELECT id FROM messages WHERE sender=? AND target=? AND key=? AND (body=? OR body=?)').get(w.session,generic.id,`db-received-${message}`,`DB_RECEIVED ${nonce} ${w.name}`,`DB_RECEIVED ${nonce}`);
const directed=(kind,reply)=>workers.every(a=>workers.filter(b=>b!==a).every(b=>find(a.session,b.session,`${kind} ${nonce} ${a.name}`)&&find(b.session,a.session,`${reply} ${nonce} ${b.name}`)));
try{
 for(const [vendor,model] of [['codex','gpt-6-luna'],['claude','haiku'],['pi',piModel]].filter(([v])=>vendors.includes(v)))for(let index=1;index<=perVendor;index++)start(vendor,index,model);
 saveState();console.log(JSON.stringify({directory,nonce}));
 await until(()=>workers.every(w=>w.session&&find(w.session,controller.id,`READY ${nonce} ${w.name}`)),`${workerCount} ready workers`);
 for(const w of workers)command(w,'MESH');
 await until(()=>directed('DIRECT','ACK_DIRECT'),`${pairCount} directed messages and ${pairCount} peer replies`);
 for(const w of workers)command(w,'NOTIFY');
 await until(()=>workers.every(a=>{
  const broadcast=find(a.session,'*',`BROADCAST ${nonce} ${a.name}`);
  return broadcast&&workers.filter(b=>b!==a).every(b=>find(b.session,a.session,`ACK_BROADCAST ${nonce} ${b.name}`))&&find(generic.id,a.session,`DB_ACK_BROADCAST ${nonce}`);
 }),`${workerCount} broadcasts reach all peers and DB-only client`);
 assert.equal(db.prepare('SELECT count(*) n FROM subscriptions').get().n,0);
 for(const w of workers){
  const m=find(w.session,'*',`BROADCAST ${nonce} ${w.name}`);
  assert.equal(db.prepare('SELECT count(*) n FROM deliveries WHERE message=?').get(m.id).n,broadcastRecipients);
  assert.ok(receipts(w,'notify_all',v=>v.id===m.id&&v.recipients===broadcastRecipients).length,`${w.name}: missing actual notify_all receipt`);
 }
 for(const w of workers)py('send_message',{session:generic.id,to:w.session,body:`DB_DIRECT ${nonce}`,key:`db-direct-${w.name}`});
 await until(()=>workers.every(w=>dbReply(w,find(generic.id,w.session,`DB_DIRECT ${nonce}`).id)),`DB-only direct messages reach all ${workerCount} workers`);
 const broadcastArgs={session:generic.id,body:`DB_ALL ${nonce}`,key:'db-all'};
 const sent=py('notify_all',broadcastArgs);assert.deepEqual(py('notify_all',broadcastArgs),sent);assert.equal(sent.recipients,broadcastRecipients);
 await until(()=>workers.every(w=>dbReply(w,sent.id)),'DB-only notify_all and retry snapshot');
 for(const w of workers)command(w,'BLOCK');
 await until(()=>workers.every(w=>receipts(w,'lock',v=>v.ok===false&&v.conflict?.owner===controller.id).length),`${workerCount} actual parent-tree lock conflicts`);
 for(const w of workers)assert.ok(receipts(w,'lock',v=>v.ok===false&&v.conflict?.owner===controller.id).length);
 invoke('unlock',{lease:held.id},controller.id);heldReleased=true;
 for(const w of workers){
  command(w,'LEASE');
  await until(()=>receipts(w,'lock',v=>v.ok===true&&v.lease?.owner===w.session).length&&receipts(w,'renew',v=>v.renewed===true).length&&receipts(w,'unlock',v=>v.released===true).length,`${w.name} acquire-renew-release`);
  for(const [tool,predicate] of [['lock',v=>v.ok===true&&v.lease?.owner===w.session],['renew',v=>v.renewed===true],['unlock',v=>v.released===true]])assert.ok(receipts(w,tool,predicate).length,`${w.name}: missing ${tool}`);
 }
 await until(()=>db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n===0,'every delivery acknowledged');
 assert.equal(db.prepare('SELECT count(*) n FROM leases').get().n,0);
 const all=rows();
 assert.equal(all.filter(m=>m.body.startsWith(`DIRECT ${nonce} `)).length,pairCount);
 assert.equal(all.filter(m=>m.body.startsWith(`ACK_DIRECT ${nonce} `)).length,pairCount);
 assert.equal(all.filter(m=>m.body.startsWith(`BROADCAST ${nonce} `)).length,workerCount);
 const matrix=workers.map(a=>({sender:a.name,recipients:workers.filter(b=>b!==a).map(b=>({name:b.name,message:find(a.session,b.session,`DIRECT ${nonce} ${a.name}`).id,reply:find(b.session,a.session,`ACK_DIRECT ${nonce} ${b.name}`).id}))}));
 for(const a of workers)for(const b of workers.filter(w=>w!==a)){
  const m=find(a.session,b.session,`DIRECT ${nonce} ${a.name}`);
  assert.ok(receipts(a,'send_message',v=>v.id===m.id).length,`${a.name}: DB row lacks vendor tool receipt`);
 }
 report={passed:true,nonce,models:workers.map(({name,vendor,model})=>({name,vendor,model})),skillSha256:createHash('sha256').update(readFileSync(join(skill,'SKILL.md'))).digest('hex'),phases,matrix,
  checks:[`all ${pairCount} directed worker pairs with replies`,'same-vendor pairs included',`${workerCount} notify_all calls with ${broadcastRecipients} recipients each and actual tool receipts`,'zero topic subscriptions required',`Python-only direct and broadcast messages reach all ${workerCount} workers`,'broadcast retry preserves receipt and snapshot',`${workerCount} tree conflicts and ${workerCount} acquire/renew/unlock lifecycles`,'all deliveries acknowledged and leases released'],
  metrics:{messages:all.length,deliveries:db.prepare('SELECT count(*) n FROM deliveries').get().n,directedPairs:pairCount,workerBroadcasts:workerCount,workerBroadcastDeliveries:workerCount*broadcastRecipients,dbOnlyBroadcastRecipients:broadcastRecipients,unexpectedModelReports:all.filter(m=>m.body.startsWith(`UNEXPECTED ${nonce} `)).length,supervisorInterventions:all.filter(m=>m.sender===controller.id&&m.body.startsWith(`CHECK ${nonce}:`)).length},messages:all,events};
}catch(error){process.exitCode=1;writeFileSync(join(directory,'failure.json'),JSON.stringify({error:error.message,state:state(),phases,messages:rows(),events,workers:workers.map(({name,stderr,code})=>({name,stderr,code}))},null,2),{mode:0o600});console.error(JSON.stringify({error:error.message,evidence:directory}));}
finally{
 clearInterval(heartbeat);
 await Promise.all(workers.map(w=>new Promise(resolve=>{if(w.exited)return resolve();const timer=setTimeout(()=>w.child.kill('SIGKILL'),5000);w.child.once('exit',()=>{clearTimeout(timer);resolve();});w.child.kill('SIGTERM');})));
 if(!heldReleased)invoke('unlock',{lease:held.id},controller.id);
 invoke('leave',{},controller.id);py('leave',{session:generic.id});
 if(report){
  for(const w of workers)assert.throws(()=>process.kill(w.pid,0),e=>e.code==='ESRCH');
  assert.equal(db.prepare('SELECT count(*) n FROM sessions WHERE expiresAt>?').get(Date.now()).n,0);
  report.checks.push('all owned vendor processes stopped and test sessions expired');
  writeFileSync(join(directory,'result.json'),JSON.stringify(report,null,2),{mode:0o600});
  console.log(JSON.stringify({passed:true,result:join(directory,'result.json'),metrics:report.metrics}));
 }
 db.close();saveState();
}
