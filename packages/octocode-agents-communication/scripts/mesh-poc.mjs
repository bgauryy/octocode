import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { cpSync, createWriteStream, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { randomUUID, createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';

// Acceptance is checked against DB rows and actual tool receipts, never a model's DONE claim.
const probeInput=(command,input)=>['send_message','notify_all','lock','lock_many'].includes(command)?{...( ['send_message','notify_all'].includes(command)?{wake:'action'}:{}),reasoning:`Validate ${command} interoperability in this isolated communication exercise`,...input}:input;

const python=process.env.COMMUNICATION_PYTHON, piModel=process.env.COMMUNICATION_PI_MODEL;
const vendors=(process.env.COMMUNICATION_VENDORS||'codex,claude,pi').split(',');
if(new Set(vendors).size!==vendors.length||vendors.some(v=>!['codex','claude','pi'].includes(v)))throw Error('COMMUNICATION_VENDORS must name unique codex,claude,pi vendors');
if(!python||(vendors.includes('pi')&&!piModel))throw Error('Set COMMUNICATION_PYTHON and, for Pi, an exact COMMUNICATION_PI_MODEL provider/model');
if(process.platform==='win32')throw Error('This live harness currently validates the POSIX skill launcher only');
const perVendor=Number(process.env.COMMUNICATION_WORKERS_PER_VENDOR||2);
if(!Number.isInteger(perVendor)||perVendor<1||perVendor>3)throw Error('COMMUNICATION_WORKERS_PER_VENDOR must be 1, 2 or 3');
const workerNames=vendors.flatMap(v=>Array.from({length:perVendor},(_,i)=>`mesh-${v}-${i+1}`));
const expectedTools=JSON.parse(readFileSync(new URL('../rust/catalog.json',import.meta.url),'utf8')).tools.map(tool=>tool.name);
const workerCount=workerNames.length,pairCount=workerCount*(workerCount-1),broadcastRecipients=workerCount+1;
const directory=mkdtempSync(join(tmpdir(),'communication-mesh-'));
const suppliedSkill=process.env.COMMUNICATION_SKILL_PATH;
const skill=suppliedSkill||join(directory,'skill');
if(!suppliedSkill)cpSync(fileURLToPath(new URL('../skills/octocode-agents-communication',import.meta.url)),skill,{recursive:true});
const cli=join(skill,'scripts/agents-communication'),database=join(directory,'v1.sqlite');
const nonce=randomUUID(),events=[],workers=[],phases=[];
let traceError;
const trace=createWriteStream(join(directory,'events.jsonl'),{mode:0o600});
trace.on('error',error=>{traceError=error;});
const started=Date.now(),deadline=started+600000;
const invoke=(command,input={},session)=>JSON.parse(execFileSync(cli,[command,JSON.stringify(probeInput(command,input)),'--workspace',directory,'--database',database,...(session?['--session',session]:[])],{encoding:'utf8',timeout:120000,killSignal:'SIGTERM'}));
const py=(op,data)=>JSON.parse(execFileSync(python,[join(skill,'scripts/sqlite_agent.py'),database,directory,op,JSON.stringify(probeInput(op,data))],{encoding:'utf8',env:{...process.env,PATH:''}}));
const controller=invoke('join',{name:'mesh-controller',vendor:'test'});
const generic=py('join',{name:'mesh-db-only',vendor:'python-sqlite'});
const db=new DatabaseSync(database);
const held=invoke('lock',{path:'mesh',kind:'tree',ttlMs:600000},controller.id).lease;
let heldReleased=false,report,heartbeatError;
const documentName=`handoff-${nonce}.md`;
const documentPrefix='Large evidence stays in this shared document. Read only the requested section.\n'.repeat(600);
const documentMarker=`DOC_PROOF ${nonce}`;
const documentOffset=Buffer.byteLength(documentPrefix);
const sharedDocument=invoke('share_document',{name:documentName,content:documentPrefix+documentMarker+'\n'},controller.id);
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
  if(traceError)throw traceError;
  for(const w of workers){
   if(!w.session||w.exited||Date.now()-(w.lastEventAt||started)<60000||Date.now()-(w.lastWaitReportAt||0)<60000)continue;
   const pending=db.prepare('SELECT count(*) n FROM deliveries WHERE recipient=? AND acknowledgedAt IS NULL').get(w.session).n;
   if(pending){w.lastWaitReportAt=Date.now();console.log(JSON.stringify({waiting:w.name,phase:label,pendingMessages:pending,secondsSinceEvent:Math.round((Date.now()-(w.lastEventAt||started))/1000),lastEvent:w.lastEventType,note:'Observer only; no replay, model prompt or automatic restart'}));}
  }
  const failed=workers.find(w=>(w.exited&&!w.expectedExit)||w.protocolError);if(failed)throw Error(`${failed.name}: ${failed.protocolError||failed.stderr.slice(-2000)}`);
  if(Date.now()>deadline)throw Error(`Timed out: ${label}`);
  service();await new Promise(r=>setTimeout(r,300));
 }
 phases.push({name:label,elapsedSeconds:(Date.now()-started)/1000});console.log(JSON.stringify(phases.at(-1)));
}
function start(vendor,index,model){
 const name=`mesh-${vendor}-${index}`;
 const prompt=`You are ${name}, one of ${workerCount} real workers testing database-backed communication. Use only the supplied bound communication tools, not another vendor's private agent manager. Do not create agents or edit repository source files. Only the supplied document tools may access shared handoff documents. Your controller is ${controller.id}. Test token: ${nonce}.
Use short plain-text message bodies exactly as specified below (no JSON inside body). Reuse stable retry keys. Handle each incoming message ID once, then ack it using the ack tool. Reply only where requested; never reply to an ACK. Finish turns promptly so the proxy can deliver more inbox data. Do not call inbox: this managed host automatically delivers every new message to you. Inbox is a manual recovery tool and no recovery is requested in this test. No polling loops, sleeps, or progress messages.
First call peers, send body READY ${nonce} ${name} directly to the controller, and finish your turn.
Only handle messages with this token. Controller commands:
- CAPABILITIES: call peers and send CAPABILITIES ${nonce} ${name} TOOLS= followed by a comma-separated list of all supplied bound communication tool names to the controller. Name tools you can actually call; no descriptions.
- DOCUMENT TOKEN NAME OFFSET: call read_document with name NAME, offset OFFSET, limit 100. Send DOC_READ ${nonce} ${name} followed by the exact DOC_PROOF line returned, to controller. Never paste the full document into messages.
- BUNDLE: call lock_many on [{path:mesh/bundle-b.md},{path:mesh/bundle-a.md}] with ttlMs 120000, check success, unlock each returned lease ID, then send BUNDLE_DONE ${nonce} ${name} to controller.
- HOLD: acquire mesh/closed.md with ttlMs 120000, then send HELD ${nonce} ${name} to controller and retain this lease until shutdown.
- MESH: call peers; find these ${workerCount} names: ${workerNames.join(', ')}. Send DIRECT ${nonce} ${name} once to each of the OTHER ${workerCount-1} worker session IDs, using key direct-${nonce}-RECIPIENT_ID. Do not send DIRECT to the controller or DB-only agent.
- NOTIFY: call notify_all once with body BROADCAST ${nonce} ${name} and key broadcast-${nonce}. This snapshots all active peers, including controller/DB-only; no subscription needed.
- BLOCK: attempt lock on mesh/report.md with ttlMs 120000. Expected parent-tree conflict: if denied, send ASK_LOCK ${nonce} ${name} directly to the conflict owner with key ask-lock-LEASE_ID, then send BLOCKED ${nonce} ${name} to controller. Ask only once and finish your turn; do not wait or retry until LEASE arrives. Never report a conflict unless the tool actually denied it. If unexpectedly acquired, unlock and report UNEXPECTED to controller.
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
  try{const event={at:Date.now(),worker:name,...JSON.parse(line)};events.push(event);trace.write(JSON.stringify(event)+'\n');w.lastEventAt=event.at;w.lastEventType=event.type;
   if(event.type==='ready'){w.session=event.session;w.pid=event.pid;saveState();console.log(JSON.stringify({ready:name}));}
  }catch(e){w.protocolError=e.message;}
 });
}
function command(w,name,detail=''){return invoke('send_message',{to:w.session,body:`${name} ${nonce}${detail?' '+detail:''}`,key:`${name}-${w.name}`},controller.id);}
const dbReply=(w,message)=>db.prepare('SELECT id FROM messages WHERE sender=? AND target=? AND key=? AND (body=? OR body=?)').get(w.session,generic.id,`db-received-${message}`,`DB_RECEIVED ${nonce} ${w.name}`,`DB_RECEIVED ${nonce}`);
const directed=(kind,reply)=>workers.every(a=>workers.filter(b=>b!==a).every(b=>find(a.session,b.session,`${kind} ${nonce} ${a.name}`)&&find(b.session,a.session,`${reply} ${nonce} ${b.name}`)));
try{
 for(const [vendor,model] of [['codex','gpt-6-luna'],['claude','haiku'],['pi',piModel]].filter(([v])=>vendors.includes(v)))for(let index=1;index<=perVendor;index++)start(vendor,index,model);
 saveState();console.log(JSON.stringify({directory,nonce}));
 await until(()=>workers.every(w=>w.session&&find(w.session,controller.id,`READY ${nonce} ${w.name}`)),`${workerCount} ready workers`);
 for(const w of workers)command(w,'CAPABILITIES');
 await until(()=>workers.every(w=>rows().some(m=>m.sender===w.session&&m.target===controller.id&&m.body.startsWith(`CAPABILITIES ${nonce} ${w.name} TOOLS=`))&&receipts(w,'peers',v=>JSON.stringify(v).includes('mesh-db-only')).length),`${workerCount} workspace discovery and tool inventory answers`);
 for(const w of workers){
  const answer=rows().find(m=>m.sender===w.session&&m.body.startsWith(`CAPABILITIES ${nonce} ${w.name} TOOLS=`)).body;
  for(const name of expectedTools)assert.ok(answer.includes(name),`${w.name}: tool inventory omitted ${name}`);
 }
 for(const w of workers)command(w,'DOCUMENT',`${documentName} ${documentOffset}`);
 await until(()=>workers.every(w=>find(w.session,controller.id,`DOC_READ ${nonce} ${w.name} ${documentMarker}`)&&receipts(w,'read_document',v=>JSON.stringify(v).includes(documentMarker)).length),`${workerCount} bounded document reads and concise handoffs`);
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
 await until(()=>workers.every(w=>find(w.session,controller.id,`ASK_LOCK ${nonce} ${w.name}`)),`${workerCount} conflict-owner questions without blocking`);
 for(const w of workers)assert.ok(receipts(w,'lock',v=>v.ok===false&&v.conflict?.owner===controller.id).length);
 invoke('unlock',{lease:held.id},controller.id);heldReleased=true;
 for(const w of workers){
  command(w,'LEASE');
  await until(()=>receipts(w,'lock',v=>v.ok===true&&v.lease?.owner===w.session).length&&receipts(w,'renew',v=>v.renewed===true).length&&receipts(w,'unlock',v=>v.released===true).length,`${w.name} acquire-renew-release`);
  for(const [tool,predicate] of [['lock',v=>v.ok===true&&v.lease?.owner===w.session],['renew',v=>v.renewed===true],['unlock',v=>v.released===true]])assert.ok(receipts(w,tool,predicate).length,`${w.name}: missing ${tool}`);
 }
 for(const w of workers){
  command(w,'BUNDLE');
  await until(()=>find(w.session,controller.id,`BUNDLE_DONE ${nonce} ${w.name}`)&&receipts(w,'lock_many',v=>v.ok===true&&v.leases?.length===2).length,`${w.name} atomic reverse-order multi-path acquisition`);
 }
 await until(()=>db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n===0,'every delivery acknowledged');
 // A stalled owner stays present but does not renew its lease. Expiry lets peers proceed.
 const stale=invoke('lock',{path:'mesh/expired.md',ttlMs:1000},generic.id).lease;
 assert.equal(invoke('lock',{path:'mesh/expired.md'},controller.id).ok,false);
 await new Promise(resolve=>setTimeout(resolve,1100));
 const recovered=invoke('lock',{path:'mesh/expired.md'},controller.id);assert.equal(recovered.ok,true);
 assert.equal(invoke('renew',{lease:stale.id},generic.id).renewed,false);
 assert.equal(invoke('unlock',{lease:stale.id},generic.id).released,false);
 invoke('unlock',{lease:recovered.lease.id},controller.id);
 invoke('prune',{},controller.id);
 phases.push({name:'present but stalled owner expires; stale lease cannot renew or unlock',elapsedSeconds:(Date.now()-started)/1000});
 // Close one real vendor worker after all communication barriers, retaining a live lease.
 const closing=workers[0];command(closing,'HOLD');
 await until(()=>find(closing.session,controller.id,`HELD ${nonce} ${closing.name}`)&&receipts(closing,'lock',v=>v.ok===true&&v.lease?.path?.endsWith('/mesh/closed.md')).length,'real vendor worker holds lease before close');
 await until(()=>db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n===0,'close command and response acknowledged');
 closing.expectedExit=true;
 await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('Closing worker did not stop within 10 seconds')),10000);closing.child.once('exit',()=>{clearTimeout(timer);resolve();});closing.child.kill('SIGTERM');});
 const closedRecovery=invoke('lock',{path:'mesh/closed.md'},controller.id);assert.equal(closedRecovery.ok,true);
 assert.ok(db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(closing.session).expiresAt<=Date.now());
 invoke('unlock',{lease:closedRecovery.lease.id},controller.id);
 phases.push({name:'closed real vendor releases presence and lease; peer continues',elapsedSeconds:(Date.now()-started)/1000});
 assert.equal(db.prepare('SELECT count(*) n FROM leases').get().n,0);
 const recoveryReads=events.filter(e=>e.type==='tool-result'&&e.tool?.endsWith('inbox')).length;
 assert.equal(recoveryReads,0,'managed workers use host delivery without unsolicited inbox recovery reads');
 const duplicateDispatches=db.prepare("SELECT session,entityId,count(*) n FROM audit WHERE kind='dispatch.staged' GROUP BY session,entityId HAVING count(*)>1").all();
 assert.deepEqual(duplicateDispatches,[],'no recipient is offered the same message twice');
 for(const w of workers){
  const deliveryCount=db.prepare('SELECT count(*) n FROM deliveries WHERE recipient=?').get(w.session).n;
  const submittedCount=db.prepare("SELECT count(*) n FROM dispatches WHERE recipient=? AND state='submitted'").get(w.session).n;
  assert.equal(submittedCount,deliveryCount,`${w.name}: every received delivery has one submitted dispatch`);
 }
 const all=rows();
 assert.equal(all.filter(m=>m.body.startsWith(`DIRECT ${nonce} `)).length,pairCount);
 assert.equal(all.filter(m=>m.body.startsWith(`ACK_DIRECT ${nonce} `)).length,pairCount);
 assert.equal(all.filter(m=>m.body.startsWith(`BROADCAST ${nonce} `)).length,workerCount);
 const matrix=workers.map(a=>({sender:a.name,recipients:workers.filter(b=>b!==a).map(b=>({name:b.name,message:find(a.session,b.session,`DIRECT ${nonce} ${a.name}`).id,reply:find(b.session,a.session,`ACK_DIRECT ${nonce} ${b.name}`).id}))}));
 for(const a of workers)for(const b of workers.filter(w=>w!==a)){
  const m=find(a.session,b.session,`DIRECT ${nonce} ${a.name}`);
  assert.ok(receipts(a,'send_message',v=>v.id===m.id).length,`${a.name}: DB row lacks vendor tool receipt`);
 }
 report={passed:true,nonce,skillSource:suppliedSkill?'existing built skill':'copied standalone skill',skillPath:skill,models:workers.map(({name,vendor,model})=>({name,vendor,model})),skillSha256:createHash('sha256').update(readFileSync(join(skill,'SKILL.md'))).digest('hex'),phases,matrix,
  document:{name:documentName,bytes:sharedDocument.document.bytes,readOffset:documentOffset,readLimit:100,receipt:sharedDocument},usage:db.prepare("SELECT session,data FROM audit WHERE kind='usage' ORDER BY id").all().map(r=>({session:r.session,...JSON.parse(r.data)})),
  checks:[`${workerCount} peer discovery and capability questions`,`${workerCount} bounded document handoffs without full-body messages`,`${workerCount} lock owner questions before retry`,`${workerCount} atomic multi-path acquisitions`,'present but stalled owner lease expiry and stale-ID rejection','closed real vendor worker lease recovery',`all ${pairCount} directed worker pairs with replies`,'same-vendor pairs included',`${workerCount} notify_all calls with ${broadcastRecipients} recipients each and actual tool receipts`,'zero topic subscriptions required',`Python-only direct and broadcast messages reach all ${workerCount} workers`,'broadcast retry preserves receipt and snapshot',`${workerCount} tree conflicts and ${workerCount} acquire/renew/unlock lifecycles`,'all deliveries acknowledged and leases released','each worker delivery staged once and submitted; no automatic replay'],
  metrics:{unsolicitedInboxReads:recoveryReads,messages:all.length,deliveries:db.prepare('SELECT count(*) n FROM deliveries').get().n,directedPairs:pairCount,workerBroadcasts:workerCount,workerBroadcastDeliveries:workerCount*broadcastRecipients,dbOnlyBroadcastRecipients:broadcastRecipients,unexpectedModelReports:all.filter(m=>m.body.startsWith(`UNEXPECTED ${nonce} `)).length,supervisorInterventions:all.filter(m=>m.sender===controller.id&&m.body.startsWith(`CHECK ${nonce}:`)).length},messages:all,events};
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
  report.usage=db.prepare("SELECT session,data FROM audit WHERE kind='usage' ORDER BY id").all().map(r=>({session:r.session,...JSON.parse(r.data)}));
  report.usageSummary=workers.map(w=>{
   const entries=report.usage.filter(r=>r.session===w.session);
   const cumulative=entries.filter(r=>r.scope==='cumulative');
   const measured=cumulative.length?cumulative.slice(-1):entries.filter(r=>r.scope==='turn'||r.scope==='request');
   const fields=['inputTokens','outputTokens','cachedInputTokens','cacheWriteTokens'];
   return {worker:w.name,vendor:w.vendor,records:entries.length,accounting:cumulative.length?'latest cumulative snapshot':'sum of per-turn/request reports',...Object.fromEntries(fields.map(field=>[field,measured.some(r=>Number.isFinite(r[field]))?measured.reduce((sum,r)=>sum+(r[field]||0),0):null])),maxObservedContextTokens:entries.some(r=>Number.isFinite(r.contextTokens))?Math.max(...entries.map(r=>r.contextTokens||0)):null};
  });
  const createdAt=new Map(db.prepare("SELECT entityId,at FROM audit WHERE kind='message.created'").all().map(r=>[Number(r.entityId),r.at]));
  const roundTrips=report.matrix.flatMap(row=>row.recipients.map(pair=>createdAt.get(pair.reply)-createdAt.get(pair.message))).sort((a,b)=>a-b);
  report.metrics.directReplyLatencyMs={min:roundTrips[0],median:roundTrips[Math.floor(roundTrips.length/2)],p95:roundTrips[Math.min(roundTrips.length-1,Math.ceil(roundTrips.length*.95)-1)],max:roundTrips.at(-1)};
  report.metrics.maxMessageBodyBytes=Math.max(...report.messages.map(m=>Buffer.byteLength(m.body)));
  report.metrics.documentBytesAvoidedPerRecipient=report.document.bytes-report.document.readLimit;
  report.metrics.usageNote='Recipient model usage; routing uses no model. Vendor token semantics differ; missing fields are null. Final interrupted responses may not report usage.';
  writeFileSync(join(directory,'result.json'),JSON.stringify(report,null,2),{mode:0o600});
  console.log(JSON.stringify({passed:true,result:join(directory,'result.json'),metrics:report.metrics}));
 }
 db.close();saveState();
 await new Promise(resolve=>trace.end(resolve));
}
