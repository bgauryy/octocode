import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync,spawnSync} from 'node:child_process';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {binary,tempWorkspace} from './helpers.mjs';

function fixture(t){
 const workspace=tempWorkspace(t,'communication-status-'),database=join(workspace,'state.sqlite');
 const flags=['--workspace',workspace,'--database',database];
 const call=(name,input={},id)=>JSON.parse(execFileSync(binary,[name,JSON.stringify(input),...flags,...(id?['--session',id]:[])],{encoding:'utf8',stdio:'pipe'}));
 const a=call('join',{name:'worker',vendor:'test',task:'review',status:'busy'}).id;
 const b=call('join',{name:'peer',vendor:'test'}).id;
 const db=new DatabaseSync(database);t.after(()=>db.close());
 return {a,b,call,db,flags};
}
test('set_status changes only bound task/status, preserves presence/leases and rejects impersonation',t=>{
 const {a,b,call,db}=fixture(t);
 call('lock',{path:'code.rs',reasoning:'Review guarded edit'},a);
 const before=db.prepare('SELECT * FROM sessions WHERE id=?').get(a);
 const lease=db.prepare('SELECT * FROM leases WHERE owner=?').get(a);
 assert.deepEqual(call('set_status',{status:'blocked'},a),{id:a,task:'review',status:'blocked'});
 assert.deepEqual(call('set_status',{task:''},a),{id:a,task:'',status:'blocked'});
 const after=db.prepare('SELECT * FROM sessions WHERE id=?').get(a);
 assert.deepEqual({...after,task:before.task,status:before.status},{...before});
 assert.deepEqual(db.prepare('SELECT * FROM leases WHERE owner=?').get(a),lease);
 for(const input of [{},{status:'oops'},{status:'busy',id:b},{status:'busy',session:b},{vendor:'other'},{ttlMs:60000},{task:null}]){
  assert.throws(()=>call('set_status',input,a),/Command failed/);
 }
 assert.equal(db.prepare('SELECT status FROM sessions WHERE id=?').get(b).status,'unknown');
 db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(a);
 assert.throws(()=>call('set_status',{status:'available'},a),/expired session/);
 assert.equal(db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(a).expiresAt,0);
});
test('MCP-only worker can declare a blocker and profile discovery exposes precisely selected tools',t=>{
 const {a,b,flags,call}=fixture(t);
 const expected={messaging:['peers','set_status','send_message','inbox','complete'],review:['peers','set_status','send_message','inbox','complete','share_document','read_document','context'],editing:['peers','set_status','send_message','inbox','complete','share_document','read_document','context','locks','lock','lock_many','renew','unlock']};
 for(const [profile,names] of Object.entries(expected)){
  const frames=[{id:1,method:'initialize',params:{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'status-test',version:'1'}}},{id:2,method:'tools/list'}, {id:3,method:'tools/call',params:{name:'set_status',arguments:{status:'blocked',task:'Need review evidence'}}},{id:4,method:'tools/call',params:{name:'set_status',arguments:{status:'available',id:b}}}].map(x=>JSON.stringify({jsonrpc:'2.0',...x})).join('\n')+'\n';
  const child=spawnSync(binary,['mcp','--session',a,'--tools',profile,...flags],{input:frames,encoding:'utf8',timeout:10000});
  assert.equal(child.status,0,child.stderr);
  const replies=child.stdout.trim().split('\n').map(JSON.parse);
  assert.deepEqual(replies[1].result.tools.map(x=>x.name).sort(),names.sort());
  assert.deepEqual(JSON.parse(replies[2].result.content[0].text),{id:a,status:'blocked',task:'Need review evidence'});
  assert.ok(replies[3].error || replies[3].result.isError);
  assert.equal(call('peers',{},a).items.find(x=>x.id===a).status,'blocked');
 }
});
