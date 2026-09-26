import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

const testInput=(command,input)=>['send_message','notify_all','lock','lock_many'].includes(command)?{reasoning:`Verify ${command} behavior in this isolated regression fixture`,...input}:input;

const root=fileURLToPath(new URL('../',import.meta.url));
const target=execFileSync('rustc',['-vV'],{encoding:'utf8'}).match(/^host: (.+)$/m)[1];
const binary=join(root,'skills/octocode-agents-communication/scripts/bin',target,`octocode-agents-communication${process.platform==='win32'?'.exe':''}`);
function fixture(t){
 const workspace=mkdtempSync(join(tmpdir(),'communication-leases-')), database=join(workspace,'v1.sqlite');
 t.after(()=>rmSync(workspace,{recursive:true,force:true}));
 const args=(session,name,input={})=>[...name.split(' '),JSON.stringify(testInput(name,input)),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])];
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
 const f=fixture(t),old=f.call(f.a,'lock',{path:'closed',ttlMs:86400000}).lease;
 f.call(f.a,'leave');assert.equal(f.call(f.b,'lock',{path:'closed'}).ok,true);
 f.call(f.a,'resume',{vendor:'raw'});assert.equal(f.call(f.a,'renew',{leaseId:old.id}).renewed,false);
 f.call(f.a,'lock',{path:'crashed',ttlMs:86400000});
 const db=new DatabaseSync(f.database);db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now()-1,f.a);db.close();
 assert.equal(f.call(f.b,'lock',{path:'crashed'}).ok,true);assert.throws(()=>f.call(f.a,'heartbeat'));
 const cleanup=f.call(f.b,'prune');assert.ok(cleanup.removed>=1);
 assert.equal(f.call(f.b,'entity list audit').items.length>0,true);
});
