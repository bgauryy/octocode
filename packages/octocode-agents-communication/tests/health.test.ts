import {test} from 'node:test';
import type {TestContext} from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, execFile, spawn, spawnSync } from './helpers.ts';
import {promisify} from 'node:util';
import {existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import { binary, only, tempWorkspace, commandArgs, jsonCall } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';
import { fixture as storeFixture, runtime } from './store-support.ts';
const exec=promisify(execFile);
function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-health-'),database=join(workspace,'db.sqlite');
  const args=commandArgs(workspace,database);
  const call=jsonCall(binary,workspace,database,{stdio:'pipe',timeout:10000});
  const a=call('join',{name:'sender',vendor:'raw'}).id,b=call('join',{name:'recipient',vendor:'other'}).id;
  const db=new DatabaseSync(database);t.after(()=>db.close());
  return {workspace,database,args,call,a,b,db};
}
test('health stays read-only under a writer, omits bodies, and treats passive waiting as normal',async t=>{
  const f=fixture(t);
  f.call('send_message',{parts: [{ text: 'private-context-never-in-health' }], to:f.b,wake:'passive',reasoning:'Inform without waking'},f.a);
  const audit=only(f.db.prepare('SELECT count(*) n FROM records').get()).n;
  f.db.exec('BEGIN IMMEDIATE');
  let health: JsonRecord;
  // A reader that waited on the writer lock would hit the busy timeout and exit non-zero, failing exec.
  try {health=JSON.parse((await exec(binary,f.args('health'))).stdout);}
  finally {f.db.exec('ROLLBACK');}
  assert.equal(health.status,'clear');assert.equal(health.counts.waitingPassive,1);assert.equal(health.counts.attention,0);
  assert.deepEqual(health.issues,[]);assert.equal(health.next,undefined);
  assert.ok(!JSON.stringify(health).includes('private-context-never-in-health'));
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM records').get()).n,audit);
  assert.equal(only(f.db.prepare('SELECT acknowledgedAt FROM deliveries').get()).acknowledgedAt,null);
  f.call('send_message',{parts: [{ text: 'New action' }], replyRequired: true, to:f.b,reasoning:'Distinguish pending work from a fault'},f.a);
  const pending=f.call('health');assert.equal(pending.status,'pending');assert.equal(pending.counts.attention,0);
});
test('health reports uncertain, stalled and expired action work without replay or missing fanout pages',t=>{
  const f=fixture(t),c=f.call('join',{name:'third',vendor:'raw'}).id;
  const notice=f.call("send_message",{to: '*', parts: [{ text: 'Shared work' }], wake:'action',reasoning:'Validate operational visibility'},f.a);
  for(const [recipient,state] of [[f.b,'uncertain'],[c,'staged']])f.db.prepare('INSERT INTO dispatches(message,recipient,token,transport,state,attemptedAt) VALUES(?,?,?,\'raw\',?,?)').run(notice.id,recipient,recipient,state,Date.now()-10000);
  const first=f.call('health',{limit:1,staleAfterMs:1000});
  assert.equal(first.status,'attention');assert.equal(first.counts.attention,2);assert.equal(first.issues.length,1);
  const second=f.call(first.next.command,first.next.input);
  assert.equal(second.issues.length,1);assert.equal(second.next,undefined);
  assert.notEqual(second.issues[0].recipient,first.issues[0].recipient);
  assert.deepEqual([first.issues[0].issue,second.issues[0].issue].sort(),['stalledOffer','uncertain']);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM dispatches').get()).n,2);
  const old=f.call('send_message',{parts: [{ text: 'Expired work' }], replyRequired: true, to:f.b,reasoning:'Keep failed work visible',ttlMs:1000},f.a);
  f.db.prepare('UPDATE messages SET expiresAt=0 WHERE id=?').run(old.id);
  assert.ok(f.call('health').issues.some((row: JsonRecord)=>row.message===old.id&&row.issue==='expiredAction'));
});
test('health separates stale handling from offline recipients and isolates workspaces',t=>{
  const f=fixture(t);
  const message=f.call('send_message',{parts: [{ text: 'Please handle' }], replyRequired: true, to:f.b,reasoning:'Inspect submitted vs handled'},f.a);
  f.db.prepare("INSERT INTO dispatches(message,recipient,token,transport,state,attemptedAt,submittedAt) VALUES(?,?,?,'raw','submitted',?,?)").run(message.id,f.b,'receipt',Date.now()-10000,Date.now()-10000);
  assert.equal(f.call('health',{staleAfterMs:1000}).issues[0].issue,'overdueHandling');
  f.db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(f.b);
  assert.equal(f.call('health').issues[0].issue,'offlineRecipient');
  f.db.prepare('UPDATE sessions SET workspace=? WHERE id=?').run('/different-workspace',f.b);
  assert.equal(f.call('health').counts.unacknowledged,0);
});
test('health rejects invalid limits and never creates a missing DB',t=>{
  const f=fixture(t);
  for(const input of [{limit:0},{limit:101},{staleAfterMs:0},{after:{message:1}},{after:{message:0,recipient:'x'}}])assert.throws(()=>f.call('health',input));
  const absent=join(f.workspace,'missing.sqlite');
  assert.throws(()=>execFileSync(binary,['health','--workspace',f.workspace,'--database',absent],{stdio:'pipe',timeout:10000}));
  assert.equal(existsSync(absent),false);
});
test('health flags an ownerless staged offer promptly but not one held by a live delivery owner',async t=>{
  const f=fixture(t);
  f.call('attach',{transport:'raw'},f.b);
  const message=f.call('send_message',{parts: [{ text: 'Offered then crashed' }], replyRequired: true, to:f.b,reasoning:'Surface stranded offers'},f.a);
  f.db.prepare("INSERT INTO dispatches(message,recipient,token,transport,state,attemptedAt) VALUES(?,?,?,'hook:grok','staged',?)").run(message.id,f.b,'crashed',Date.now()-31000);
  const orphan=f.call('health');
  assert.equal(orphan.status,'attention');assert.equal(orphan.issues[0].issue,'stalledOffer');
  assert.equal(orphan.issues[0].submittedAt,undefined,'No nulls in issue rows');
    const listen=spawn(binary,['listen','--workspace',f.workspace,'--database',f.database,'--session',f.b]);
  t.after(()=>listen.kill('SIGKILL'));
  await new Promise((resolve,reject)=>{listen.stdout.on('data',resolve);listen.on('close',reject);});
  assert.deepEqual(f.call('health').issues,[],'A live owner keeps the full stale threshold');
  assert.equal(only(f.db.prepare('SELECT state FROM dispatches').get()).state,'staged','Health never replays');
});

// Probing a live owner spawns `ps` where /proc is unavailable; the probe runs once per recipient per health call.
test('health probes a recipient\'s delivery owner once however many of its offers are staged', { skip: process.platform === 'linux' || process.platform === 'win32' }, async t => {
  const f = storeFixture(t);
  const sender = await f.join('sender');
  const recipient = await f.join('recipient');
  await using lock = await runtime.ownerLock.claimDeliveryOwner(f.file, recipient);
  const insert = f.store.db.raw.prepare("INSERT INTO dispatches(message,recipient,token,transport,state,attemptedAt) VALUES(?,?,?,'raw','staged',?)");
  for (let i = 0; i < 4; i += 1) {
    const message = await f.call(sender, 'send_message', {parts: [{ text: `Offer ${i}` }], replyRequired: true,  to: recipient, key: `k${i}`, reasoning: 'Stage several offers for one recipient' });
    insert.run(message.id, recipient, `t${i}`, Date.now() - 60000);
  }
  const bin = mkdtempSync(join(tmpdir(), 'health-ps-'));
  t.after(() => rmSync(bin, { recursive: true, force: true }));
  const log = join(bin, 'calls');
  const realPs = execFileSync('which', ['ps'], { encoding: 'utf8' }).trim();
  writeFileSync(join(bin, 'ps'), `#!/bin/sh\necho x >> '${log}'\nexec '${realPs}' "$@"\n`, { mode: 0o755 });
  // The first run of a fresh script can stall past the probe's 2s limit under load, and a killed probe never reaches the log.
  assert.equal(spawnSync(join(bin, 'ps'), ['-p', String(process.pid)], { stdio: 'ignore', timeout: 30000 }).status, 0, 'the wrapper runs before the probe is measured');
  rmSync(log);
  const path = process.env.PATH;
  process.env.PATH = bin + ':' + path;
  t.after(() => { process.env.PATH = path; });
  const report = runtime.health.health(f.store, {});
  assert.deepEqual(report.issues, [], 'a live owner keeps the full stale threshold');
  assert.equal(report.counts.staged, 4);
  assert.equal(readFileSync(log, 'utf8').trim().split('\n').length, 1, 'one ps probe for one recipient');
  assert.ok(lock);
});
