import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { cpSync, existsSync, mkdtempSync, rmSync, readFileSync, mkdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { getOctocodeHome } from '@octocodeai/config';

const root=fileURLToPath(new URL('../',import.meta.url));
const target=execFileSync('rustc',['-vV'],{encoding:'utf8'}).match(/^host: (.+)$/m)[1];
const binary=join(root,'skills/octocode-agents-communication/scripts/bin',target,`octocode-agents-communication${process.platform==='win32'?'.exe':''}`);
const skill=join(root,'skills/octocode-agents-communication');
function fixture(t){const workspace=mkdtempSync(join(tmpdir(),'communication-native-'));t.after(()=>rmSync(workspace,{recursive:true,force:true}));return {workspace,database:join(workspace,'nested/v1.sqlite')};}
function invoke(context,args,extra={}){return JSON.parse(execFileSync(binary,[...args,'--workspace',context.workspace,'--database',context.database],{encoding:'utf8',stdio:['pipe','pipe','pipe'],...extra}));}
function joinAgent(context,name='a'){return invoke(context,['join',JSON.stringify({name,vendor:'generic'})]);}
function start(context,args,input){return new Promise((resolve,reject)=>{const child=spawn(binary,[...args,'--workspace',context.workspace,'--database',context.database]);let stdout='',stderr='';child.stdout.on('data',v=>stdout+=v);child.stderr.on('data',v=>stderr+=v);child.on('error',reject);child.on('close',code=>resolve({code,stdout,stderr}));child.stdin.end(input);});}

test('inspection, discovery and rejected commands do not create storage',t=>{
 const f=fixture(t);assert.equal(invoke(f,['db','info']).exists,false);
 assert.equal(invoke(f,['schema','entities']).length,8);
 for(const args of [['unknown'],['peers'],['join','{"name":"bad"}'],['heartbeat','--session','missing']])assert.throws(()=>invoke(f,args));
 assert.equal(existsSync(dirname(f.database)),false);
});
test('help is compact, command-specific and discoverable without storage',t=>{
 const f=fixture(t),help=invoke(f,['--help']);
 assert.equal(help.implementation,'Rust');assert.ok(JSON.stringify(help).length<2000);
 assert.ok(help.commands.includes('run'));assert.ok(help.discover.includes('<command> --help'));
 for(const name of help.commands){
  const command=invoke(f,[...name.split(' '),'--help']);
  assert.equal(command.name,name);assert.equal(typeof command.description,'string');
  assert.equal(typeof command.usage,'string');assert.equal(command.commands,undefined);
  assert.deepEqual(invoke(f,['schema',...name.split(' ')]),command);
 }
 assert.equal(invoke(f,['schema','entity','session']).name,'session');
 assert.throws(()=>invoke(f,['unknown','--help']));
 assert.equal(existsSync(dirname(f.database)),false);
});
test('native home resolution matches the shared configuration package',t=>{
 const f=fixture(t);
 for(const override of ['', 'relative-home',' ../other-home ',join(f.workspace,'custom')]){
  const env={...process.env,OCTOCODE_HOME:override};
  const value=JSON.parse(execFileSync(binary,['db','info'],{env,encoding:'utf8'}));
  assert.equal(value.path,join(getOctocodeHome(env),'agents-communication/v1.sqlite'));
 }
});
test('CLI entity round trips, subscriptions, delivery status and generic resume',t=>{
 const f=fixture(t),a=joinAgent(f),b=joinAgent(f,'b');const call=(...args)=>invoke(f,[...args,'--session',a.id]);
 assert.equal(call('entity','set','session',a.id,'{"name":"updated","vendorSession":"external"}').name,'updated');
 assert.throws(()=>call('entity','set','session',b.id,'{"name":"bad"}'));
 call('entity','set','subscriptions',a.id,'{"topics":["build","build"]}');
 assert.deepEqual(call('entity','get','subscriptions',a.id).topics,['build']);
 const sent=call('send_message',JSON.stringify({to:b.id,body:'hello',key:'one'}));
 assert.equal(call('entity','list','message','{"direction":"sent"}').items.length,1);
 assert.equal(call('entity','get','delivery',`${sent.id}:${b.id}`).acknowledgedAt,null);
 invoke(f,['ack',JSON.stringify({message:sent.id}),'--session',b.id]);
 assert.equal(typeof call('entity','get','delivery',`${sent.id}:${b.id}`).acknowledgedAt,'number');
 const lock=call('lock','{"path":"src","kind":"tree"}');
 assert.equal(call('entity','list','lease','{"path":"src/file"}').items[0].id,lock.lease.id);
 call('leave');assert.equal(call('resume','{"vendor":"generic"}').id,a.id);
 assert.equal(call('entity','get','lease',String(lock.lease.id)),null);
});
test('eight cold CLI processes initialize one database and acquire one lease',async t=>{
 const f=fixture(t);
 const joins=await Promise.all(Array.from({length:8},(_,i)=>start(f,['join',JSON.stringify({name:String(i),vendor:'generic'})])));
 for(const result of joins)assert.equal(result.code,0,result.stderr);
 const agents=joins.map(r=>JSON.parse(r.stdout));assert.equal(invoke(f,['peers']).items.length,8);
 const locks=await Promise.all(agents.map((a,i)=>start(f,['lock',JSON.stringify({path:i%2?'Contended':'contended'}),'--session',a.id])));
 for(const result of locks)assert.equal(result.code,0,result.stderr);
 assert.equal(locks.filter(r=>JSON.parse(r.stdout).ok).length,1);
});
test('MCP binds identity and survives malformed frames',async t=>{
 const f=fixture(t),a=joinAgent(f);
 const frames=[null,'INVALID',...[
  {jsonrpc:'2.0',id:1,method:'initialize'},
  {jsonrpc:'2.0',id:2,method:'tools/list'},
  {jsonrpc:'2.0',id:3,method:'tools/call',params:{name:'lock',arguments:{path:'file'}}},
  {jsonrpc:'2.0',id:4,method:'tools/call',params:{name:'lock',arguments:{path:'file',owner:'other'}}},
  {jsonrpc:'2.0',id:5,method:'tools/call',params:{name:'join',arguments:{name:'hidden',vendor:'bad'}}},
 ].map(JSON.stringify)].map(v=>v===null?'null':v).join('\n')+'\n';
 const result=await start(f,['mcp','--session',a.id],frames);assert.equal(result.code,0,result.stderr);
 const rows=result.stdout.trim().split('\n').map(JSON.parse);
 assert.equal(rows[0].error.code,-32600);assert.equal(rows[1].error.code,-32700);
 assert.equal(rows[3].result.tools.length,9);assert.equal(JSON.parse(rows[4].result.content[0].text).lease.owner,a.id);
 assert.equal(rows[5].result.isError,true);assert.equal(rows[6].result.isError,true);
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
test('copied skill runs outside the repo with no Node, Cargo or vendor executables on PATH',t=>{
 const f=fixture(t);const standalone=join(f.workspace,'skill');cpSync(skill,standalone,{recursive:true});
 const runner=join(standalone,'scripts/agents-communication');
 const path=join(f.workspace,'path');mkdirSync(path);
 for(const tool of ['uname','dirname'])symlinkSync(`/usr/bin/${tool}`,join(path,tool));
 const env={...process.env,PATH:path};
 const run=(...args)=>JSON.parse(execFileSync(runner,args,{cwd:f.workspace,env,encoding:'utf8'}));
 assert.equal(run('--help').implementation,'Rust');assert.equal(run('schema','entities').length,8);
 assert.ok(run('skill').instructions.includes('scripts/agents-communication'));
 assert.equal(run('skill').instructions,readFileSync(join(standalone,'SKILL.md'),'utf8'));
 assert.ok(run('skill').instructions.trimEnd().split('\n').length<=50);
 assert.equal(existsSync(join(standalone,'references')),false);
 const protocol=run('db','protocol');
 assert.ok(protocol.protocol.includes('BEGIN IMMEDIATE'));
 assert.ok(protocol.database.sql.includes('CREATE TABLE'));
 assert.equal(existsSync(f.database),false);
 const a=run('join','{"name":"standalone","vendor":"any"}','--workspace',f.workspace,'--database',f.database);
 assert.equal(run('peers','--workspace',f.workspace,'--database',f.database).items[0].id,a.id);
 assert.equal(run('db','info','--database',f.database).compatible,true);
});
test('SQLite-only Python agent interoperates with native CLI', {skip:!process.env.COMMUNICATION_PYTHON&&'Set COMMUNICATION_PYTHON to Python 3.14 / Unicode 16 with SQLite >=3.51.3'},async t=>{
 const f=fixture(t),a=joinAgent(f);
 const py=(op,data)=>JSON.parse(execFileSync(process.env.COMMUNICATION_PYTHON,[join(skill,'scripts/sqlite_agent.py'),f.database,f.workspace,op,JSON.stringify(data)],{env:{...process.env,PATH:''},encoding:'utf8',stdio:['pipe','pipe','pipe']}));
 const b=py('join',{name:'python',vendor:'stdlib'});
 const broadcast=py('notify_all',{session:b.id,body:'all from Python',key:'py-all'});
 assert.equal(broadcast.recipients,1);assert.deepEqual(py('notify_all',{session:b.id,body:'all from Python',key:'py-all'}),broadcast);
 assert.equal(invoke(f,['inbox','--session',a.id]).items[0].id,broadcast.id);
 invoke(f,['ack',JSON.stringify({message:broadcast.id}),'--session',a.id]);
 const reverseBroadcast=invoke(f,['notify_all','{"body":"all from Rust"}','--session',a.id]);
 assert.equal(py('inbox',{session:b.id}).items[0].id,reverseBroadcast.id);
 py('ack',{session:b.id,message:reverseBroadcast.id});
 const lease=invoke(f,['lock','{"path":"src","kind":"tree"}','--session',a.id]).lease;
 assert.equal(py('lock',{session:b.id,path:'src/file'}).ok,false);
 const sent=py('send_message',{session:b.id,to:a.id,body:'Python -> Rust',key:'one'});
 assert.equal(invoke(f,['inbox','--session',a.id]).items[0].id,sent.id);
 invoke(f,['ack',JSON.stringify({message:sent.id}),'--session',a.id]);
 const reply=invoke(f,['send_message',JSON.stringify({to:b.id,body:'Rust -> Python'}),'--session',a.id]);
 assert.equal(py('inbox',{session:b.id}).items[0].id,reply.id);
 assert.equal(py('ack',{session:b.id,message:reply.id}).acknowledged,true);
 invoke(f,['unlock',JSON.stringify({lease:lease.id}),'--session',a.id]);
 const owned=py('lock',{session:b.id,path:'src',kind:'tree'});assert.equal(owned.ok,true);
 assert.equal(invoke(f,['lock','{"path":"src/file"}','--session',a.id]).ok,false);
 py('unlock',{session:b.id,lease:owned.lease.id});
 for (const [left,right] of [['NEW.txt','new.txt'],['CAFÉ/file','cafe\u0301/FILE'],['Straße','STRASSE']]) {
  const held=invoke(f,['lock',JSON.stringify({path:left}),'--session',a.id]).lease;
  assert.equal(py('lock',{session:b.id,path:right}).ok,false);
  invoke(f,['unlock',JSON.stringify({lease:held.id}),'--session',a.id]);
  const reverse=py('lock',{session:b.id,path:right});assert.equal(reverse.ok,true);
  assert.equal(invoke(f,['lock',JSON.stringify({path:left}),'--session',a.id]).ok,false);
  py('unlock',{session:b.id,lease:reverse.lease.id});
 }
 if (process.platform !== 'win32') {
  mkdirSync(join(f.workspace,'real/sub'),{recursive:true});
  writeFileSync(join(f.workspace,'real/shared'),'target');symlinkSync('real/sub',join(f.workspace,'alias'));
  const held=py('lock',{session:b.id,path:'alias/../shared'});
  assert.equal(invoke(f,['lock','{"path":"real/shared"}','--session',a.id]).ok,false);
  py('unlock',{session:b.id,lease:held.lease.id});
  symlinkSync('cycle',join(f.workspace,'cycle'));
  assert.throws(()=>py('lock',{session:b.id,path:'cycle'}));
  assert.throws(()=>py('lock',{session:b.id,path:'../escape'}));
 }
});

test('Pi inbox pages maximum escaped messages without loss or buffer overflow',async t=>{
 const f=fixture(t),a=joinAgent(f),b=joinAgent(f,'pi');
 const db=new DatabaseSync(f.database);
 // The envelope is valid at the CLI's 16,384 UTF-16-unit limit. Control
 // characters require six output bytes per unit, the largest JSON expansion.
 const body='x'+'\u0001'.repeat(16383);
 invoke(f,['send_message',JSON.stringify({to:b.id,body}),'--session',a.id]);
 const insert=db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt) VALUES(?,?,?,?,?)');
 const delivery=db.prepare('INSERT INTO deliveries(message,recipient) VALUES(?,?)');
 db.exec('BEGIN IMMEDIATE');
 for(let i=1;i<101;i++){const result=insert.run(a.id,b.id,body,`large-${i}`,Date.now()+60000);delivery.run(result.lastInsertRowid,b.id);}
 db.exec('COMMIT');db.close();
 const previous=process.env.OCTOCODE_COMMUNICATION_BINDING;
 t.after(()=>{if(previous===undefined)delete process.env.OCTOCODE_COMMUNICATION_BINDING;else process.env.OCTOCODE_COMMUNICATION_BINDING=previous;});
 process.env.OCTOCODE_COMMUNICATION_BINDING=JSON.stringify({binary,workspace:f.workspace,database:f.database,session:b.id,tools:invoke(f,['schema']).tools});
 const {default:register}=await import(pathToFileURL(join(skill,'scripts/pi-extension.mjs')));
 const tools=[],handlers=new Map();register({registerTool:tool=>tools.push(tool),on:(event,handler)=>handlers.set(event,handler)});
 assert.deepEqual(handlers.get('cache_warming_decision')({type:'cache_warming_decision',action:'warm',warmCost:0.01,missCost:1,continuationProbability:1}),{action:'stop'});
 const inbox=tools.find(tool=>tool.name==='inbox');
 let after=0;const seen=[];
 do {
  const result=await inbox.execute('page',{after});
  assert.ok(Buffer.byteLength(result.content[0].text)<=256*1024);
  assert.ok(result.details.items.length>0);
  for(const item of result.details.items){assert.equal(item.body,body);seen.push(item.id);}
  after=result.details.next;
  if(seen.length%20===0)invoke(f,['heartbeat','--session',b.id]);
 } while(after!==null);
 assert.equal(seen.length,101);assert.equal(new Set(seen).size,101);
 assert.equal(invoke(f,['entity','get','delivery',`1:${b.id}`,'--session',b.id]).acknowledgedAt,null);
});


test('malformed vendor output stops its process and expires its session', {skip:process.platform==='win32'},t=>{
 const f=fixture(t);const bin=join(f.workspace,'path');mkdirSync(bin);
 writeFileSync(join(bin,'codex'),"#!/bin/sh\necho '{invalid'\nexec /bin/sleep 60\n",{mode:0o755});
 assert.throws(()=>invoke(f,['run','--vendor','codex','--model','test','--prompt','test','--duration-ms','1000'],{env:{...process.env,PATH:bin},timeout:10000}));
 assert.deepEqual(invoke(f,['peers']).items,[]);
});

test('duration bounds silent vendor startup and expires the session', {skip:process.platform==='win32'},t=>{
 for (const vendor of ['codex','pi']) {
  const f=fixture(t);const bin=join(f.workspace,'path');mkdirSync(bin);
  writeFileSync(join(bin,vendor),'#!/bin/sh\nexec /bin/sleep 60\n',{mode:0o755});
  const started=Date.now();
  assert.throws(()=>invoke(f,['run','--vendor',vendor,'--model','test','--prompt','test','--duration-ms','1000'],{env:{...process.env,PATH:bin},timeout:6000}),error=>{
   assert.equal(error.status,1);assert.match(error.stderr.toString(),/Timed out/);return true;
  });
  assert.ok(Date.now()-started<5000);assert.deepEqual(invoke(f,['peers']).items,[]);
 }
});

test('Pi provider errors after prompt acceptance fail the worker', {skip:process.platform==='win32'},t=>{
 const f=fixture(t);const bin=join(f.workspace,'path');mkdirSync(bin);
 writeFileSync(join(bin,'pi'),`#!${process.execPath}
 const rl=require('node:readline').createInterface({input:process.stdin});
 const send=value=>console.log(JSON.stringify(value));
 rl.on('line',line=>{const c=JSON.parse(line);
 if(c.type==='get_state')send({id:c.id,type:'response',success:true,data:{sessionId:'fake-pi'}});
 if(c.type==='prompt'){
  send({id:c.id,type:'response',success:true});
  send({type:'message_end',message:{role:'assistant',stopReason:'error',errorMessage:'provider unavailable'}});
  send({type:'agent_end',willRetry:false});send({type:'agent_settled'});
 }});
 `,{mode:0o755});
 assert.throws(()=>invoke(f,['run','--vendor','pi','--model','test','--prompt','test','--duration-ms','3000'],{env:{...process.env,PATH:bin},timeout:6000}),error=>{
  assert.equal(error.status,1);assert.match(error.stderr.toString(),/Pi turn failed.*provider unavailable/);return true;
 });
 assert.deepEqual(invoke(f,['peers']).items,[]);
});

test('duration also bounds a blocked vendor stdin write', {skip:process.platform==='win32'},t=>{
 const f=fixture(t);const bin=join(f.workspace,'path');mkdirSync(bin);
 writeFileSync(join(bin,'claude'),'#!/bin/sh\nexec /bin/sleep 60\n',{mode:0o755});
 assert.throws(()=>invoke(f,['run','--vendor','claude','--model','test','--prompt','x'.repeat(128*1024),'--duration-ms','1000'],{env:{...process.env,PATH:bin},timeout:6000}),error=>{
  assert.equal(error.status,1);assert.match(error.stderr.toString(),/Timed out writing/);return true;
 });
 assert.deepEqual(invoke(f,['peers']).items,[]);
});


test('all proxy protocols receive the exact bundled skill once with their bound identity', {skip:process.platform==='win32'},t=>{
 for(const vendor of ['codex','claude','pi']){
  const f=fixture(t),bin=join(f.workspace,'path'),capture=join(f.workspace,'prompt.json');mkdirSync(bin);
  writeFileSync(join(bin,vendor),`#!${process.execPath}
const fs=require('node:fs');
fs.writeFileSync(process.env.COMMUNICATION_CAPTURE+'.boot',JSON.stringify({cwd:process.cwd(),args:process.argv.slice(2)}));
const rl=require('node:readline').createInterface({input:process.stdin});
const send=v=>console.log(JSON.stringify(v));
const capture=v=>fs.writeFileSync(process.env.COMMUNICATION_CAPTURE,JSON.stringify(v));
rl.on('line',line=>{const c=JSON.parse(line);
 if(c.method==='initialize')send({id:c.id,result:{}});
 if(c.method==='config/read')send({id:c.id,result:{config:{plugins:{'unrelated@test':{enabled:true}},mcp_servers:{unrelated:{enabled:true}}}}});
 if(c.method==='skills/list')send({id:c.id,result:{data:[{skills:[{path:'/unrelated/SKILL.md',enabled:true}]}]}});
 if(c.method==='thread/start'){fs.writeFileSync(process.env.COMMUNICATION_CAPTURE+'.thread',JSON.stringify(c.params));send({id:c.id,result:{thread:{id:'fake-codex'}}});}
 if(c.method==='turn/start'){capture(c.params.input[0].text);send({id:c.id,result:{}});send({method:'thread/tokenUsage/updated',params:{tokenUsage:{last:{inputTokens:12},total:{inputTokens:12}}}});send({method:'turn/completed',params:{turn:{status:'completed'}}});}
 if(c.type==='user'){capture(c.message.content);send({type:'result',is_error:false,usage:{input_tokens:12}});}
 if(c.type==='get_state')send({id:c.id,type:'response',success:true,data:{sessionId:'fake-pi'}});
 if(c.type==='prompt'){capture(c.message);send({id:c.id,type:'response',success:true});send({type:'message_end',message:{role:'assistant',usage:{input:12}}});send({type:'agent_settled'});}
});
`,{mode:0o755});
  // These Node programs emulate vendor CLIs; they must not inherit the test runner protocol.
  const vendorEnv={...process.env,PATH:bin,COMMUNICATION_CAPTURE:capture};delete vendorEnv.NODE_TEST_CONTEXT;
  const events=execFileSync(binary,['run','--vendor',vendor,'--model','test','--prompt','Task sentinel','--trace','--duration-ms','10000','--workspace',f.workspace,'--database',f.database],{env:vendorEnv,encoding:'utf8',timeout:15000}).trim().split('\n').map(JSON.parse);
  assert.ok(existsSync(capture),`${vendor}: prompt not captured; ${JSON.stringify(events)}`);
  const prompt=JSON.parse(readFileSync(capture,'utf8')),instructions=readFileSync(join(skill,'SKILL.md'),'utf8');
  assert.equal(prompt.split(instructions).length,2);
  assert.ok(prompt.includes(events.find(e=>e.type==='ready').session));
  assert.ok(prompt.endsWith('User task:\nTask sentinel'));
  const boot=JSON.parse(readFileSync(capture+'.boot','utf8'));
  assert.notEqual(boot.cwd,f.workspace);assert.equal(existsSync(boot.cwd),false,'temporary vendor directory survived teardown');
  assert.equal(events.filter(e=>e.type==='usage').length,1);
  if(vendor==='codex'){
   const params=JSON.parse(readFileSync(capture+'.thread','utf8'));
   assert.equal(params.cwd,boot.cwd);assert.equal(params.config.project_doc_max_bytes,0);
   assert.equal(params.config.features.shell_tool,false);assert.equal(params.ephemeral,true);
   assert.equal(params.config.plugins['unrelated@test'].enabled,false);
   assert.equal(params.config.mcp_servers.unrelated.enabled,false);
   assert.equal(params.config.features.code_mode.enabled,false);
   assert.deepEqual(params.config.skills.config,[{path:'/unrelated/SKILL.md',enabled:false}]);
   assert.equal(params.config.features.skill_search,false);
   assert.match(params.baseInstructions,/Do not initiate messages, broadcasts, subscriptions/);
  }
  if(vendor==='pi'||vendor==='claude')assert.match(boot.args[boot.args.indexOf('--system-prompt')+1],/Do not initiate messages, broadcasts, subscriptions/);
  if(vendor==='claude'){assert.ok(boot.args.includes('--system-prompt'));assert.deepEqual(JSON.parse(boot.args[boot.args.indexOf('--settings')+1]),{disableAllHooks:true,autoMemoryEnabled:false});}
  assert.deepEqual(invoke(f,['peers']).items,[]);
 }
});

test('deleted database is not silently recreated by an existing-session command',t=>{
 const f=fixture(t),a=joinAgent(f);invoke(f,['leave','--session',a.id]);
 rmSync(f.database);
 assert.throws(()=>invoke(f,['send_message','{"to":"missing","body":"must fail"}','--session',a.id]));
 assert.equal(existsSync(f.database),false);
});

test('idle proxy exits and reaps its vendor when the database is removed', {skip:process.platform==='win32'},t=>{
 const f=fixture(t),bin=join(f.workspace,'path'),pidFile=join(f.workspace,'vendor.pid');mkdirSync(bin);
 writeFileSync(join(bin,'claude'),`#!${process.execPath}
 const fs=require('node:fs');fs.writeFileSync(process.env.COMMUNICATION_PID,String(process.pid));
 require('node:readline').createInterface({input:process.stdin}).on('line',()=>{
  console.log(JSON.stringify({type:'result',is_error:false}));
  setTimeout(()=>fs.renameSync(process.env.COMMUNICATION_DB,process.env.COMMUNICATION_DB+'.removed'),100);
 });
 `,{mode:0o755});
 const env={...process.env,PATH:bin,COMMUNICATION_PID:pidFile,COMMUNICATION_DB:f.database};delete env.NODE_TEST_CONTEXT;
 assert.throws(()=>invoke(f,['run','--vendor','claude','--model','test','--prompt','wait','--duration-ms','10000'],{env,timeout:15000}),e=>{
  assert.equal(e.status,1);assert.match(e.stderr.toString(),/database disappeared/);return true;
 });
 assert.equal(existsSync(f.database),false);
 assert.throws(()=>process.kill(Number(readFileSync(pidFile,'utf8')),0),e=>e.code==='ESRCH');
});
