import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.mjs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { nativeBinary as binary, tempWorkspace } from './helpers.mjs';

function fixture(t){
 const workspace = tempWorkspace(t, 'communication-intent-', { real: true }),database=join(workspace,'communication.sqlite');
 const call=(command,input,session)=>JSON.parse(execFileSync(binary,[...command.split(' '),...(input===undefined?[]:[JSON.stringify(input)]),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])],{encoding:'utf8',stdio:['pipe','pipe','pipe'],timeout:10000}));
 const a=call('join',{name:'editor',vendor:'raw'}).id;
 const b=call('join',{name:'reviewer',vendor:'any-vendor'}).id;
 return {workspace,database,call,a,b};
}
test('messages and single/bundled locks require explicit nonblank bounded reasoning',t=>{
 const f=fixture(t);
 for(const [command,input] of [['send_message',{to:f.b,body:'Review src/api'}],['notify_all',{body:'Renaming shared API'}],['lock',{path:'src/api'}],['lock_many',{paths:[{path:'src/old'},{path:'src/new'}]}]]){
  for(const reasoning of [undefined,null,'',' \t\n','\u2003\u00a0','x'.repeat(513),'😀'.repeat(129)]){
   assert.throws(()=>f.call(command,{...input,...(reasoning===undefined?{}:{reasoning})},f.a),undefined,`${command} accepted invalid reasoning`);
  }
 }
 assert.throws(()=>f.call('send_message',{to:f.b,body:'x',reasoning:'é'.repeat(300)},f.a),e=>e.stderr.trim()==='Invalid reasoning: 600 UTF-8 bytes exceeds 512');
 const db=new DatabaseSync(f.database);t.after(()=>db.close());
 assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n,0);
 assert.equal(db.prepare('SELECT count(*) n FROM leases').get().n,0);
});
test('intent survives delivery, audit, lease conflict and keyed owner questions',t=>{
 const f=fixture(t),reasoning='Review the API change before the rename can proceed';
 f.call('attach',{transport:'raw'},f.b);
 const sent=f.call('send_message',{to:f.b,body:'Please review handoff.md',reasoning,key:'review'},f.a);
 assert.equal(f.call('send_message',{to:f.b,body:'Please review handoff.md',reasoning,key:'review'},f.a).id,sent.id);
 assert.throws(()=>f.call('send_message',{to:f.b,body:'Please review handoff.md',reasoning:'A different intent',key:'review'},f.a));
 const hooked=f.call('hook',{format:'json'},f.b);assert.equal(hooked.items[0].reasoning,undefined);assert.ok(hooked.context.includes(reasoning));
 assert.equal(f.call('inbox',{},f.b).items[0].reasoning,reasoning);
 const broadcast=f.call('notify_all',{body:'Review the rename plan',reasoning},f.a);
 assert.ok(f.call('hook',{format:'claude'},f.b).hookSpecificOutput.additionalContext.includes(reasoning));
 assert.equal(f.call('fetch',{type:'message',where:{messageId:broadcast.id}},f.a).items[0].data.reasoning,reasoning);
 const owned={...f.call('lock',{path:'src',kind:'tree',reasoning:'Rename the API and its imports together'},f.a).lease,reasoning:'Rename the API and its imports together'};
 const conflict=f.call('lock',{path:'src/api',reasoning:'Fix the API regression before release'},f.b);
 assert.equal(conflict.conflict.reasoning,owned.reasoning);
 assert.equal(conflict.next.input.reasoning,'Fix the API regression before release');
 const request=f.call(conflict.next.command,conflict.next.input,f.b);
 assert.equal(f.call(conflict.next.command,conflict.next.input,f.b).wait.id,request.wait.id);
 assert.match(f.call('inbox',{},f.a).items[0].body,/Fix the API regression before release/,'the owner learns the waiter intent');
 const changed=f.call('lock',{path:'src/api',reasoning:'Investigate an unrelated API defect'},f.b);
 assert.equal(changed.next.input.reasoning,'Investigate an unrelated API defect');
 f.call('renew',{leaseId:owned.id},f.a);f.call('unlock',{leaseId:owned.id},f.a);
 const leases=f.call('lock_many',{paths:[{path:'src/old'},{path:'src/new'}],reasoning:'Reserve both endpoints of the rename'},f.b).leases;
 const dbLeases=new DatabaseSync(f.database,{readOnly:true});
 assert.ok(leases.every(lease=>dbLeases.prepare('SELECT reasoning FROM leases WHERE id=?').get(lease.id).reasoning==='Reserve both endpoints of the rename'));dbLeases.close();
 const db=new DatabaseSync(f.database);t.after(()=>db.close());
 for(const kind of ['lease.acquired','lease.renewed','lease.removed'])assert.equal(JSON.parse(db.prepare('SELECT data FROM records WHERE type=? AND entityId=?').get(kind,String(owned.id)).data).reasoning,owned.reasoning);
 assert.equal(JSON.parse(db.prepare("SELECT data FROM records WHERE type='message' AND entityId=?").get(String(sent.id)).data).reasoning,reasoning);
});
test('raw SQLite cannot omit or rewrite intent',t=>{
 const f=fixture(t),db=new DatabaseSync(f.database);t.after(()=>db.close());
 f.call('send_message',{to:f.b,body:'Current intent',reasoning:'Review the current change'},f.a);
 f.call('lock',{path:'owned',reasoning:'Reserve the current edit'},f.a);
 for(const reasoning of [null,'','\t\n','\u2003','x'.repeat(513)]){
  assert.throws(()=>db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning) VALUES(?,?,?,?,?,?)').run(f.a,f.b,'body','bad',0,reasoning));
  assert.throws(()=>db.prepare('INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning,pathKey) VALUES(?,?,?,?,?,?,?)').run(f.workspace,'new','file',f.a,0,reasoning,'/new'));
 }
 assert.throws(()=>db.prepare('UPDATE messages SET reasoning=?').run('Invented later'));
 assert.throws(()=>db.prepare('UPDATE leases SET reasoning=?').run('Invented later'));
});
