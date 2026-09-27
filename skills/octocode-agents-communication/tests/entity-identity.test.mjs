import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
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
 for(const entity of catalog.entities){
  assert.ok(entity.fields[entity.agentIdField]);
  const rows=call(`entity list ${entity.name}`,{},a).items;assert.ok(rows.length,entity.name);
  for(const row of rows){assert.ok([a,b].includes(row[entity.agentIdField]),`${entity.name} missing valid agent identity`);assert.equal(row.agentId,undefined,'Do not duplicate role IDs');}
 }
 const db=new DatabaseSync(database);t.after(()=>db.close());
 assert.equal(db.prepare('SELECT a.session FROM documents d JOIN audit a ON a.id=d.id WHERE d.name=?').get('context.md').session,a);
 assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(),[]);
});
