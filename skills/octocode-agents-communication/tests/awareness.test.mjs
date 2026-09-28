import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.mjs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, tempWorkspace, jsonCall } from './helpers.mjs';
import { registerBoundTools } from '../scripts/pi-extension.mjs';

function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-awareness-'), database = join(workspace, 'db.sqlite');
  const call = jsonCall(binary, workspace, database, { stdio: 'pipe' });
  const hook = (event = 'postToolUse', extra = {}) => JSON.parse(execFileSync(binary,
    ['host-hook', '--vendor', 'cursor', '--workspace', workspace, '--database', database],
    { encoding: 'utf8', input: JSON.stringify({hook_event_name:event, conversation_id:'host', workspace_roots:[workspace], ...extra}) }));
  return {workspace, database, call, hook};
}

test('directory exposes declared task/status and updates without changing identity', t => {
  const f = fixture(t);
  const a = f.call('join', {name:'reviewer',vendor:'generic',task:'Review database migrations',status:'busy'});
  assert.equal(f.call('peers',{}).items[0].task, 'Review database migrations');
  f.call('heartbeat', {task:'Review messaging',status:'available'}, a.id);
  const peer = f.call('peers',{}).items[0];
  assert.equal(peer.id,a.id); assert.equal(peer.task,'Review messaging'); assert.equal(peer.status,'available');
  assert.throws(()=>f.call('heartbeat',{status:'invented'},a.id));
});

test('hooks announce directory changes once, ignore heartbeats, restore after compaction', t => {
  const f=fixture(t); f.hook('sessionStart');
  const a=f.call('join',{name:'reviewer',vendor:'generic',task:'Review SQL',status:'busy'});
  const joined=f.hook().additional_context;
  assert.ok(joined.includes(a.id)); assert.ok(joined.includes('Review SQL'));
  assert.deepEqual(f.hook(),{});
  f.call('heartbeat',{},a.id); assert.deepEqual(f.hook(),{});
  f.call('heartbeat',{task:'Review hooks',status:'available'},a.id);
  assert.ok(f.hook().additional_context.includes('Review hooks'));
  assert.deepEqual(f.hook(),{});
  assert.ok(f.hook('postToolUse',{context_generation:'compacted'}).additional_context.includes(a.id));
  f.call('leave',{},a.id);
  assert.ok(f.hook('postToolUse',{context_generation:'compacted'}).additional_context.includes(a.id));
  assert.deepEqual(f.hook('postToolUse',{context_generation:'compacted'}),{});
});

test('peer expiry generates an update without another peer action', t => {
  const f=fixture(t);f.hook('sessionStart');const a=f.call('join',{name:'peer',vendor:'generic'});
  f.hook();const db=new DatabaseSync(f.database);t.after(()=>db.close());
  db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now()+80,a.id);
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,100);
  assert.ok(f.hook().additional_context.includes(a.id));assert.deepEqual(f.hook(),{});
});

test('MCP retries reuse the connection request key; new calls remain distinct', t => {
  const f=fixture(t), a=f.call('join',{name:'sender',vendor:'generic'}), b=f.call('join',{name:'receiver',vendor:'generic'});
  const frame=(id,body='Review SQL')=>({jsonrpc:'2.0',id,method:'tools/call',params:{name:'send_message',arguments:{to:b.id,body,reasoning:'Coordinate review'}}});
  const input=[frame(1),frame(1),frame(1,'Changed intent'),frame(2)].map(JSON.stringify).join('\n')+'\n';
  const rows=execFileSync(binary,['mcp','--workspace',f.workspace,'--database',f.database,'--session',a.id],{encoding:'utf8',input}).trim().split('\n').map(JSON.parse);
  const result=i=>JSON.parse(rows[i].result.content[0].text);
  assert.equal(result(0).id,result(1).id);assert.equal(rows[2].result.isError,true);
  assert.notEqual(result(0).id,result(3).id);assert.equal(f.call('inbox',{},b.id).items.length,2);
});

test('Pi re-executions reuse host call IDs, including across bridge registration', async t => {
  const f=fixture(t), a=f.call('join',{name:'sender',vendor:'generic'}), b=f.call('join',{name:'receiver',vendor:'generic'});
  const tools=JSON.parse(execFileSync(binary,['schema','tools','--tools','send_message'],{encoding:'utf8'}));
  const bind=()=>{let tool;registerBoundTools({registerTool:t=>{tool=t;}},{binary,workspace:f.workspace,database:f.database,session:a.id,tools});return tool;};
  const input={to:b.id,body:'Review SQL',reasoning:'Coordinate review'};
  const first=await bind().execute('call-a',input);
  const retry=await bind().execute('call-a',input);
  const later=await bind().execute('call-b',input);
  assert.equal(first.details.id,retry.details.id);assert.notEqual(first.details.id,later.details.id);
  await assert.rejects(bind().execute('call-a',{...input,body:'Different request'}));
  assert.equal(f.call('inbox',{},b.id).items.length,2);
});

test('bounded directory updates give a continuation and signal changes outside the first page', t => {
  const f=fixture(t);f.hook('sessionStart');
  for(let i=0;i<20;i++) f.call('join',{name:`peer-${i}`,vendor:'generic',task:'x'.repeat(256)});
  const context=f.hook().additional_context;assert.ok(context.length<9000);assert.ok(context.includes('"next"'));
  const peers=f.call('peers',{}).items.filter(x=>x.vendor==='generic');
  f.call('heartbeat',{task:'Changed outside first page'},peers.at(-1).id);
  const changed=f.hook().additional_context;assert.ok(changed.includes('"refresh"'));assert.deepEqual(f.hook(),{});
});
