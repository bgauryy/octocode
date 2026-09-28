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
 const expected={session:'id',lease:'owner',message:'sender',delivery:'recipient',subscriptions:'session',attachment:'session',dispatch:'recipient',audit:'session'};
 assert.deepEqual(Object.fromEntries(catalog.entities.map(e=>[e.name,e.agentIdField])),expected);
 for(const action of ['get','list']){
  const command=catalog.commands.find(c=>c.name===`entity ${action}`);
  assert.deepEqual(command.inputSchema.properties.entity.enum,Object.keys(expected));
 }
 assert.deepEqual(catalog.commands.find(c=>c.name==='entity set').inputSchema.properties.entity.enum,
  catalog.entities.filter(e=>e.set).map(e=>e.name));
 for(const entity of catalog.entities){
  assert.ok(entity.fields[entity.agentIdField]);
  const rows=call(`entity list ${entity.name}`,{},a).items;assert.ok(rows.length,entity.name);
  for(const row of rows){
   assert.ok([a,b].includes(row[entity.agentIdField]),`${entity.name} missing valid agent identity`);
   assert.equal(row.agentId,undefined,'Do not duplicate role IDs');
   for(const field of Object.keys(row))assert.ok(field in entity.fields,`${entity.name}.${field} missing from discovery`);
  }
 }
 const db=new DatabaseSync(database);t.after(()=>db.close());
 assert.equal(db.prepare('SELECT a.session FROM documents d JOIN audit a ON a.id=d.id WHERE d.name=?').get('context.md').session,a);
 assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(),[]);
 assert.equal(catalog.database.applicationId,db.prepare('PRAGMA application_id').get().application_id);
 assert.equal(catalog.database.schemaVersion,db.prepare('PRAGMA user_version').get().user_version);
 const relationships=catalog.database.relationships;
 assert.deepEqual(relationships.find(r=>r.table==='dispatches'),{
  table:'dispatches',columns:['message','recipient'],
  references:{table:'deliveries',columns:['message','recipient'],entity:'delivery'},onDelete:'NO ACTION'
 });
 assert.ok(relationships.some(r=>r.table==='messages'&&r.columns[0]==='replyTo'&&r.references.entity==='message'));
 assert.ok(relationships.some(r=>r.table==='documents'&&r.references.entity==='audit'));
 for(const entity of catalog.entities)assert.deepEqual(entity.relationships,relationships.filter(r=>r.table===entity.table));
 assert.throws(()=>db.prepare("INSERT INTO attachments VALUES('missing-session','raw',NULL,1)").run(),/FOREIGN KEY/);
 assert.throws(()=>db.prepare("INSERT INTO dispatches VALUES(999,'missing-recipient','token','raw','staged',1,NULL,NULL)").run(),/FOREIGN KEY/);
});
