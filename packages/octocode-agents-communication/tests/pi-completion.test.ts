// Pi opt-in completion checks: recovery of omitted FYIs once per external work cycle, and cancellation of stale lifecycle work.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chmodSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { registerPiInbox } from '../dist/hosts/pi-inbox.js';
import type { JsonRecord } from './json-record.ts';
import { binary, boundSession, waitFor } from './helpers.ts';
import { detailsOf, fixture, type Fixture } from './pi-inbox-support.ts';

const completionMessages = (f: Fixture) => f.sent.filter(s=>s.message.customType==='octocode-completion');
const tick = () => new Promise(resolve=>setImmediate(resolve));
// Records every completion-check start and result so "no recovery" is asserted after the
// check positively decided, never after an arbitrary sleep.
function loggedChecks(f: Fixture) {
 const log=join(f.workspace,'completion-checks.jsonl'),wrapper=join(f.workspace,'logged-completion.mjs');
 writeFileSync(wrapper,`#!${process.execPath}\nimport { execFileSync } from ${JSON.stringify(new URL('./helpers.ts', import.meta.url).href)};import {appendFileSync} from 'node:fs';\nconst check=process.argv[2]==='completion-check';if(check)appendFileSync(${JSON.stringify(log)},'{"started":true}\\n');const out=execFileSync(${JSON.stringify(binary)},process.argv.slice(2),{encoding:'utf8'});if(check)appendFileSync(${JSON.stringify(log)},JSON.stringify({result:JSON.parse(out)})+'\\n');process.stdout.write(out);`);chmodSync(wrapper,0o700);
 const rows=()=>existsSync(log)?readFileSync(log,'utf8').trim().split('\n').filter(Boolean).map((line): JsonRecord=>JSON.parse(line)):[];
 const started=()=>rows().filter(row=>row.started).length,results=()=>rows().filter(row=>row.result).map(row=>row.result);
 // Waits until `count` checks finished; the last one must not block, so no recovery can follow.
 const settledAllow=async (count: number)=>{await waitFor(()=>results().length>=count,{timeout:5000,message:'Completion check did not finish'});
  assert.equal(results().length,count);const last=results().at(-1);assert.ok(last);assert.notEqual(last.decision,'block');};
 return {wrapper,started,results,settledAllow};
}
async function settleCompletion(f: Fixture, expected: number) {
 const deadline=Date.now()+3000;
 while(Date.now()<deadline && completionMessages(f).length<expected) await new Promise(resolve=>setTimeout(resolve,20));
 assert.equal(completionMessages(f).length,expected);
}
test('Pi opt-in completion recovers omitted FYI once per external work cycle without automatic ACK',async t=>{
 const f=fixture(t),checks=loggedChecks(f),controller=registerPiInbox(f.pi,{binary:checks.wrapper,database:f.database,completionCheck:true});
 await f.fire('session_start');
 try {
  const session=boundSession(controller),sender=f.run('join',{name:'sender',vendor:'generic'}).id;
  const sent=f.run('send_message',{to:session,body:'FYI evidence',reasoning:'Check omission recovery',wake:'passive'},sender);
  await controller.drain();assert.equal(completionMessages(f).length,0,'idle passive delivery cannot wake recovery');
  await f.fire('agent_start');await f.fire('agent_end');await settleCompletion(f,1);
  const recovery=completionMessages(f)[0];assert.deepEqual(detailsOf(recovery).pending,[sent.id]);
  assert.equal(recovery.options.triggerTurn,true);assert.equal(recovery.options.deliverAs,'followUp');
  assert.ok(!recovery.message.content.includes('FYI evidence'));
  await f.fire('input',{source:'extension',text:'Recovery'});
  await f.fire('before_agent_start');await f.fire('agent_start');await f.fire('agent_end');
  await tick();assert.equal(checks.started(),1,'the recovery turn reuses its cycle budget without another check');
  assert.equal(completionMessages(f).length,1,'unfinished recovery cannot loop');
  assert.equal(f.run('inbox',{message:sent.id},session).items.length,1,'recovery is not acknowledgement');
  await f.fire('input',{source:'interactive',text:'Continue review'});await f.fire('agent_start');await f.fire('agent_end');await settleCompletion(f,2);
  assert.equal(checks.started(),2,'only the new external cycle ran a check');
  f.run('complete',{message:sent.id},session);
  await f.fire('input',{source:'rpc',text:'Next task'});await f.fire('agent_start');await f.fire('agent_end');
  await checks.settledAllow(3);assert.equal(completionMessages(f).length,2);
  f.run('send_message',{to:session,body:'New action',wake:'action',reasoning:'Verify action cycle reset'},sender);
  await controller.drain();await f.fire('agent_start');await f.fire('agent_end');await settleCompletion(f,3);
 } finally {await f.fire('session_shutdown');}
});
test('Pi completion ignores staged-only passive mail and cancels stale lifecycle work',async t=>{
 const f=fixture(t),checks=loggedChecks(f);f.setDisk(false);
 const controller=registerPiInbox(f.pi,{binary:checks.wrapper,database:f.database,completionCheck:true});await f.fire('session_start');
 try {
  const session=boundSession(controller),sender=f.run('join',{name:'sender',vendor:'generic'}).id;
  f.run('send_message',{to:session,body:'not durable',reasoning:'Check submitted-only recovery',wake:'passive'},sender);
  await controller.drain();await f.fire('agent_start');await f.fire('agent_end');
  await checks.settledAllow(1);assert.equal(completionMessages(f).length,0);
  f.setDisk(true);f.flush();await controller.drain();
  const stale={...f.ctx,sessionManager:{...f.ctx.sessionManager,getSessionId:()=> 'wrong'}};
  await f.fire('agent_end',{},stale);await tick();assert.equal(checks.started(),1,'a stale context schedules no check');
  await f.fire('agent_start');await f.fire('agent_end');await f.fire('session_shutdown');await tick();
  assert.equal(checks.started(),1,'shutdown cancels the pending lifecycle check');assert.equal(completionMessages(f).length,0);
 } finally {await f.fire('session_shutdown');}
});

test('Pi shutdown cancels an in-flight completion result before it can wake a stale session',async t=>{
 const f=fixture(t),marker=join(f.workspace,'completion-ready'),release=join(f.workspace,'completion-release'),wrapper=join(f.workspace,'delayed-completion.mjs');
 writeFileSync(wrapper,`#!${process.execPath}\nimport { execFileSync } from ${JSON.stringify(new URL('./helpers.ts', import.meta.url).href)};import {writeFileSync,existsSync} from 'node:fs';\nconst out=execFileSync(${JSON.stringify(binary)},process.argv.slice(2),{encoding:'utf8'});if(process.argv[2]==='completion-check'){writeFileSync(${JSON.stringify(marker)},'ready');while(!existsSync(${JSON.stringify(release)}))await new Promise(r=>setTimeout(r,5));}process.stdout.write(out);`);chmodSync(wrapper,0o700);
 const controller=registerPiInbox(f.pi,{binary:wrapper,database:f.database,completionCheck:true});await f.fire('session_start');
 try {
  const session=boundSession(controller),sender=f.run('join',{name:'sender',vendor:'generic'}).id;
  f.run('send_message',{to:session,body:'FYI',reasoning:'Check stale recovery cancellation',wake:'passive'},sender);
  await controller.drain();await f.fire('agent_start');await f.fire('agent_end');
  const deadline=Date.now()+5000;while(!existsSync(marker)){if(Date.now()>deadline)throw new Error('Completion did not reach barrier');await new Promise(r=>setTimeout(r,5));}
  const stopping=f.fire('session_shutdown');writeFileSync(release,'go');await stopping;
  assert.equal(completionMessages(f).length,0);assert.equal(controller.getBinding(),null);
 } finally {writeFileSync(release,'go');await f.fire('session_shutdown');}
});
