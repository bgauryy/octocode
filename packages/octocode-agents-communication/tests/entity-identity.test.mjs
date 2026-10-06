import {test} from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.mjs';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {binary,tempWorkspace,withReasoning} from './helpers.mjs';

test('every public entity exposes its declared agent identity and documents preserve authorship',t=>{
 const workspace=tempWorkspace(t,'communication-entities-',{real:true}),database=join(workspace,'audit.sqlite');
 const call=(command,input={},session)=>JSON.parse(execFileSync(binary,[...command.split(' '),JSON.stringify(withReasoning(command,input)),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])],{encoding:'utf8',stdio:'pipe'}));
 const a=call('join',{name:'author',vendor:'raw'}).id,b=call('join',{name:'recipient',vendor:'raw'}).id;
 call('lock',{path:'fixture',reasoning:'Verify owner'},a);
 call('attach',{transport:'raw'},b);
 call('subscribe',{topics:['review']},b);
 call('send_message',{to:b,body:'Inspect fixture',reasoning:'Verify sender/recipient'},a);
 call('hook',{format:'json'},b);
 const document=call('share_document',{name:'context.md',content:'Evidence',reasoning:'Verify author',context:{summary:'Fixture',path:'.'}},a).document;
 assert.equal(document.author,a);
 assert.equal(call('read_document',{name:'context.md'},b).document.author,a);
 assert.equal(call('context',{path:'.'},b).items[0].author,a);
 const catalog=JSON.parse(execFileSync(binary,['schema'],{encoding:'utf8'}));
 const types=catalog.recordTypes.map(e=>e.type);
 assert.ok(types.includes('message'));assert.ok(types.includes('memory'));assert.ok(types.includes('coordinate.in'));
 assert.equal(catalog.entities,undefined);
 assert.ok(!catalog.commands.some(command=>command.name.startsWith('entity ')));
 const rows=call('fetch',{},a).items;
 for(const row of rows){
  assert.ok([a,b].includes(row.from));assert.equal(row.path,workspace);
  assert.ok(types.includes(row.type),row.type);
  for(const field of Object.keys(row))assert.ok(field in catalog.recordEnvelope.properties,field);
 }
 const db=new DatabaseSync(database);t.after(()=>db.close());
 assert.equal(db.prepare('SELECT r.[from] author FROM documents d JOIN records r ON r.id=d.id WHERE d.name=?').get('context.md').author,a);
 assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(),[]);
 assert.equal(catalog.database.applicationId,db.prepare('PRAGMA application_id').get().application_id);
 assert.equal(catalog.database.schemaVersion,db.prepare('PRAGMA user_version').get().user_version);
 assert.ok(catalog.database.relationships.some(r=>r.table==='documents'&&r.references.table==='records'));
 assert.throws(()=>db.prepare("INSERT INTO attachments VALUES('missing-session','raw',NULL,1)").run(),/Record path must match its author workspace/);
 assert.throws(()=>db.prepare("INSERT INTO dispatches VALUES(999,'missing-recipient','token','raw','staged',1,NULL,NULL)").run(),/Record path must match its author workspace/);
});
