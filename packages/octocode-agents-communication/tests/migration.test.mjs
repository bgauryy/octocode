import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,existsSync} from 'node:fs';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {binary,execFileSync,tempWorkspace} from './helpers.mjs';

function fixture(t){
 const workspace=tempWorkspace(t,'communication-migrate-',{real:true}),database=join(workspace,'v1.sqlite');
 const db=new DatabaseSync(database);
 db.exec(readFileSync(new URL('./fixtures/schema-v1.sql',import.meta.url),'utf8'));
 db.prepare('INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES(?,?,?,?,?)').run('author',workspace,'Author','claude',Date.now()+600000);
 db.prepare('INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES(?,?,?,?,?)').run('recipient',workspace,'Recipient','grok',Date.now()+600000);
 const message=Number(db.prepare("INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,replyRequired) VALUES('author','recipient','Retained evidence','v1-key',?,'Migrate without loss',0)").run(Date.now()+600000).lastInsertRowid);
 db.prepare("INSERT INTO deliveries(message,recipient) VALUES(?,'recipient')").run(message);
 const data={name:'old.md',author:'author',path:'.octocode/communication/old.md',bytes:8,sha256:'fixture',reasoning:'Retain publication'};
 db.prepare("INSERT INTO audit(session,kind,entityId,at,data,key) VALUES('author','document.created','old.md',?,?,'old.md')").run(Date.now(),JSON.stringify(data));
 const count=db.prepare('SELECT count(*) n FROM audit').get().n;
 db.close();
 const call=(name,input,session)=>JSON.parse(execFileSync(binary,[...name.split(' '),JSON.stringify(input),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])],{encoding:'utf8',stdio:'pipe'}));
 return {workspace,database,message,count,call};
}
test('v1 migration preserves all evidence and IDs, publishes a verified backup, and serves the new CLI',t=>{
 const f=fixture(t),backup=join(f.workspace,'backup.sqlite');
 assert.throws(()=>f.call('fetch',{},'recipient'),/Incompatible/);
 const result=f.call('db migrate',{backup});assert.equal(result.records,f.count);assert.equal(result.integrity,'ok');
 const db=new DatabaseSync(f.database);t.after(()=>db.close());
 assert.equal(db.prepare('SELECT count(*) n FROM records').get().n,f.count);
 assert.equal(db.prepare('SELECT body FROM messages WHERE id=?').get(f.message).body,'Retained evidence');
 assert.equal(db.prepare("SELECT name FROM documents WHERE name='old.md'").get().name,'old.md');
 assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(),[]);
 const old=new DatabaseSync(backup,{readOnly:true});t.after(()=>old.close());
 assert.equal(old.prepare('PRAGMA user_version').get().user_version,1);
 assert.equal(old.prepare('SELECT count(*) n FROM audit').get().n,f.count);
 const mail=f.call('fetch',{incoming:true,search:'Retained'},'recipient').items[0];
 assert.equal(mail.data.messageId,f.message);assert.equal(mail.from,'author');assert.equal(mail.to,'recipient');
 f.call('complete',{message:f.message},'recipient');
 assert.equal(f.call('fetch',{type:'delivery.acknowledged'},'author').items.length,1);
 assert.throws(()=>f.call('db migrate',{backup}),/already exists/);
});
test('unknown v1 schema is rejected before backup or mutation',t=>{
 const f=fixture(t),db=new DatabaseSync(f.database);db.exec('CREATE TABLE unexpected(id INTEGER)');db.close();
 const before=readFileSync(f.database);
 assert.throws(()=>f.call('db migrate',{backup:join(f.workspace,'backup.sqlite')}),/recognized v1, v2, v3 or v4 schema/);
 assert.deepEqual(readFileSync(f.database),before);
});
test('conversion failure rolls back v1 schema and rows while retaining the verified backup',t=>{
 const f=fixture(t),backup=join(f.workspace,'backup.sqlite');
 const db=new DatabaseSync(f.database);
 db.prepare("INSERT INTO audit(session,kind,entityId,at,data) VALUES('author','message.created','999999',?,'{}')").run(Date.now());
 const before=db.prepare('SELECT * FROM audit ORDER BY id').all();db.close();
 assert.throws(()=>f.call('db migrate',{backup}),/missing message.*rolled back/);
 assert.equal(existsSync(backup),true);
 for(const path of [f.database,backup]){
  const restored=new DatabaseSync(path,{readOnly:true});
  assert.equal(restored.prepare('PRAGMA user_version').get().user_version,1);
  assert.deepEqual(restored.prepare('SELECT * FROM audit ORDER BY id').all(),before);
  assert.equal(restored.prepare('SELECT count(*) n FROM messages').get().n,1);
  assert.deepEqual(restored.prepare('PRAGMA foreign_key_check').all(),[]);restored.close();
 }
});

test('v2 migration preserves every record and state exactly while new snapshots are complete',t=>{
 const workspace=tempWorkspace(t,'communication-v2-migrate-',{real:true}),database=join(workspace,'v2.sqlite'),backup=join(workspace,'backup.sqlite');
 const db=new DatabaseSync(database);
 db.exec(readFileSync(new URL('./fixtures/schema-v2.sql',import.meta.url),'utf8'));
 db.prepare('INSERT INTO sessions(id,workspace,name,vendor,branch,expiresAt) VALUES(?,?,?,?,?,?)').run('author',workspace,'Original','claude','old-branch',Date.now()+600000);
 db.prepare("UPDATE sessions SET name='Renamed',task='Original work',status='busy' WHERE id='author'").run();
 const before=db.prepare('SELECT * FROM records ORDER BY id').all(),state=db.prepare('SELECT * FROM sessions').all();db.close();
 const call=(name,input={})=>JSON.parse(execFileSync(binary,[...name.split(' '),JSON.stringify(input),'--workspace',workspace,'--database',database,'--session','author'],{encoding:'utf8',stdio:'pipe'}));
 assert.throws(()=>call('fetch'),/Incompatible/);
 const result=call('db migrate',{backup});assert.equal(result.schemaVersion,5);assert.equal(result.sourceSchemaVersion,2);assert.equal(result.records,before.length);
 const migrated=new DatabaseSync(database);t.after(()=>migrated.close());
 assert.deepEqual(migrated.prepare('SELECT * FROM records ORDER BY id').all(),before);
 assert.deepEqual(migrated.prepare('SELECT * FROM sessions').all(),state);
 assert.deepEqual(migrated.prepare('PRAGMA foreign_key_check').all(),[]);
 const old=new DatabaseSync(backup,{readOnly:true});t.after(()=>old.close());
 assert.equal(old.prepare('PRAGMA user_version').get().user_version,2);
 assert.deepEqual(old.prepare('SELECT * FROM records ORDER BY id').all(),before);
 call('set_status',{task:'New work',status:'available'});
 const profile=call('fetch',{type:'coordinate.profile'}).items.at(-1);
 assert.equal(profile.data.name,'Renamed');assert.equal(profile.data.vendor,'claude');assert.equal(profile.data.task,'New work');assert.ok(profile.data.expiresAt>Date.now());
 assert.equal(call('fetch',{type:'coordinate.in'}).items[0].data.name,'Original');
 assert.equal(call('fetch',{type:'coordinate.profile'}).items[0].data.vendor,undefined,'historical identity must not be invented');
});
test('v4 migration adds the lease wait queue and keeps every row and live lease',t=>{
 const workspace=tempWorkspace(t,'communication-v4-migrate-',{real:true}),database=join(workspace,'v4.sqlite'),backup=join(workspace,'backup.sqlite');
 const db=new DatabaseSync(database);
 db.exec(readFileSync(new URL('./fixtures/schema-v4.sql',import.meta.url),'utf8'));
 db.prepare('INSERT INTO workspaces VALUES(?,?)').run(workspace,workspace);
 for(const id of ['owner','waiter'])db.prepare('INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES(?,?,?,?,?)').run(id,workspace,id,'generic',Date.now()+600000);
 db.prepare("INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning,pathKey) VALUES(?,?,'file','owner',?,'Owner edit',?)").run(workspace,join(workspace,'a.ts'),Date.now()+300000,join(workspace,'a.ts').toLowerCase());
 const before=db.prepare('SELECT * FROM records ORDER BY id').all(),leases=db.prepare('SELECT * FROM leases').all();db.close();
 const call=(name,input={},session='waiter')=>JSON.parse(execFileSync(binary,[...name.split(' '),JSON.stringify(input),'--workspace',workspace,'--database',database,'--session',session],{encoding:'utf8',stdio:'pipe'}));
 assert.throws(()=>call('fetch'),/Incompatible/);
 const result=call('db migrate',{backup});assert.equal(result.schemaVersion,5);assert.equal(result.sourceSchemaVersion,4);
 const migrated=new DatabaseSync(database);t.after(()=>migrated.close());
 assert.deepEqual(migrated.prepare('SELECT * FROM records ORDER BY id').all().slice(0,before.length),before);
 assert.deepEqual(migrated.prepare('SELECT * FROM leases').all(),leases);
 assert.equal(migrated.prepare("SELECT count(*) n FROM sqlite_schema WHERE name IN ('lease_waits','lease_waits_order')").get().n,2);
 assert.equal(call('lock',{path:'a.ts',reasoning:'Queue after migration',wait:true}).queued,true);
});
test('unknown v2 schema is rejected without publishing a backup',t=>{
 const workspace=tempWorkspace(t,'communication-v2-unknown-',{real:true}),database=join(workspace,'v2.sqlite'),backup=join(workspace,'backup.sqlite');
 const db=new DatabaseSync(database);db.exec(readFileSync(new URL('./fixtures/schema-v2.sql',import.meta.url),'utf8'));db.exec('CREATE INDEX unexpected ON sessions(name)');db.close();
 const before=readFileSync(database);
 assert.throws(()=>execFileSync(binary,['db','migrate',JSON.stringify({backup}),'--workspace',workspace,'--database',database],{encoding:'utf8',stdio:'pipe'}),/recognized v1, v2, v3 or v4 schema/);
 assert.deepEqual(readFileSync(database),before);assert.equal(existsSync(backup),false);
});
