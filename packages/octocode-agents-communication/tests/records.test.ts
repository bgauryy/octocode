import {test} from 'node:test';
import type {TestContext} from 'node:test';
import assert from 'node:assert/strict';
import {join} from 'node:path';
import {readFileSync,existsSync} from 'node:fs';
import {DatabaseSync} from 'node:sqlite';
import type { JsonRecord } from './json-record.ts';
import {binary,execFileSync,only,tempWorkspace,withReasoning,root} from './helpers.ts';

const runtime=await import('../dist/runtime.js');

function fixture(t: TestContext, git = false) {
 const workspace=tempWorkspace(t,'communication-records-',{real:true}),database=join(workspace,'records.sqlite');
 if(git)execFileSync('git',['init','--quiet',workspace],{stdio:'pipe'});
 const call=(name: string,input: Record<string,unknown>={},session?: string): JsonRecord=>JSON.parse(execFileSync(binary,[...name.split(' '),JSON.stringify(withReasoning(name,input)),'--workspace',workspace,'--database',database,...(session?['--session',session]:[])],{encoding:'utf8',stdio:'pipe'}));
 const a=call('join',{name:'author',vendor:'claude',branch:'feature/unified'}).id;
 const b=call('join',{name:'recipient',vendor:'grok',...(git?{branch:null}:{})}).id;
 const c=call('join',{name:'observer',vendor:'codex'}).id;
 return {workspace,database,call,a,b,c};
}
test('one typed envelope covers coordination, mail, leases, documents and generic JSON',t=>{
 const f=fixture(t);
 const message=f.call('send_message',{to:f.b,body:'Review shared records',conversationId:'records:1'},f.a);
 f.call('subscribe',{topics:['review']},f.b);
 const lease=f.call('lock',{path:'src',kind:'tree'},f.a).lease;
 f.call('renew',{leaseId:lease.id,ttlMs:120000},f.a);
 f.call('unlock',{leaseId:lease.id},f.a);
 f.call('share_document',{name:'evidence.md',content:'Evidence'},f.a);
 const memory=f.call('record',{type:'memory',data:{content:'Remember unicode paths',nested:{nil:null},tags:['paths']},key:'memory:1'},f.a);
 assert.ok(Number.isSafeInteger(memory.recordId));
 assert.equal(f.call('record',{type:'memory',data:memory.data,key:'memory:1'},f.a).recordId,memory.recordId);
 const rows: JsonRecord[]=f.call('fetch',{},f.a).items;
 for(const row of rows){
  assert.equal(row.path,f.workspace);assert.ok([f.a,f.b,f.c].includes(row.from));
  assert.ok(Object.hasOwn(row,'to'));assert.equal(typeof row.type,'string');
  assert.equal(typeof row.timestamp,'number');assert.ok(Object.hasOwn(row,'data'));
 }
 assert.ok(rows.some(r=>r.type==='coordinate.in'));
 const mail=rows.find(r=>r.type==='message');
 assert.ok(mail);
 assert.equal(mail.to,f.b);assert.equal(mail.data.messageId,message.id);assert.equal(mail.data.body,'Review shared records');
 assert.equal(mail.branch,'feature/unified');
 for(const type of ['subscription.added','lease.acquired','lease.renewed','lease.removed','document','memory'])assert.ok(rows.some(r=>r.type===type),type);
 assert.equal(memory.data.nested.nil,null);
 assert.equal(f.call('fetch',{type:'message'},f.c).items.length,0,'mail visibility cannot be bypassed through the event log');
 assert.equal(f.call('fetch',{type:'message',branch:'feature/unified',where:{conversationId:'records:1'}},f.b).items.length,1);
 assert.equal(f.call('fetch',{type:'message',where:{body:'Review shared records'}},f.b).items.length,1);
 assert.equal(f.call('fetch',{search:'unicode'},f.b).items[0].recordId,memory.recordId);
 assert.equal(f.call('fetch',{type:'memory',where:{'nested.nil':null}},f.b).items[0].recordId,memory.recordId);
 assert.equal(f.call('fetch',{recordId:memory.recordId},f.b).items[0].data.content,'Remember unicode paths');
 assert.ok(rows.every(r=>!Object.hasOwn(r,'id')),'history envelopes expose recordId only');
 assert.throws(()=>f.call('record',{type:'message',data:{body:'bypass'}},f.a),/dedicated|not an allowed/);
 assert.throws(()=>f.call('record',{type:'memory',data:{}},f.a),/content|required/);
});
test('typed data queries distinguish null, missing, false and zero; branch can be cleared',t=>{
 const f=fixture(t);
 const input={type:'event',key:'typed',data:{name:'facts',value:false,nullable:null}};
 const row=f.call('record',input,f.a);
 assert.equal(f.call('fetch',{type:'event',where:{value:false}},f.b).items[0].recordId,row.recordId);
 assert.equal(f.call('fetch',{type:'event',where:{value:0}},f.b).items.length,0);
 assert.equal(f.call('fetch',{type:'event',where:{nullable:null}},f.b).items.length,1);
 assert.equal(f.call('fetch',{type:'event',where:{missing:null}},f.b).items.length,0);
 assert.throws(()=>f.call('record',{...input,data:{...input.data,value:0}},f.a),/different/);
 f.call('heartbeat',{branch:null},f.a);
 assert.equal(f.call('record',{type:'event',data:{name:'cleared'}},f.a).branch,undefined);
 assert.equal(f.call('record',{type:'event',data:{name:'override'},branch:'other'},f.a).branch,'other');
 assert.throws(()=>f.call('record',{type:'event',data:{name:'invalid'},to:'missing'},f.a),/Unknown/);
});
test('current dispatch fetch excludes superseded attempts and completed stages',t=>{
 const f=fixture(t);
 const mail=f.call('send_message',{to:f.b,body:'Handoff'},f.a);
 f.call('attach',{transport:'raw'},f.b);
 f.call('hook',{format:'json',deferConfirm:true,consumer:'fixture'},f.b);
 const first=f.call('fetch',{type:'dispatch.staged',current:true,from:f.b},f.b).items[0];
 f.call('retry_delivery',{message:mail.id,reason:'Confirmed no context receipt'},f.b);
 assert.equal(f.call('fetch',{type:'dispatch.staged',current:true},f.b).items.length,0);
 f.call('hook',{format:'json',deferConfirm:true,consumer:'fixture'},f.b);
 const second=f.call('fetch',{type:'dispatch.staged',current:true,from:f.b},f.b).items[0];
 assert.notEqual(second.data.token,first.data.token);
 assert.equal(second.data.transport,'raw:fixture');
 assert.equal(f.call('fetch',{type:'dispatch.staged',from:f.b},f.b).items.length,2);
 f.call('confirm_delivery',{items:[{id:mail.id,dispatchToken:second.data.token}]},f.b);
 assert.equal(f.call('fetch',{type:'dispatch.staged',current:true},f.b).items.length,0);
 assert.equal(f.call('fetch',{type:'dispatch.submitted',current:true},f.b).items.length,1);
});
test('record fetch preserves filters and a fixed high-water mark across every page',t=>{
 const f=fixture(t);
 for(let i=0;i<9;i++)f.call('record',{type:'event',data:{name:'checkpoint',index:i},to:f.b},f.a);
 let result=f.call('fetch',{type:'event',from:f.a,to:f.b,limit:2,where:{name:'checkpoint'}},f.b),rows=[...result.items];
 f.call('record',{type:'event',data:{name:'checkpoint',index:99},to:f.b},f.a);
 while(result.next){assert.equal(result.next.command,'fetch');result=f.call('fetch',result.next.input,f.b);rows.push(...result.items);}
 assert.deepEqual(rows.map(r=>r.data.index),[0,1,2,3,4,5,6,7,8]);
 assert.equal(new Set(rows.map(r=>r.recordId)).size,9);
 assert.equal(f.call('fetch',{type:'event'},f.c).items.length,0);
 const db=new DatabaseSync(f.database);t.after(()=>db.close());
 assert.equal(only(db.prepare("SELECT count(*) n FROM sqlite_schema WHERE name='audit'").get()).n,0);
 assert.throws(()=>db.exec('DELETE FROM records'),/append-only/);
 const plan: JsonRecord[]=db.prepare('EXPLAIN QUERY PLAN SELECT id FROM records WHERE path=? AND type=? AND id>? ORDER BY id LIMIT 10').all(f.workspace,'event',0);
 assert.ok(plan.some(row=>row.detail.includes('records_type')),JSON.stringify(plan));
 assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(),[]);
});
test('all 26 operational and generic payloads conform to the discovered envelope and type schemas',t=>{
 const f=fixture(t);
 f.call('heartbeat',{branch:'changed',name:'renamed'},f.a);
 f.call('set_status',{task:'Validate payload contracts',status:'busy'},f.a);
 f.call('subscribe',{topics:['schema']},f.b);f.call('subscribe',{topics:[]},f.b);
 const lease=f.call('lock',{path:'owned'},f.a).lease;
 f.call('renew',{leaseId:lease.id,ttlMs:120000},f.a);f.call('unlock',{leaseId:lease.id},f.a);
 f.call('share_document',{name:'typed.md',content:'Proof',context:{summary:'Typed document',branch:'changed'}},f.a);
 f.call('record',{type:'memory',data:{content:'Retain generic data',path:'x'.repeat(5000)}},f.a);
 f.call('record',{type:'event',data:{name:'checkpoint',nested:{value:null}}},f.a);
 f.call('record',{type:'checkpoint',data:{goal:'Validate payload contracts',done:['schemas'],next:'Check envelopes'}},f.a);
 f.call('record_usage',{key:'usage',scope:'turn',inputTokens:7},f.a);
 f.call('attach',{transport:'raw'},f.b);f.call('attach',{transport:'raw'},f.b);
 const mail=f.call('send_message',{to:f.b,body:'Contract evidence'},f.a);
 const db=new DatabaseSync(f.database);
 db.prepare("UPDATE deliveries SET claimedBy='fixture',claimUntil=0 WHERE message=?").run(mail.id);
 db.prepare('UPDATE deliveries SET claimedBy=NULL WHERE message=?').run(mail.id);
 f.call('hook',{format:'json',deferConfirm:true,consumer:'schemas'},f.b);
 db.prepare("UPDATE dispatches SET state='uncertain',error='Receipt unknown' WHERE message=?").run(mail.id);
 f.call('retry_delivery',{message:mail.id,reason:'Confirmed receipt absent'},f.b);
 f.call('hook',{format:'json',deferConfirm:true,consumer:'schemas'},f.b);
 const staged=f.call('fetch',{type:'dispatch.staged',current:true},f.b).items[0];
 f.call('confirm_delivery',{items:[{id:mail.id,dispatchToken:staged.data.token}]},f.b);
 f.call('complete',{message:mail.id},f.b);
 execFileSync(binary,['host-hook','--vendor','claude','--workspace',f.workspace,'--database',f.database],{
  input:JSON.stringify({hook_event_name:'SessionStart',session_id:'schemas-host',cwd:f.workspace,permission_mode:'default'}),encoding:'utf8'});
 f.call('leave',{},f.a);db.close();
 const {catalog,database,records,validation}=runtime,schema=catalog.catalog(),reader=database.openDatabase(f.database,true),seen=new Set();
 try{
  for(const row of database.query(reader,records.RECORD_SELECT)){
   const record=records.envelope(row);
   validation.validate(validation.asObject(schema.recordEnvelope,'recordEnvelope'),record);
   validation.validate(validation.asObject(catalog.recordType(record.type).dataSchema,'dataSchema'),record.data);
   seen.add(record.type);
  }
 }finally{reader.close();}
 assert.deepEqual([...seen].sort(),schema.recordTypes.map((t: JsonRecord)=>t.type).sort());
});
test('bound MCP exposes discriminated memory/event/checkpoint schemas and preserves generic JSON on the wire',t=>{
 const f=fixture(t),data={content:'Wire proof',nullable:null,nested:{boolean:false},path:'x'.repeat(5000)};
 const frames=[
  {jsonrpc:'2.0',id:1,method:'tools/list'},
  {jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'record',arguments:{type:'memory',data}}},
  {jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'record',arguments:{type:'memory',data}}},
  {jsonrpc:'2.0',id:3,method:'tools/call',params:{name:'fetch',arguments:{type:'memory',where:{nullable:null}}}},
  {jsonrpc:'2.0',id:4,method:'tools/call',params:{name:'record',arguments:{type:'event',data:{content:'Missing name'}}}},
 ];
 const responses=execFileSync(binary,['mcp','--tools','fetch,record','--session',f.a,'--workspace',f.workspace,'--database',f.database],{
  input:frames.map(frame=>JSON.stringify(frame)).join('\n')+'\n',encoding:'utf8',stdio:'pipe'}).trim().split('\n').map((line): JsonRecord=>JSON.parse(line));
 const schema=responses[0].result.tools.find((tool: JsonRecord)=>tool.name==='record').inputSchema;
 assert.deepEqual(schema.oneOf.map((item: JsonRecord)=>item.properties.type.const),['memory','event','checkpoint']);
 const record=JSON.parse(responses[1].result.content[0].text);
 assert.equal(record.to,null);assert.deepEqual(record.data,data);
 assert.deepEqual(responses[1],responses[2]);
 assert.deepEqual(JSON.parse(responses[3].result.content[0].text).items,[record]);
 assert.equal(responses[4].result.isError,true);assert.match(responses[4].result.content[0].text,/name|required/);
 assert.equal(f.call('fetch',{type:'memory'},f.a).items.length,1);
});
test('Git and host joins capture optional branch names; heartbeat updates or clears them',t=>{
 const f=fixture(t,true);
 assert.equal(f.call('fetch',{type:'coordinate.in',from:f.b},f.b).items[0].branch,undefined);
 execFileSync('git',['-C',f.workspace,'symbolic-ref','HEAD','refs/heads/feature/captured'],{stdio:'pipe'});
 const identity=f.call('join',{name:'branch-agent',vendor:'generic'}).id;
 assert.equal(f.call('fetch',{type:'coordinate.in',from:identity},identity).items[0].branch,'feature/captured');
 execFileSync(binary,['host-hook','--vendor','codex','--workspace',f.workspace,'--database',f.database],{
  input:JSON.stringify({hook_event_name:'SessionStart',session_id:'branch-host',cwd:f.workspace}),encoding:'utf8'});
 const db=new DatabaseSync(f.database,{readOnly:true});t.after(()=>db.close());
 assert.equal(only(db.prepare("SELECT branch FROM sessions WHERE vendorSession='branch-host'").get()).branch,'feature/captured');
 execFileSync('git',['-C',f.workspace,'symbolic-ref','HEAD','refs/heads/feature/switched'],{stdio:'pipe'});
 assert.equal(f.call('record',{type:'event',data:{name:'before-update'}},identity).branch,'feature/captured');
 f.call('heartbeat',{branch:'feature/switched'},identity);
 assert.equal(f.call('record',{type:'event',data:{name:'after-update'}},identity).branch,'feature/switched');
 f.call('heartbeat',{branch:null},identity);
 assert.equal(f.call('record',{type:'event',data:{name:'cleared'}},identity).branch,undefined);
});

test('coordination snapshots retain event-time identity and leave uses the same workspace agent',t=>{
 const f=fixture(t);
 f.call('heartbeat',{name:'new-name',branch:'next-branch'},f.a);
 f.call('set_status',{task:'Review schema',status:'blocked'},f.a);
 const db=new DatabaseSync(f.database);t.after(()=>db.close());
 const before=f.call('fetch',{from:f.a,types:['coordinate.in','coordinate.update','coordinate.profile','coordinate.out']},f.b).items;
 assert.equal(before[0].data.name,'author');assert.equal(before[0].data.vendor,'claude');assert.equal(before[0].data.status,'unknown');
 assert.equal(before.at(-1).data.name,'new-name');assert.equal(before.at(-1).data.task,'Review schema');
 assert.equal(f.call('peers').items.find((p: JsonRecord)=>p.id===f.a).branch,'next-branch');
 // Shortening a still-live presence is an update; clock expiry has no synthetic out event.
 db.prepare('UPDATE sessions SET expiresAt=expiresAt-100 WHERE id=?').run(f.a);
 assert.equal(f.call('fetch',{type:'coordinate.out',from:f.a},f.b).items.length,0);
 f.call('leave',{},f.a);
 const out=f.call('fetch',{type:'coordinate.out',from:f.a},f.b).items[0];
 assert.equal(out.from,f.a);assert.equal(out.path,f.workspace);assert.equal(out.branch,'next-branch');
 assert.deepEqual({...out.data,expiresAt:0},{name:'new-name',vendor:'claude',task:'Review schema',status:'blocked',expiresAt:0});
 assert.ok(out.data.expiresAt<=out.timestamp);
 const history=f.call('fetch',{type:'coordinate.in',from:f.a},f.b).items[0];assert.equal(history.data.name,'author');
});
test('all type discovery examples are valid, compact fields are complete and discovery creates no DB',t=>{
 const workspace=tempWorkspace(t,'communication-types-',{real:true}),database=join(workspace,'absent.sqlite');
 const call=<Value=JsonRecord>(...args: string[]): Value=>JSON.parse(execFileSync(binary,[...args,'--workspace',workspace,'--database',database],{encoding:'utf8',stdio:'pipe'}));
 const full=call<JsonRecord[]>('schema','types'),compact=call<JsonRecord[]>('schema','types','--compact');
 const byType=(list: JsonRecord[],type: string): JsonRecord=>{const found=list.find(x=>x.type===type);assert.ok(found,`No ${type} schema`);return found;};
 const types=runtime.catalog.catalog().recordTypes.map((item: JsonRecord)=>item.type);
 assert.deepEqual(full.map(item=>item.type),types);assert.deepEqual(compact.map(item=>item.type),types);
 for(const item of full){
  const brief=byType(compact,item.type);
  assert.deepEqual([...brief.required,...brief.optional].sort(),Object.keys(item.dataSchema.properties).sort());
  assert.match(item.description,/\S/);assert.match(item.routing,/\S/);assert.deepEqual(item.fetch,{type:item.type});
  assert.deepEqual(call('schema','type',item.type),item);
  for(const field of Object.keys(item.dataSchema.properties))assert.ok(readFileSync(join(root,'docs/RECORDS.md'),'utf8').includes(field),item.type+':'+field);
 }
 assert.deepEqual(byType(compact,'document').nested.context.optional,['path','kind','branch','ttlMs','expiresAt']);
 assert.deepEqual(byType(compact,'message').nested.next.nested.input.required,['recordId']);
 assert.equal(byType(full,'attachment.created').example.endpoint,null,'schema examples retain null');
 assert.equal(existsSync(database),false);
 assert.ok(Buffer.byteLength(JSON.stringify(compact))<Buffer.byteLength(JSON.stringify(full))/2);
 for(const item of full)runtime.validation.validate(item.dataSchema,item.example);
 assert.throws(()=>call('fetch','--compact'),/supported only for schema types/);
});
