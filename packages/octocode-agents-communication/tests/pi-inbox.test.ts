import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.ts';
import { existsSync, writeFileSync, readFileSync, chmodSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { registerPiInbox } from '../dist/hosts/pi-inbox.js';
import type { PiBinding } from '../dist/hosts/pi-inbox.js';
import type { JsonRecord } from './json-record.ts';
import { binary, tempWorkspace, boundBinding, boundSession } from './helpers.ts';
import { detailsOf, fixture } from './pi-inbox-support.ts';

test('Pi directory-only changes appear once without waking a turn', async t => {
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});
 await f.fire('session_start');
 try {
  const peer=f.run('join',{name:'reviewer',vendor:'generic',task:'Review storage',status:'busy'}).id;
  await controller.drain();
  let updates=f.sent.filter(s=>s.message.customType==='octocode-directory');
  assert.equal(updates.length,1);assert.ok(updates[0].message.content.includes(peer));
  assert.equal(updates[0].options.triggerTurn,false);
  await controller.drain();assert.equal(f.sent.filter(s=>s.message.customType==='octocode-directory').length,1);
  f.run('heartbeat',{status:'available'},peer);await controller.drain();
  updates=f.sent.filter(s=>s.message.customType==='octocode-directory');
  assert.equal(updates.length,2);assert.ok(updates[1].message.content.includes('available'));
 } finally {await f.fire('session_shutdown');}
});
test('Pi native tool selection uses the same CLI contract without creating storage',t=>{
 const f=fixture(t);
 registerPiInbox(f.pi,{binary,database:f.database,tools:'complete,send_message,peers'});
 assert.deepEqual(f.tools.map(tool=>tool.name),['peers','send_message','complete']);
 assert.equal(existsSync(f.database),false);
 const bad=fixture(t);
 assert.throws(()=>registerPiInbox(bad.pi,{binary,database:bad.database,tools:'unknown'}));
 assert.equal(existsSync(bad.database),false);
});
test('Pi durable receipts survive reload and bound tools are registered once',async t=>{
 const f=fixture(t);const seen: { binding: PiBinding | null }={binding:null};const controller=registerPiInbox(f.pi,{binary,database:f.database,onBinding:value=>{seen.binding=value;}});
 assert.equal(registerPiInbox(f.pi),controller);
 await f.fire('session_start');assert.ok(seen.binding?.session);const session=seen.binding.session;const count=f.tools.length;
 const sender=f.run('join',{vendor:'raw',name:'sender'}).id;
 const sent=f.run('send_message',{to:session,body:'durable',reasoning:'Test receipt',wake:'passive'},sender);
 await controller.drain();
 assert.ok(f.ledger.some(e=>e.details.receipts?.some((r: JsonRecord)=>r.id===sent.id)));
 assert.equal(f.run('inbox',{},session).items.length,1,'persistence is not acknowledgement');
 assert.ok(f.sent.every(s=>s.options.triggerTurn===false&&s.options.deliverAs==='steer'));
 await f.fire('session_start');assert.equal(seen.binding?.session,session);assert.equal(f.tools.length,count);
 assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,1);
 await f.fire('session_shutdown');assert.equal(controller.getBinding(),null);
});
test('Pi staged message without a durable receipt is recovered after lifecycle reload',async t=>{
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});
 await f.fire('session_start');const session=boundSession(controller);
 const sender=f.run('join',{vendor:'raw',name:'sender'}).id;
 const sent=f.run('send_message',{to:session,body:'missing receipt',reasoning:'Test reload recovery'},sender);
 f.setPersist(false);await controller.drain();f.setPersist(true);
 const before=JSON.parse(execFileSync(binary,['fetch',JSON.stringify({type:'dispatch.staged',current:true,from:session,where:{messageId:sent.id}}),'--workspace',f.workspace,'--database',f.database,'--session',session],{encoding:'utf8'}));
 assert.equal(before.items[0].type.slice('dispatch.'.length),'staged');
 await f.fire('session_start');
 assert.equal(f.ledger.filter(e=>e.details.receipts?.some((r: JsonRecord)=>r.id===sent.id)).length,1);
 await f.fire('session_shutdown');
});
test('Pi confirms a persisted staged attempt on reload without replaying its body',async t=>{
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});await f.fire('session_start');
 const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
 f.run('send_message',{to:session,body:'already durable',reasoning:'Test receipt recovery'},sender);
 const {items}=f.run('hook',{format:'json',deferConfirm:true,consumer:'pi:pi-test'},session);
 f.ledger.push({type:'custom_message',customType:'octocode-peer',details:{session,database:f.database,receipts:items.map(({id,dispatchToken}: JsonRecord)=>({id,dispatchToken}))}});f.flush();
 await f.fire('session_start');assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,0);
 await f.fire('session_shutdown');
});
test('disabled Pi persistence creates no database and leaves no enabled binding',async t=>{
 const f=fixture(t);let enabled=false;const controller=registerPiInbox(f.pi,{binary,database:f.database,enabled:()=>enabled});
 await f.fire('session_start');assert.equal(existsSync(f.database),false);assert.equal(controller.getBinding(),null);
 enabled=true;await f.fire('session_start');assert.ok(controller.getBinding());enabled=false;await f.fire('session_start');assert.equal(controller.getBinding(),null);
 await assert.rejects(f.tools[0].execute('x',{}),/disabled/);await f.fire('session_shutdown');
});

test('Pi confirms a durable receipt in a session ledger larger than 64 MiB, with lines crossing every read chunk edge',async t=>{
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});await f.fire('session_start');
 const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
 const message=f.run('send_message',{to:session,body:'in a huge ledger',reasoning:'Test receipt scan of a large ledger'},sender);
 const {items}=f.run('hook',{format:'json',deferConfirm:true,consumer:'pi:pi-test'},session);
 const state=()=>JSON.parse(execFileSync(binary,['fetch',JSON.stringify({types:['dispatch.staged','dispatch.submitted'],current:true,from:session,where:{messageId:message.id}}),'--workspace',f.workspace,'--database',f.database,'--session',session],{encoding:'utf8'})).items[0].type.slice('dispatch.'.length);
 assert.equal(state(),'staged');
 // 70 filler lines of ~1.0 MB (multi-byte text, so a chunk edge can fall inside a character) pass 64 MiB; the receipt is last.
 const filler=JSON.stringify({type:'message',text:'é'.repeat(500000)}),receipt={type:'custom_message',customType:'octocode-peer',details:{session,database:f.database,receipts:items.map(({id,dispatchToken}: JsonRecord)=>({id,dispatchToken}))}};
 writeFileSync(join(f.workspace,'pi-session.jsonl'),[JSON.stringify({type:'session',id:'pi-test'}),...Array(70).fill(filler),JSON.stringify(receipt)].join('\n')+'\n');
 assert.ok(statSync(join(f.workspace,'pi-session.jsonl')).size>64*1024*1024);
 await f.fire('session_start');
 assert.equal(state(),'submitted');assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,0);
 await f.fire('session_shutdown');
});
test('Pi memory-only context never confirms until the matching session file is written',async t=>{
 const f=fixture(t);f.setDisk(false);const controller=registerPiInbox(f.pi,{binary,database:f.database});await f.fire('session_start');
 const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
 const message=f.run('send_message',{to:session,body:'not durable yet',reasoning:'Test real persistence boundary'},sender);
 await controller.drain();await controller.drain();
 const state=()=>JSON.parse(execFileSync(binary,['fetch',JSON.stringify({types:['dispatch.staged','dispatch.submitted','dispatch.uncertain','dispatch.ready'],current:true,from:session,where:{messageId:message.id}}),'--workspace',f.workspace,'--database',f.database,'--session',session],{encoding:'utf8'})).items[0].type.slice('dispatch.'.length);
 assert.equal(state(),'staged');assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,1);
 f.setDisk(true);f.flush();await controller.drain();assert.equal(state(),'submitted');
 await f.fire('session_shutdown');
});

test('Pi confirms accumulated durable batches and continues draining without replay',async t=>{
 const f=fixture(t);f.setDisk(false);const controller=registerPiInbox(f.pi,{binary,database:f.database});
 try {
 await f.fire('session_start');const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
 const ids=[];
 for(let i=0;i<35;i++) ids.push(f.run('send_message',{to:session,body:`pending-${i}`,key:`pending-${i}`,reasoning:'Verify delayed ledger flush across several native batches'},sender).id);
 for(let i=0;i<4;i++) await controller.drain();
 const receipts=()=>f.sent.filter(s=>s.message.customType==='octocode-peer').flatMap(s=>detailsOf(s).receipts).map(r=>r.id);
 assert.deepEqual(receipts(),ids);
 f.setDisk(true);f.flush();await controller.drain();
 const dispatches=JSON.parse(execFileSync(binary,['fetch',JSON.stringify({types:['dispatch.staged','dispatch.submitted','dispatch.uncertain','dispatch.ready'],current:true,from:session}),'--workspace',f.workspace,'--database',f.database,'--session',session],{encoding:'utf8'})).items;
 assert.equal(dispatches.length,35);assert.ok(dispatches.every((d: JsonRecord)=>d.type==='dispatch.submitted'));
 assert.deepEqual(receipts(),ids,'confirmation must not replay context');
 const later=f.run('send_message',{to:session,body:'after confirmation',reasoning:'Verify polling was not stalled'},sender);
 await controller.drain();assert.deepEqual(receipts(),[...ids,later.id]);
 } finally { await f.fire('session_shutdown'); }
});

test('shutdown fences an in-flight inbox poll before it can inject peer context',async t=>{
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});await f.fire('session_start');
 const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
 f.run('send_message',{to:session,body:'stop before delivery',reasoning:'Test lifecycle fence'},sender);
 const draining=controller.drain();await Promise.resolve();const stopping=f.fire('session_shutdown');
 await Promise.all([draining,stopping]);
 assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,0);
 assert.equal(controller.getBinding(),null);
});

test('context from another Pi session cannot receive a previous session inbox',async t=>{
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});await f.fire('session_start');
 const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
 f.run('send_message',{to:session,body:'belongs to old session',reasoning:'Test stale context'},sender);
 f.ctx.sessionManager.getSessionId=()=> 'new-pi-session';
 await f.fire('before_agent_start');await controller.drain();
 assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,0);
 await f.fire('session_shutdown');
});

test('turning persistence off stops bound tools without waiting for session restart',async t=>{
 const f=fixture(t);let enabled=true;const controller=registerPiInbox(f.pi,{binary,database:f.database,enabled:()=>enabled});await f.fire('session_start');
 enabled=false;
 await assert.rejects(f.tool('send_message').execute('x',{to:'any',body:'disabled',reasoning:'No persistence'}),/disabled/);
 await f.fire('before_agent_start');assert.equal(controller.getBinding(),null);
 await f.fire('session_shutdown');
});


test('context outside the bound workspace cannot route peer data or tools',async t=>{
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});await f.fire('session_start');
 const other = tempWorkspace(t, 'pi-other-workspace-'); f.ctx.cwd=other;
 assert.equal(controller.isBoundContext(f.ctx),false);
 await assert.rejects(f.tool('peers').execute('x',{}),/disabled/);
 await f.fire('before_agent_start');assert.equal(controller.getBinding(),null);
 await f.fire('session_shutdown');
});


for(const replacement of [false,true]) test(`${replacement?'replacement':'shutdown'} during native join closes the superseded identity`,async t=>{
 const f=fixture(t),marker=join(f.workspace,'joined.json'),release=join(f.workspace,'release'),wrapper=join(f.workspace,'delayed-native.mjs');
 writeFileSync(wrapper,`#!${process.execPath}
import { execFileSync } from ${JSON.stringify(new URL('./helpers.ts', import.meta.url).href)};import {writeFileSync,existsSync} from 'node:fs';
const out=execFileSync(${JSON.stringify(binary)},process.argv.slice(2),{encoding:'utf8'});if(process.argv[2]==='join'){writeFileSync(${JSON.stringify(marker)},out);while(!existsSync(${JSON.stringify(release)}))await new Promise(r=>setTimeout(r,5));}process.stdout.write(out);`);chmodSync(wrapper,0o700);
 const controller=registerPiInbox(f.pi,{binary:wrapper,database:f.database});const starting=f.fire('session_start');
 try {
   const deadline=Date.now()+5000;while(!existsSync(marker)){if(Date.now()>deadline)throw new Error('join did not reach barrier');await new Promise(r=>setTimeout(r,5));}
   const id=JSON.parse(readFileSync(marker,'utf8')).id;
   f.setDisk(false);
   const nextContext={...f.ctx,sessionManager:{...f.ctx.sessionManager,getSessionId:()=> 'replacement-session'}};
   const changing=replacement?f.fire('session_start',{},nextContext):f.fire('session_shutdown');
   writeFileSync(release,'go');await Promise.all([starting,changing]);
   f.run('join',{vendor:'raw',name:'inspector'});
   const identity=JSON.parse(execFileSync(binary,['binding','--workspace',f.workspace,'--database',f.database,'--session',id],{encoding:'utf8'}));
   if(replacement){assert.equal(boundBinding(controller).vendorSession,'replacement-session');assert.equal(f.sent.length,1);}
   else {assert.equal(controller.getBinding(),null);assert.equal(f.sent.length,0);}
   assert.equal(identity.active,false);
 } finally {writeFileSync(release,'go');await starting.catch(()=>{});await f.fire('session_shutdown');}
});

test('revoked SDK contexts fail closed without throwing from idle checks',async t=>{
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});await f.fire('session_start');
 Object.defineProperty(f.ctx,'sessionManager',{get(){throw new Error('stale SDK context');}});
 assert.equal(controller.getBinding(),null);assert.equal(controller.isBoundContext(f.ctx),false);
 await controller.drain();await f.fire('before_agent_start');await f.fire('session_shutdown');
});


test('Pi native context preserves reply correlation without replaying durable messages', async t => {
 const f = fixture(t), controller = registerPiInbox(f.pi, { binary, database: f.database });
 await f.fire('session_start');
 try {
  const session = boundSession(controller);
  const sender = f.run('join', { vendor: 'raw', name: 'peer' }).id;
  const question = f.run('send_message', { to: sender, question: 'Can you review?', reasoning: 'Request a peer review', conversationId: 'review:42' }, session);
  const reply = f.run('complete', { message:question.id, outcome:'done',reply:{conclusion:'Review complete.'}, reasoning:'Report review result' }, sender);
  const notice = f.run('send_message', { to: session, body: 'Independent notice.', reasoning: 'Share a separate update' }, sender);
  await controller.drain();
  const contexts = f.sent.filter(item => item.message.customType === 'octocode-peer');
  assert.equal(contexts.length, 1);
  const content = contexts[0].message.content;
  const messages = JSON.parse(content.slice(content.indexOf('\n') + 1));
  const correlated = messages.find((item: JsonRecord) => item.data.messageId === reply.id);
  assert.equal(correlated.data.replyTo, question.id);
  assert.equal(correlated.data.conversationId, 'review:42');
  assert.equal(correlated.from, sender);
  assert.equal(correlated.data.body, 'BLUF\nOutcome: done\nConclusion: Review complete.');
  assert.equal(correlated.data.kind, 'answer');
  const unrelated = messages.find((item: JsonRecord) => item.data.messageId === notice.id);
  assert.equal(Object.hasOwn(unrelated.data, 'replyTo'), false);
  assert.equal(Object.hasOwn(unrelated.data, 'conversationId'), false);
  assert.equal(Object.hasOwn(unrelated.data, 'topic'), false);
  assert.equal(Object.hasOwn(correlated.data, 'topic'), false);
  assert.equal(unrelated.data.body, 'Independent notice.');
  assert.deepEqual(detailsOf(contexts[0]).receipts.map((item: JsonRecord) => item.id), [reply.id, notice.id]);
  assert.ok(detailsOf(contexts[0]).receipts.every((item: JsonRecord) => typeof item.dispatchToken === 'string'));
  assert.deepEqual(contexts[0].options, { triggerTurn: true, deliverAs: 'steer' });
  assert.equal(f.run('inbox', {}, session).items.length, 2, 'native persistence must not acknowledge application handling');
  await f.fire('session_start');
  assert.equal(f.sent.filter(item => item.message.customType === 'octocode-peer').length, 1, 'durable correlation is not replayed on reload');
 } finally {
  await f.fire('session_shutdown');
 }
});

test('idle Pi wakes once for action and preserves preceding passive context without another turn', async t => {
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});
 await f.fire('session_start');
 try {
  const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
  f.run('send_message',{to:session,body:'FYI',wake:'passive',reasoning:'Share context without interrupting'},sender);
  await controller.drain();
  const question=f.run('send_message',{to:session,body:'Please review',wake:'action',reasoning:'Unblock review'},sender);
  await controller.drain();await controller.drain();
  const peers=f.sent.filter(s=>s.message.customType==='octocode-peer');
  assert.deepEqual(peers.map(s=>s.options.triggerTurn),[false,true]);
  assert.equal(peers.filter(s=>detailsOf(s).receipts.some((r: JsonRecord)=>r.id===question.id)).length,1);
  assert.equal(f.run('inbox',{},session).items.length,2,'wake and durable delivery do not acknowledge handling');
  await f.fire('session_start');
  assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,2,'reload never wakes durable messages again');
 } finally {await f.fire('session_shutdown');}
});

test('action arriving during a Pi turn waits for idle and wakes once after agent_end', async t => {
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});
 await f.fire('session_start');
 try {
  const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
  await f.fire('agent_start');
  f.run('send_message',{to:session,body:'Next task',wake:'action',reasoning:'Coordinate after the active task'},sender);
  await controller.drain();
  assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,0);
  await f.fire('agent_end');
  await new Promise(resolve=>setImmediate(resolve));await controller.drain();
  const peers=f.sent.filter(s=>s.message.customType==='octocode-peer');
  assert.equal(peers.length,1);assert.equal(peers[0].options.triggerTurn,true);
 } finally {await f.fire('session_shutdown');}
});

test('Pi before_agent_start includes action context in the existing turn without a nested wake', async t => {
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});
 await f.fire('session_start');
 try {
  const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
  f.run('send_message',{to:session,body:'Join the current task',wake:'action',reasoning:'Supply context to the turn already starting'},sender);
  f.ctx.isIdle=()=>false;
  await controller.drain();assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,0);
  await f.fire('before_agent_start');
  const peers=f.sent.filter(s=>s.message.customType==='octocode-peer');
  assert.equal(peers.length,1);assert.equal(peers[0].options.triggerTurn,false);
  f.ctx.isIdle=()=>true;await controller.drain();
  assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,1);
 } finally {await f.fire('session_shutdown');}
});

test('a user turn starting during a Pi inbox read suppresses the pending automatic wake', async t => {
 const f=fixture(t),marker=join(f.workspace,'hook-ready'),release=join(f.workspace,'hook-release'),wrapper=join(f.workspace,'delayed-hook.mjs');
 writeFileSync(wrapper,`#!${process.execPath}\nimport { execFileSync } from ${JSON.stringify(new URL('./helpers.ts', import.meta.url).href)};import {writeFileSync,existsSync} from 'node:fs';\nconst out=execFileSync(${JSON.stringify(binary)},process.argv.slice(2),{encoding:'utf8'});if(process.argv[2]==='hook'&&JSON.parse(out).items.length){writeFileSync(${JSON.stringify(marker)},'ready');while(!existsSync(${JSON.stringify(release)}))await new Promise(r=>setTimeout(r,5));}process.stdout.write(out);`);chmodSync(wrapper,0o700);
 const controller=registerPiInbox(f.pi,{binary:wrapper,database:f.database});await f.fire('session_start');
 try {
  const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
  f.run('send_message',{to:session,body:'Concurrent action',wake:'action',reasoning:'Join the task while a user turn starts'},sender);
  const draining=controller.drain();
  const deadline=Date.now()+5000;while(!existsSync(marker)){if(Date.now()>deadline)throw new Error('Hook read did not reach barrier');await new Promise(r=>setTimeout(r,5));}
  const starting=f.fire('before_agent_start');
  writeFileSync(release,'go');await Promise.all([draining,starting]);
  const peers=f.sent.filter(s=>s.message.customType==='octocode-peer');
  assert.equal(peers.length,1);assert.equal(peers[0].options.triggerTurn,false);
 } finally {writeFileSync(release,'go');await f.fire('session_shutdown');}
});

test('Pi action wakes a fresh session before disk persistence but confirms only after its receipt is durable', async t => {
 const f=fixture(t);f.setDisk(false);
 const controller=registerPiInbox(f.pi,{binary,database:f.database});await f.fire('session_start');
 try {
  const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
  const message=f.run('send_message',{to:session,body:'Start review',wake:'action',reasoning:'Wake a fresh receiver'},sender);
  const state=()=>JSON.parse(execFileSync(binary,['fetch',JSON.stringify({types:['dispatch.staged','dispatch.submitted','dispatch.uncertain','dispatch.ready'],current:true,from:session,where:{messageId:message.id}}),'--workspace',f.workspace,'--database',f.database,'--session',session],{encoding:'utf8'})).items[0].type.slice('dispatch.'.length);
  await controller.drain();await controller.drain();
  const peers=f.sent.filter(s=>s.message.customType==='octocode-peer');
  assert.equal(peers.length,1);assert.equal(peers[0].options.triggerTurn,true);assert.equal(state(),'staged');
  f.setDisk(true);f.flush();await controller.drain();assert.equal(state(),'submitted');
  assert.equal(f.sent.filter(s=>s.message.customType==='octocode-peer').length,1);
 } finally {await f.fire('session_shutdown');}
});

test('Pi native delivery rejection stays staged and visible without an automatic repeated wake', async t => {
 const f=fixture(t),controller=registerPiInbox(f.pi,{binary,database:f.database});await f.fire('session_start');
 const errors: string[]=[];t.mock.method(console,'error',(message: string)=>errors.push(message));
 try {
  const session=boundSession(controller),sender=f.run('join',{vendor:'raw',name:'sender'}).id;
  const message=f.run('send_message',{to:session,body:'Needs handling',wake:'action',reasoning:'Verify failed native delivery remains pending'},sender);
  let attempts=0;f.pi.sendMessage=()=>{attempts++;throw new Error('Native session unavailable');};
  await controller.drain();await controller.drain();
  assert.equal(attempts,1);assert.ok(errors.some(e=>e.includes('Native session unavailable')));
  const dispatch=JSON.parse(execFileSync(binary,['fetch',JSON.stringify({types:['dispatch.staged','dispatch.submitted','dispatch.uncertain','dispatch.ready'],current:true,from:session,where:{messageId:message.id}}),'--workspace',f.workspace,'--database',f.database,'--session',session],{encoding:'utf8'}));
  assert.equal(dispatch.items[0].type.slice('dispatch.'.length),'staged');assert.equal(f.run('inbox',{},session).items.length,1);
 } finally {await f.fire('session_shutdown');}
});
