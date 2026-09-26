import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdtempSync,rmSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {DatabaseSync} from 'node:sqlite';
const binary=fileURLToPath(new URL('../skills/octocode-agents-communication/scripts/agents-communication',import.meta.url));
const exec=promisify(execFile);
function fixture(t) {
  const workspace=mkdtempSync(join(tmpdir(),'communication-health-')),database=join(workspace,'db.sqlite');
  t.after(()=>rmSync(workspace,{recursive:true,force:true}));
  const args=(name,input={},session)=>[name,JSON.stringify(input),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])];
  const call=(...a)=>JSON.parse(execFileSync(binary,args(...a),{encoding:'utf8',stdio:'pipe',timeout:10000}));
  const a=call('join',{name:'sender',vendor:'raw'}).id,b=call('join',{name:'recipient',vendor:'other'}).id;
  const db=new DatabaseSync(database);t.after(()=>db.close());
  return {workspace,database,args,call,a,b,db};
}
test('health stays read-only under a writer, omits bodies, and treats passive waiting as normal',async t=>{
  const f=fixture(t);
  f.call('send_message',{to:f.b,body:'private-context-never-in-health',wake:'passive',reasoning:'Inform without waking'},f.a);
  const audit=f.db.prepare('SELECT count(*) n FROM audit').get().n;
  f.db.exec('BEGIN IMMEDIATE');
  let health;
  try {health=JSON.parse((await exec(binary,f.args('health'),{timeout:1500})).stdout);}
  finally {f.db.exec('ROLLBACK');}
  assert.equal(health.status,'clear');assert.equal(health.counts.waitingPassive,1);assert.equal(health.counts.attention,0);
  assert.deepEqual(health.issues,[]);assert.equal(health.next,undefined);
  assert.ok(!JSON.stringify(health).includes('private-context-never-in-health'));
  assert.equal(f.db.prepare('SELECT count(*) n FROM audit').get().n,audit);
  assert.equal(f.db.prepare('SELECT acknowledgedAt FROM deliveries').get().acknowledgedAt,null);
  f.call('send_message',{to:f.b,body:'New action',reasoning:'Distinguish pending work from a fault'},f.a);
  const pending=f.call('health');assert.equal(pending.status,'pending');assert.equal(pending.counts.attention,0);
});
test('health reports uncertain, stalled and expired action work without replay or missing fanout pages',t=>{
  const f=fixture(t),c=f.call('join',{name:'third',vendor:'raw'}).id;
  const notice=f.call('notify_all',{body:'Shared work',wake:'action',reasoning:'Validate operational visibility'},f.a);
  for(const [recipient,state] of [[f.b,'uncertain'],[c,'staged']])f.db.prepare('INSERT INTO dispatches(message,recipient,token,transport,state,attemptedAt) VALUES(?,?,?,\'raw\',?,?)').run(notice.id,recipient,recipient,state,Date.now()-10000);
  const first=f.call('health',{limit:1,staleAfterMs:1000});
  assert.equal(first.status,'attention');assert.equal(first.counts.attention,2);assert.equal(first.issues.length,1);
  const second=f.call(first.next.command,first.next.input);
  assert.equal(second.issues.length,1);assert.equal(second.next,undefined);
  assert.notEqual(second.issues[0].recipient,first.issues[0].recipient);
  assert.deepEqual([first.issues[0].issue,second.issues[0].issue].sort(),['stalledOffer','uncertain']);
  assert.equal(f.db.prepare('SELECT count(*) n FROM dispatches').get().n,2);
  const old=f.call('send_message',{to:f.b,body:'Expired work',reasoning:'Keep failed work visible',ttlMs:1000},f.a);
  f.db.prepare('UPDATE messages SET expiresAt=0 WHERE id=?').run(old.id);
  assert.ok(f.call('health').issues.some(row=>row.message===old.id&&row.issue==='expiredAction'));
});
test('health separates stale handling from offline recipients and isolates workspaces',t=>{
  const f=fixture(t);
  const message=f.call('send_message',{to:f.b,body:'Please handle',reasoning:'Inspect submitted vs handled'},f.a);
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
  const message=f.call('send_message',{to:f.b,body:'Offered then crashed',reasoning:'Surface stranded offers'},f.a);
  f.db.prepare("INSERT INTO dispatches(message,recipient,token,transport,state,attemptedAt) VALUES(?,?,?,'hook:grok','staged',?)").run(message.id,f.b,'crashed',Date.now()-31000);
  const orphan=f.call('health');
  assert.equal(orphan.status,'attention');assert.equal(orphan.issues[0].issue,'stalledOffer');
  assert.equal(orphan.issues[0].submittedAt,undefined,'No nulls in issue rows');
  const {spawn}=await import('node:child_process');
  const listen=spawn(binary,['listen','--workspace',f.workspace,'--database',f.database,'--session',f.b]);
  t.after(()=>listen.kill('SIGKILL'));
  await new Promise((resolve,reject)=>{listen.stdout.on('data',resolve);listen.on('close',reject);});
  assert.deepEqual(f.call('health').issues,[],'A live owner keeps the full stale threshold');
  assert.equal(f.db.prepare('SELECT state FROM dispatches').get().state,'staged','Health never replays');
});
