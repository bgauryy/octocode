import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from './helpers.mjs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { nativeBinary as binary, tempWorkspace, withReasoning } from './helpers.mjs';

function fixture(t){
 const workspace = tempWorkspace(t, 'communication-leases-', {real:true}), database=join(workspace,'communication.sqlite');
 const args=(session,name,input={})=>[...name.split(' '),JSON.stringify(withReasoning(name,input)),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])];
 const call=(session,name,input)=>JSON.parse(execFileSync(binary,args(session,name,input),{encoding:'utf8',stdio:['pipe','pipe','pipe']}));
 const parallel=(session,name,input)=>new Promise((resolve,reject)=>{const child=spawn(binary,args(session,name,input));let out='',err='';child.stdout.on('data',b=>out+=b);child.stderr.on('data',b=>err+=b);child.on('error',reject);child.on('close',code=>code?reject(Error(err)):resolve(JSON.parse(out)));});
 const a=call(null,'join',{name:'a',vendor:'raw'}).id,b=call(null,'join',{name:'b',vendor:'raw'}).id;
 return {workspace,database,call,parallel,a,b};
}
test('atomic competing path sets have one winner, no partial reservation, and owner handoff',async t=>{
 const f=fixture(t),paths=[{path:'source'},{path:'destination'}];
 const results=await Promise.all([f.parallel(f.a,'lock_many',{paths}),f.parallel(f.b,'lock_many',{paths:[...paths].reverse()})]);
 assert.equal(results.filter(r=>r.ok).length,1);
 const winner=results.find(r=>r.ok),loser=results.find(r=>!r.ok),owner=results[0].ok?f.a:f.b,waiter=owner===f.a?f.b:f.a;
 assert.equal(winner.leases.length,2);assert.deepEqual(winner.leases.map(l=>l.path).sort(),['destination','source']);assert.equal(loser.owner.id,owner);assert.deepEqual(loser.heldLeaseIds,[]);assert.ok(loser.retryAfterMs>0&&loser.retryAfterMs<=60000);
 assert.equal(f.call(waiter,'entity list lease').items.length,2);
 const first=f.call(waiter,loser.next.command,loser.next.input),again=f.call(waiter,loser.next.command,loser.next.input);assert.equal(first.id,again.id);
 for(const lease of winner.leases)assert.equal(f.call(owner,'unlock',{leaseId:lease.id}).released,true);
 assert.equal(f.call(waiter,'lock_many',{paths}).ok,true);
});
test('conflict reports held leases; internal alias/parent overlap fails without acquiring anything',t=>{
 const f=fixture(t),held=f.call(f.a,'lock',{path:'held'}).lease;
 f.call(f.b,'lock',{path:'blocked',kind:'tree'});
 const denied=f.call(f.a,'lock_many',{paths:[{path:'free'},{path:'blocked/child'}]});
 assert.equal(denied.ok,false);assert.deepEqual(denied.heldLeaseIds,[held.id]);
 assert.equal(f.call(f.b,'lock',{path:'free'}).ok,true);
 const self=f.call(f.a,'lock',{path:'held'});assert.equal(self.next,undefined);assert.equal(self.conflict.path,'held');assert.equal(self.owner.id,f.a);
 for(const paths of [[{path:'X'},{path:'x'}],[{path:'tree',kind:'tree'},{path:'tree/file'}],[],[{path:'../escape'}]])assert.throws(()=>f.call(f.a,'lock_many',{paths}));
});
test('stalled owner lease expires without heartbeat renewing it; stale ID cannot affect new owner',async t=>{
 const f=fixture(t),old=f.call(f.a,'lock',{path:'stalled',ttlMs:1000}).lease;
 f.call(f.a,'heartbeat');assert.equal(f.call(f.b,'lock',{path:'stalled'}).ok,false);
 await new Promise(resolve=>setTimeout(resolve,1100));
 const fresh=f.call(f.b,'lock',{path:'stalled'}).lease;assert.ok(fresh.id>old.id);
 assert.equal(f.call(f.a,'renew',{leaseId:old.id}).renewed,false);assert.equal(f.call(f.a,'unlock',{leaseId:old.id}).released,false);
 assert.equal(f.call(f.b,'renew',{leaseId:fresh.id}).renewed,true);
});
test('closed or expired presence frees long leases; resume cannot resurrect old ownership',t=>{
 const f=fixture(t),old=f.call(f.a,'lock',{path:'closed',ttlMs:600000}).lease;
 f.call(f.a,'leave');assert.equal(f.call(f.b,'lock',{path:'closed'}).ok,true);
 f.call(f.a,'resume',{vendor:'raw'});assert.equal(f.call(f.a,'renew',{leaseId:old.id}).renewed,false);
 f.call(f.a,'lock',{path:'crashed',ttlMs:600000});
 const db=new DatabaseSync(f.database);db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now()-1,f.a);db.close();
 assert.equal(f.call(f.b,'lock',{path:'crashed'}).ok,true);assert.throws(()=>f.call(f.a,'heartbeat'));
 const cleanup=f.call(f.b,'prune');assert.ok(cleanup.removed>=1);
 assert.equal(f.call(f.b,'entity list audit').items.length>0,true);
});

test('locks exposes owner, intent and timestamps; ten-minute cap applies to acquisition and renewal',t=>{
 const f=fixture(t),reasoning='Edit parser with a covering reservation';
 const lease=f.call(f.a,'lock',{path:'parser.rs',reasoning,ttlMs:600000}).lease;
 assert.equal(lease.owner,f.a);assert.equal(lease.reasoning,reasoning);
 assert.equal(lease.expiresAt-lease.refreshedAt,600000);assert.equal(lease.acquiredAt,lease.refreshedAt);
 const listed=f.call(f.b,'locks').items[0];
 for(const key of ['owner','reasoning','acquiredAt','refreshedAt','expiresAt'])assert.equal(listed[key],lease[key]);
 const conflict=f.call(f.b,'lock',{path:'parser.rs',reasoning:'Need parser next'});
 assert.equal(conflict.ok,false);assert.equal(conflict.owner.id,f.a);assert.equal(conflict.conflict.reasoning,reasoning);
 assert.equal(conflict.conflict.refreshedAt,lease.refreshedAt);assert.equal(conflict.next.input.to,f.a);
 const sent=f.call(f.b,conflict.next.command,conflict.next.input);assert.ok(sent.id);
 assert.equal(f.call(f.b,'check_write',{paths:[{path:'parser.rs'}]}).ok,false);
 for(const [command,input] of [['lock',{path:'too-long'}],['lock_many',{paths:[{path:'too-long'}]}],['renew',{leaseId:lease.id}]])assert.throws(()=>f.call(f.a,command,{...input,ttlMs:600001}));
 const renewed=f.call(f.a,'renew',{leaseId:lease.id,ttlMs:600000});assert.equal(renewed.renewed,true);
 const updated=f.call(f.b,'locks').items[0];assert.equal(updated.acquiredAt,lease.acquiredAt);assert.ok(updated.refreshedAt>lease.refreshedAt);assert.equal(updated.expiresAt-updated.refreshedAt,600000);
 const db=new DatabaseSync(f.database);assert.throws(()=>db.prepare('UPDATE leases SET expiresAt=refreshedAt+600001 WHERE id=?').run(lease.id));db.close();
 assert.equal(f.call(f.a,'unlock',{leaseId:lease.id}).released,true);
 assert.equal(f.call(f.b,'lock',{path:'parser.rs',reasoning:'Agreed handoff'}).ok,true);
});

test('workspace locks paginates every live lease and ignores stale locks and expired owners',t=>{
 const f=fixture(t),db=new DatabaseSync(f.database),at=Date.now();
 const insert=db.prepare("INSERT INTO leases(workspace,path,kind,owner,acquiredAt,refreshedAt,expiresAt,reasoning,pathKey) VALUES(?,?,'file',?,?,?,?,?,?)");
 const add=(workspace,path,owner,stamp,expiry)=>Number(insert.run(workspace,join(workspace,path),owner,stamp,stamp,expiry,'Reserved for editing',join(workspace,path).toLowerCase()).lastInsertRowid);
 const ids=[];for(let i=0;i<105;i++)ids.push(add(f.workspace,`file-${i}`,f.a,at,at+600000));
 const old=add(f.workspace,'stale',f.a,at-600001,at-1);
 add(f.workspace,'expired-owner',f.b,at,at+600000);
 db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(at-1,f.b);
 const other=tempWorkspace(t,'communication-other-scope-', {real:true});
 const foreign=JSON.parse(execFileSync(binary,['join','{"name":"foreign","vendor":"raw"}','--workspace',other,'--database',f.database],{encoding:'utf8'})).id;
 add(other,'foreign',foreign,at,at+600000);
 const seen=[];let input={},pages=0;
 do {const page=f.call(f.a,'locks',input);seen.push(...page.items.map(x=>x.id));pages++;if(!page.next)break;assert.equal(page.next.command,'locks');input=page.next.input;assert.ok(pages<20);}while(true);
 assert.ok(pages>1);assert.deepEqual(seen,ids);assert.ok(!seen.includes(old));
 assert.equal(f.call(f.a,'check_write',{paths:[{path:'stale'}]}).ok,false);
 assert.equal(f.call(f.a,'renew',{leaseId:old}).renewed,false);
 assert.equal(f.call(f.a,'lock',{path:'stale'}).ok,true);
 db.close();
});


test('locks reads without competing with a held SQLite writer or changing history',t=>{
 const f=fixture(t);f.call(f.a,'lock',{path:'read-only'});
 const db=new DatabaseSync(f.database),before=db.prepare('SELECT count(*) n FROM audit').get().n;
 db.exec('BEGIN IMMEDIATE');
 try {assert.equal(f.call(f.b,'locks').items.length,1);} finally {db.exec('ROLLBACK');}
 assert.equal(db.prepare('SELECT count(*) n FROM audit').get().n,before);db.close();
});
