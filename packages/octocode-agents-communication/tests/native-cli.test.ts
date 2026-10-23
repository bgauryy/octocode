// The native CLI and bound MCP server as black boxes: storage-free inspection, help, typed history, concurrent cold
// starts, MCP framing and identity, wait, read-only inspection and the packaged skill.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { JsonRecord } from './json-record.ts';
import { root } from './helpers.ts';
import { fixture, invoke, joinAgent, start } from './native-support.ts';

test('inspection, discovery and rejected commands do not create storage',t=>{
 const f=fixture(t);assert.equal(invoke(f,['db','info']).exists,false);
 assert.equal(invoke(f,['schema','types']).length,26);
 for(const args of [['unknown'],['peers'],['join','{"name":"bad"}'],['heartbeat','--session','missing']])assert.throws(()=>invoke(f,args));
 assert.equal(existsSync(dirname(f.database)),false);
});
test('help is compact, command-specific and discoverable without storage',t=>{
 const f=fixture(t),help=invoke(f,['--help']);
 assert.equal(help.implementation,'TypeScript');assert.ok(JSON.stringify(help).length<2000);
 assert.ok(help.commands.includes('run'));assert.ok(help.discover.includes('<command> --help'));
 for(const name of help.commands){
  const command=invoke(f,[...name.split(' '),'--help']);
  assert.equal(command.name,name);assert.equal(typeof command.description,'string');
  assert.equal(typeof command.usage,'string');assert.equal(command.commands,undefined);
  assert.deepEqual(invoke(f,['schema',...name.split(' ')]),command);
 }
 assert.equal(invoke(f,['schema','type','coordinate.in']).type,'coordinate.in');
 assert.throws(()=>invoke(f,['unknown','--help']));
 assert.equal(existsSync(dirname(f.database)),false);
});
test('CLI typed history, subscriptions, delivery status and generic resume',t=>{
 const f=fixture(t),a=joinAgent(f),b=joinAgent(f,'b');const call=(...args: string[])=>invoke(f,[...args,'--session',a.id]);
 call('heartbeat','{"name":"updated","vendorSession":"external"}');
 assert.equal(call('peers').items.find((x: JsonRecord)=>x.id===a.id).name,'updated');
 assert.throws(()=>call('entity','set','session',b.id,'{"name":"bad"}'));
 call('subscribe','{"topics":["build","build"]}');
 assert.equal(call('fetch','{"type":"subscription.added"}').items[0].data.topic,'build');
 const sent=call('send_message',JSON.stringify({to:b.id,body:'hello',key:'one'}));
 assert.equal(call('fetch',JSON.stringify({type:'message',from:a.id})).items.length,1);
 assert.equal(call('fetch',JSON.stringify({type:'delivery.acknowledged',where:{messageId:sent.id}})).items.length,0);
 invoke(f,['complete',JSON.stringify({message:sent.id}),'--session',b.id]);
 assert.equal(call('fetch',JSON.stringify({type:'delivery.acknowledged',where:{messageId:sent.id}})).items.length,1);
 const lock=call('lock','{"path":"src","kind":"tree"}');
 assert.equal(call('locks','{"path":"src/file"}').items[0].id,lock.lease.id);
 call('leave');assert.equal(call('resume','{"vendor":"generic"}').id,a.id);
 assert.equal(call('locks').items.length,0);
 assert.equal(call('fetch',JSON.stringify({type:'lease.removed',where:{leaseId:lock.lease.id}})).items.length,1);
});
test('eight cold CLI processes initialize one database and acquire one lease',async t=>{
 const f=fixture(t);
 const joins=await Promise.all(Array.from({length:8},(_,i)=>start(f,['join',JSON.stringify({name:String(i),vendor:'generic'})])));
 for(const result of joins)assert.equal(result.code,0,result.stderr);
 const agents: JsonRecord[]=joins.map(r=>JSON.parse(r.stdout));assert.equal(invoke(f,['peers']).items.length,8);
 const locks=await Promise.all(agents.map((a,i)=>start(f,['lock',JSON.stringify({path:i%2?'Contended':'contended'}),'--session',a.id])));
 for(const result of locks)assert.equal(result.code,0,result.stderr);
 assert.equal(locks.filter(r=>JSON.parse(r.stdout).ok).length,1);
});
test('MCP rejects an oversized frame, ignores blank lines and keeps serving queued requests',async t=>{
 const f=fixture(t),a=joinAgent(f);
 const oversized=JSON.stringify({jsonrpc:'2.0',id:1,method:'ping',params:{pad:'x'.repeat(8*1024*1024)}});
 const input=['',oversized,'   ',JSON.stringify({jsonrpc:'2.0',id:2,method:'ping'})].join('\n')+'\n';
 const result=await start(f,['mcp','--session',a.id],input);assert.equal(result.code,0,result.stderr);
 const rows=result.stdout.trim().split('\n').map((line): JsonRecord=>JSON.parse(line));
 assert.equal(rows.length,2);
 assert.equal(rows[0].error.code,-32600);assert.equal(rows[0].id,null);
 assert.deepEqual(rows[1],{jsonrpc:'2.0',id:2,result:{}});
});
test('MCP binds identity and survives malformed frames',async t=>{
 const f=fixture(t),a=joinAgent(f);
 const frames=[null,'INVALID',...[
  {jsonrpc:'2.0',id:1,method:'initialize'},
  {jsonrpc:'2.0',id:2,method:'tools/list'},
  {jsonrpc:'2.0',id:3,method:'tools/call',params:{name:'lock',arguments:{path:'file',reasoning:'Reserve a fixture path to verify bound identity'}}},
  {jsonrpc:'2.0',id:4,method:'tools/call',params:{name:'lock',arguments:{path:'file',owner:'other'}}},
  {jsonrpc:'2.0',id:5,method:'tools/call',params:{name:'join',arguments:{name:'hidden',vendor:'bad'}}},
  {jsonrpc:'2.0',id:6,method:'tools/call',params:{name:'context',arguments:{path:'.'}}},
  {jsonrpc:'2.0',id:7,method:'tools/call',params:{name:'locks',arguments:{}}},
 ].map(frame=>JSON.stringify(frame))].map(v=>v===null?'null':v).join('\n')+'\n';
 const result=await start(f,['mcp','--session',a.id],frames);assert.equal(result.code,0,result.stderr);
 const rows=result.stdout.trim().split('\n').map((line): JsonRecord=>JSON.parse(line));
 assert.equal(rows[0].error.code,-32600);assert.equal(rows[1].error.code,-32700);
 assert.equal(rows[3].result.tools.length,18);assert.equal(JSON.parse(rows[4].result.content[0].text).lease.path,'file');assert.equal(invoke(f,['locks','{"path":"file"}','--session',a.id]).items[0].owner,a.id);
 const catalog=invoke(f,['schema']);
 assert.deepEqual(rows[3].result.tools,catalog.tools);
 for(const tool of catalog.tools){
  const command=invoke(f,[tool.name,'--help']);
  assert.equal(tool.description,command.description);
  assert.deepEqual(tool.inputSchema,command.inputSchema);
 }
 assert.equal(rows[5].result.isError,true);assert.equal(rows[6].result.isError,true);
 assert.deepEqual(JSON.parse(rows[7].result.content[0].text).items,[]);
 const leases=JSON.parse(rows[8].result.content[0].text).items;assert.equal(leases.length,1);assert.equal(leases[0].owner,a.id);assert.ok(leases[0].refreshedAt<=leases[0].expiresAt);
});
test('CLI wait receives a message and acknowledges only when requested',async t=>{
 const f=fixture(t),a=joinAgent(f),b=joinAgent(f,'b');
 const waiting=start(f,['inbox','wait','{"timeoutMs":3000}','--session',b.id]);
 const sent=invoke(f,['send_message',JSON.stringify({to:b.id,body:'wake'}),'--session',a.id]);
 const result=await waiting;assert.equal(result.code,0,result.stderr);assert.equal(JSON.parse(result.stdout).items[0].id,sent.id);
 assert.equal(invoke(f,['inbox','--session',b.id]).items.length,1);
});
test('read-only inspection rejects schema changes without repairing them',t=>{
 const f=fixture(t);joinAgent(f);const db=new DatabaseSync(f.database);db.exec('DROP TABLE subscriptions; PRAGMA wal_checkpoint(TRUNCATE)');db.close();
 const before=readFileSync(f.database);assert.throws(()=>joinAgent(f));assert.deepEqual(readFileSync(f.database),before);
 assert.equal(invoke(f,['db','info']).compatible,false);
});
test('the skill command returns the packaged instructions, bounded, with the installed reference root',t=>{
 const f=fixture(t);
 const response=invoke(f,['skill']);
 assert.equal(response.instructions,readFileSync(join(root,'OPERATING.md'),'utf8'));
 // The compact skill routes setup, hooks and the command catalog to references under docs/.
 assert.ok(Buffer.byteLength(response.instructions)<=30000);
 assert.equal(response.referenceRoot,join(response.packageRoot,'docs'));
 for(const name of readdirSync(response.referenceRoot))assert.ok(readFileSync(join(response.referenceRoot,name),'utf8').length,name);
});
