import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

const root=fileURLToPath(new URL('../',import.meta.url));
const target=execFileSync('rustc',['-vV'],{encoding:'utf8'}).match(/^host: (.+)$/m)[1];
const binary=join(root,'skills/octocode-agents-communication/scripts/bin',target,`octocode-agents-communication${process.platform==='win32'?'.exe':''}`);
function fixture(t,initialize=true){
 const workspace=realpathSync(mkdtempSync(join(tmpdir(),'communication-intent-'))),database=join(workspace,'v1.sqlite');
 t.after(()=>rmSync(workspace,{recursive:true,force:true}));
 const call=(command,input,session)=>JSON.parse(execFileSync(binary,[...command.split(' '),...(input===undefined?[]:[JSON.stringify(input)]),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])],{encoding:'utf8',stdio:['pipe','pipe','pipe'],timeout:10000}));
 const a=initialize?call('join',{name:'editor',vendor:'raw'}).id:null;
 const b=initialize?call('join',{name:'reviewer',vendor:'any-vendor'}).id:null;
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
 assert.equal(f.call('hook',{format:'json'},f.b).items[0].reasoning,reasoning);
 assert.equal(f.call('inbox',{},f.b).items[0].reasoning,reasoning);
 const broadcast=f.call('notify_all',{body:'Review the rename plan',reasoning},f.a);
 assert.ok(f.call('hook',{format:'claude'},f.b).hookSpecificOutput.additionalContext.includes(reasoning));
 assert.equal(f.call(`entity get message ${broadcast.id}`,undefined,f.a).reasoning,reasoning);
 const owned=f.call('lock',{path:'src',kind:'tree',reasoning:'Rename the API and its imports together'},f.a).lease;
 const conflict=f.call('lock',{path:'src/api',reasoning:'Fix the API regression before release'},f.b);
 assert.equal(conflict.conflict.reasoning,owned.reasoning);
 assert.equal(conflict.next.input.reasoning,'Fix the API regression before release');
 const request=f.call(conflict.next.command,conflict.next.input,f.b);
 assert.equal(f.call(conflict.next.command,conflict.next.input,f.b).id,request.id);
 const changed=f.call('lock',{path:'src/api',reasoning:'Investigate an unrelated API defect'},f.b);
 assert.notEqual(changed.next.input.key,conflict.next.input.key);
 f.call('renew',{lease:owned.id},f.a);f.call('unlock',{lease:owned.id},f.a);
 const leases=f.call('lock_many',{paths:[{path:'src/old'},{path:'src/new'}],reasoning:'Reserve both endpoints of the rename'},f.b).leases;
 assert.ok(leases.every(lease=>lease.reasoning==='Reserve both endpoints of the rename'));
 const db=new DatabaseSync(f.database);t.after(()=>db.close());
 for(const kind of ['lease.acquired','lease.renewed','lease.removed'])assert.equal(JSON.parse(db.prepare('SELECT data FROM audit WHERE kind=? AND entityId=?').get(kind,String(owned.id)).data).reasoning,owned.reasoning);
 assert.equal(JSON.parse(db.prepare("SELECT data FROM audit WHERE kind='message.created' AND entityId=?").get(String(sent.id)).data).reasoning,reasoning);
});
test('raw SQLite cannot omit or rewrite intent; v2 migration preserves unknown historical intent',t=>{
 const f=fixture(t,false),db=new DatabaseSync(f.database);t.after(()=>db.close());
 db.exec(readFileSync(join(root,'rust/schema-v1.sql'),'utf8')+readFileSync(join(root,'rust/schema-v2.sql'),'utf8'));
 db.exec('PRAGMA application_id=1329678147; PRAGMA user_version=2; PRAGMA journal_mode=WAL');
 db.prepare('INSERT INTO sessions VALUES(?,?,?,?,?,?)').run('old',f.workspace,'old','raw',null,Date.now()+60000);
 db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt) VALUES(?,?,?,?,?)').run('old','old','Historical body','historical',Date.now()+3600000);
 db.prepare('INSERT INTO leases(workspace,path,kind,owner,expiresAt) VALUES(?,?,?,?,?)').run(f.workspace,join(f.workspace,'old-path'),'file','old',Date.now()+60000);
 assert.throws(()=>f.call('db migrate'));
 db.prepare('UPDATE sessions SET expiresAt=0').run();
 assert.deepEqual(f.call('db migrate'),{schemaVersion:6,migrated:true});
 assert.deepEqual(f.call('db migrate'),{schemaVersion:6,migrated:false});
 assert.equal(db.prepare('SELECT reasoning FROM messages').get().reasoning,null);
 assert.equal(db.prepare('SELECT reasoning FROM leases').get().reasoning,null);
 assert.equal(db.prepare('SELECT body FROM messages').get().body,'Historical body');
 for(const reasoning of [null,'','\t\n','\u2003','x'.repeat(513)]){
  assert.throws(()=>db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning) VALUES(?,?,?,?,?,?)').run('old','old','body','bad',0,reasoning));
  assert.throws(()=>db.prepare('INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning) VALUES(?,?,?,?,?,?)').run(f.workspace,'new','file','old',0,reasoning));
 }
 assert.throws(()=>db.prepare('UPDATE messages SET reasoning=?').run('Invented later'));
 assert.throws(()=>db.prepare('UPDATE leases SET reasoning=?').run('Invented later'));
});
