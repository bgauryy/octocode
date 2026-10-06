import { test } from 'node:test';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from './helpers.mjs';
import { cpSync, realpathSync, existsSync, rmSync, readFileSync, mkdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { python } from '../src/artifact-checks.mjs';
import { getOctocodeHome } from '../../../packages/octocode-config/src/home.ts';
import { root, nativeBinary as binary, tempWorkspace, reasoningCommands, withReasoning, fastHeartbeatEnv } from './helpers.mjs';

function testArgs(args){
 if(!reasoningCommands.includes(args[0])||!args[1]?.startsWith('{'))return args;
 return [args[0],JSON.stringify(withReasoning(args[0],JSON.parse(args[1]))),...args.slice(2)];
}

const skill=join(root,'.');
function stableVendor(bin,vendor,script){rmSync(join(bin,vendor),{force:true});symlinkSync(join(root,'tests/vendor-fixture.sh'),join(bin,vendor));return {COMMUNICATION_FIXTURE_NODE:process.execPath,COMMUNICATION_FIXTURE_SCRIPT:script};}
function fixture(t){const workspace = tempWorkspace(t, 'communication-native-');return {workspace,database:join(workspace,'nested/communication.sqlite')};}
function invoke(context,args,extra={}){return JSON.parse(execFileSync(binary,[...testArgs(args),'--workspace',context.workspace,'--database',context.database],{encoding:'utf8',stdio:['pipe','pipe','pipe'],...extra}));}
function joinAgent(context,name='a'){return invoke(context,['join',JSON.stringify({name,vendor:'generic'})]);}
function start(context,args,input){return new Promise((resolve,reject)=>{const child=spawn(binary,[...testArgs(args),'--workspace',context.workspace,'--database',context.database]);let stdout='',stderr='';child.stdout.on('data',v=>stdout+=v);child.stderr.on('data',v=>stderr+=v);child.on('error',reject);child.on('close',code=>resolve({code,stdout,stderr}));child.stdin.end(input);});}

test('inspection, discovery and rejected commands do not create storage',t=>{
 const f=fixture(t);assert.equal(invoke(f,['db','info']).exists,false);
 assert.equal(invoke(f,['schema','types']).length,25);
 for(const args of [['unknown'],['peers'],['join','{"name":"bad"}'],['heartbeat','--session','missing']])assert.throws(()=>invoke(f,args));
 assert.equal(existsSync(dirname(f.database)),false);
});
test('help is compact, command-specific and discoverable without storage',t=>{
 const f=fixture(t),help=invoke(f,['--help']);
 assert.equal(help.implementation,'Python');assert.ok(JSON.stringify(help).length<2000);
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
test('Python home resolution matches the shared configuration package',t=>{
 const f=fixture(t);
 for(const override of ['', 'relative-home',' ../other-home ',join(f.workspace,'custom')]){
  const env={...process.env,OCTOCODE_HOME:override};
  const value=execFileSync(python(),['-B','-c','import sys; sys.path.insert(0,sys.argv[1]); from octocode_config import get_octocode_home; print(get_octocode_home())',join(root,'scripts')],{env,encoding:'utf8'}).trim();
  assert.equal(value,getOctocodeHome(env));
 }
});
test('CLI typed history, subscriptions, delivery status and generic resume',t=>{
 const f=fixture(t),a=joinAgent(f),b=joinAgent(f,'b');const call=(...args)=>invoke(f,[...args,'--session',a.id]);
 call('heartbeat','{"name":"updated","vendorSession":"external"}');
 assert.equal(call('peers').items.find(x=>x.id===a.id).name,'updated');
 assert.throws(()=>call('entity','set','session',b.id,'{"name":"bad"}'));
 call('subscribe','{"topics":["build","build"]}');
 assert.equal(call('fetch','{"type":"subscription.added"}').items[0].data.topic,'build');
 const sent=call('send_message',JSON.stringify({to:b.id,body:'hello',key:'one',replyRequired:false}));
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
 const agents=joins.map(r=>JSON.parse(r.stdout));assert.equal(invoke(f,['peers']).items.length,8);
 const locks=await Promise.all(agents.map((a,i)=>start(f,['lock',JSON.stringify({path:i%2?'Contended':'contended'}),'--session',a.id])));
 for(const result of locks)assert.equal(result.code,0,result.stderr);
 assert.equal(locks.filter(r=>JSON.parse(r.stdout).ok).length,1);
});
test('MCP rejects an oversized frame, ignores blank lines and keeps serving queued requests',async t=>{
 const f=fixture(t),a=joinAgent(f);
 const oversized=JSON.stringify({jsonrpc:'2.0',id:1,method:'ping',params:{pad:'x'.repeat(8*1024*1024)}});
 const input=['',oversized,'   ',JSON.stringify({jsonrpc:'2.0',id:2,method:'ping'})].join('\n')+'\n';
 const result=await start(f,['mcp','--session',a.id],input);assert.equal(result.code,0,result.stderr);
 const rows=result.stdout.trim().split('\n').map(JSON.parse);
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
 ].map(JSON.stringify)].map(v=>v===null?'null':v).join('\n')+'\n';
 const result=await start(f,['mcp','--session',a.id],frames);assert.equal(result.code,0,result.stderr);
 const rows=result.stdout.trim().split('\n').map(JSON.parse);
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
test('copied skill runs outside the repo with no Node, Cargo or vendor executables on PATH',t=>{
 const f=fixture(t);const standalone=join(f.workspace,'skill');mkdirSync(standalone);
 for(const entry of ['OPERATING.md','scripts'])cpSync(join(skill,entry),join(standalone,entry),{recursive:true});
 const runner=join(standalone,'scripts/agents-communication');
 const path=join(f.workspace,'path');mkdirSync(path);
 for(const tool of ['uname','dirname'])symlinkSync(`/usr/bin/${tool}`,join(path,tool));
 const interpreter=execFileSync(python(),['-c','import sys; print(sys.executable)'],{encoding:'utf8'}).trim();
 const env={...process.env,PATH:path,OCTOCODE_PYTHON:interpreter};
 const run=(...args)=>{
  const started=Date.now();
  try{return JSON.parse(execFileSync('/bin/sh',[runner,...args],{cwd:f.workspace,env,encoding:'utf8',stdio:['pipe','pipe','pipe'],timeout:60000,maxBuffer:1024*1024}));}
  catch(error){t.diagnostic(JSON.stringify({phase:'copied-skill-launcher',command:args[0],elapsedMs:Date.now()-started,code:error.code,signal:error.signal,stderr:error.stderr?.toString().slice(-2000)}));throw error;}
 };
 assert.equal(run('--help').implementation,'Python');assert.equal(run('schema','types').length,25);
 assert.ok(run('skill').instructions.includes('scripts/agents-communication'));
 assert.equal(run('skill').instructions,readFileSync(join(standalone,'OPERATING.md'),'utf8'));
 // The compact skill routes setup, hooks and the command catalog to portable references.
 assert.ok(Buffer.byteLength(run('skill').instructions)<=30000);
 assert.equal(existsSync(join(standalone,'references')),false);
 const protocol=run('db','protocol');
 assert.ok(protocol.protocol.includes('BEGIN IMMEDIATE'));
 assert.ok(protocol.database.sql.includes('CREATE TABLE'));
 assert.equal(existsSync(f.database),false);
 const a=run('join','{"name":"standalone","vendor":"any"}','--workspace',f.workspace,'--database',f.database);
 assert.equal(run('peers','--workspace',f.workspace,'--database',f.database).items[0].id,a.id);
 assert.equal(run('db','info','--database',f.database).compatible,true);
});
test('Pi inbox pages maximum escaped messages without loss or buffer overflow',async t=>{
 const f=fixture(t),a=joinAgent(f),b=joinAgent(f,'pi');
 const db=new DatabaseSync(f.database);
 // The envelope is valid at the CLI's 16,384 UTF-16-unit limit. Control
 // characters require six output bytes per unit, the largest JSON expansion.
 const body='x'+'\u0001'.repeat(16383);
 invoke(f,['send_message',JSON.stringify({to:b.id,body}),'--session',a.id]);
 const insert=db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning) VALUES(?,?,?,?,?,?)');
 const delivery=db.prepare('INSERT INTO deliveries(message,recipient) VALUES(?,?)');
 db.exec('BEGIN IMMEDIATE');
 for(let i=1;i<101;i++){const result=insert.run(a.id,b.id,body,`large-${i}`,Date.now()+60000,'Verify bounded delivery of escaped payloads');delivery.run(result.lastInsertRowid,b.id);}
 db.exec('COMMIT');db.close();
 const previous=process.env.OCTOCODE_COMMUNICATION_BINDING;
 t.after(()=>{if(previous===undefined)delete process.env.OCTOCODE_COMMUNICATION_BINDING;else process.env.OCTOCODE_COMMUNICATION_BINDING=previous;});
 process.env.OCTOCODE_COMMUNICATION_BINDING=JSON.stringify({binary,workspace:f.workspace,database:f.database,session:b.id,tools:invoke(f,['schema']).tools});
 const {default:register}=await import(pathToFileURL(join(skill,'scripts/pi-extension.mjs')));
 const tools=[],handlers=new Map();register({registerTool:tool=>tools.push(tool),on:(event,handler)=>handlers.set(event,handler)});
 const catalog=invoke(f,['schema']);
 for(const tool of tools){
  const definition=catalog.tools.find(item=>item.name===tool.name);
  assert.equal(tool.description,definition.description);
  assert.deepEqual(tool.parameters,definition.inputSchema);
 }
 assert.deepEqual(handlers.get('cache_warming_decision')({type:'cache_warming_decision',action:'warm',warmCost:0.01,missCost:1,continuationProbability:1}),{action:'stop'});
 const inbox=tools.find(tool=>tool.name==='inbox');
 let after=0;const seen=[];
 do {
  const result=await inbox.execute('page',{after});
  assert.ok(Buffer.byteLength(result.content[0].text)<=256*1024);
  assert.ok(result.details.items.length>0);
  for(const item of result.details.items){assert.equal(item.body,body);seen.push(item.id);}
  after=result.details.next?.input.after;
  if(seen.length%20===0)invoke(f,['heartbeat','--session',b.id]);
 } while(after!=null);
 assert.equal(seen.length,101);assert.equal(new Set(seen).size,101);
 assert.equal(invoke(f,['fetch','{"type":"delivery.acknowledged","where":{"messageId":1}}','--session',b.id]).items.length,0);
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
 const script=join(bin,'pi.cjs');writeFileSync(script,readFileSync(join(bin,'pi')));
 const env={...process.env,PATH:bin,...stableVendor(bin,'pi',script)};delete env.NODE_TEST_CONTEXT;
 // Separate first execution of the fresh fixture from the provider-error deadline.
 execFileSync(join(bin,'pi'),[],{env,input:'',stdio:['pipe','pipe','pipe'],timeout:30000});
 assert.throws(()=>invoke(f,['run','--vendor','pi','--model','test','--prompt','test','--duration-ms','3000'],{env,timeout:6000}),error=>{
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


// Stop a worker once its first turn completes instead of waiting out --duration-ms.
function runUntilTurn(args,env,timeout=15000,turns=1){return new Promise((resolve,reject)=>{
 const child=spawn(binary,args,{env});let stdout='',stderr='',interrupted=false;
 let forceTimer;const timer=setTimeout(()=>{child.kill('SIGTERM');forceTimer=setTimeout(()=>child.kill('SIGKILL'),2000);},timeout);
 child.stdout.on('data',chunk=>{stdout+=chunk;
  if(!interrupted&&stdout.split('\n').filter(line=>line.includes('"turn-completed"')).length>=turns){interrupted=true;child.kill('SIGINT');}});
 child.stderr.on('data',chunk=>stderr+=chunk);child.on('error',reject);
 child.on('close',code=>{clearTimeout(timer);clearTimeout(forceTimer);
  if(code===0&&interrupted)resolve(stdout.trim().split('\n').map(JSON.parse));
  else reject(Object.assign(Error(`worker exited ${code}`),{stdout,stderr}));});
});}
test('all proxy protocols receive the worker skill once with their bound identity', {skip:process.platform==='win32'},async t=>{
 for(const vendor of ['codex','claude','pi']){
  const f=fixture(t),bin=join(f.workspace,'path'),capture=join(f.workspace,'prompt.json');mkdirSync(bin);
  writeFileSync(join(bin,vendor),`#!${process.execPath}
const fs=require('node:fs');
fs.writeFileSync(process.env.COMMUNICATION_CAPTURE+'.boot',JSON.stringify({at:Date.now(),cwd:process.cwd(),args:process.argv.slice(2)}));
const rl=require('node:readline').createInterface({input:process.stdin});
const send=v=>console.log(JSON.stringify(v));
const capture=v=>fs.writeFileSync(process.env.COMMUNICATION_CAPTURE,JSON.stringify(v));
rl.on('line',line=>{const c=JSON.parse(line);
 fs.appendFileSync(process.env.COMMUNICATION_CAPTURE+'.frames',JSON.stringify({at:Date.now(),id:c.id,method:c.method,type:c.type})+'\\n');
 if(c.method==='initialize')send({id:c.id,result:{}});
 if(c.method==='thread/inject_items')send({id:c.id,result:{}});
 if(c.method==='config/read')send({id:c.id,result:{config:{plugins:{'unrelated@test':{enabled:true}},mcp_servers:{unrelated:{enabled:true}}}}});
 if(c.method==='skills/list')send({id:c.id,result:{data:[{skills:[{path:'/unrelated/OPERATING.md',enabled:true}]}]}});
 if(c.method==='thread/start'){fs.writeFileSync(process.env.COMMUNICATION_CAPTURE+'.thread',JSON.stringify(c.params));send({id:c.id,result:{thread:{id:'fake-codex'}}});}
 if(c.method==='turn/start'){capture(c.params.input[0].text);send({id:c.id,result:{}});send({method:'thread/tokenUsage/updated',params:{tokenUsage:{last:{inputTokens:12},total:{inputTokens:12}}}});send({method:'turn/completed',params:{turn:{status:'completed'}}});}
 if(c.type==='user'){capture(c.message.content);send({type:'result',is_error:false,usage:{input_tokens:12}});}
 if(c.type==='get_state')send({id:c.id,type:'response',success:true,data:{sessionId:'fake-pi'}});
 if(c.type==='prompt'){capture(c.message);send({id:c.id,type:'response',success:true});send({type:'message_end',message:{role:'assistant',usage:{input:12}}});send({type:'agent_settled'});}
});
`,{mode:0o755});
  // Keep the Node interpreter stable and pass vendor flags only to the fixture.
  const script = join(bin, `${vendor}.cjs`);
  writeFileSync(script, readFileSync(join(bin, vendor)));
  const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
  writeFileSync(join(bin, vendor), `#!/bin/sh\nprintf 'started' > ${quote(capture+'.launch')}\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`, { mode: 0o755 });
  // These Node programs emulate vendor CLIs; they must not inherit the test runner protocol.
  const vendorEnv={...process.env,PATH:bin,COMMUNICATION_CAPTURE:capture,...stableVendor(bin,vendor,script)};delete vendorEnv.NODE_TEST_CONTEXT;
  // Separate the host's first-execution checks on this freshly written fixture
  // from protocol assertions. Dedicated tests above cover production startup deadlines.
  const preflightStarted=Date.now();
  try {
   execFileSync(join(bin,vendor),[],{env:vendorEnv,input:'',encoding:'utf8',stdio:['pipe','pipe','pipe'],timeout:30000,maxBuffer:1024*1024});
  } catch(error) {
   t.diagnostic(JSON.stringify({vendor,phase:'fixture-interpreter-preflight',elapsedMs:Date.now()-preflightStarted,wrapperStarted:existsSync(capture+'.launch'),nodeStarted:existsSync(capture+'.boot'),stderr:error.stderr?.toString().slice(-2000)}));
   throw error;
  }
  t.diagnostic(`${vendor} fixture interpreter preflight: ${Date.now()-preflightStarted} ms`);
  for(const suffix of ['.launch','.boot','.frames','.thread',''])rmSync(capture+suffix,{force:true});
  const started=Date.now();
  let events;
  try {
   events=await runUntilTurn(['run','--vendor',vendor,'--model','test','--prompt','Task sentinel','--tools','peers,send_message,complete','--trace','--duration-ms','10000','--workspace',f.workspace,'--database',f.database],vendorEnv);
  } catch(error) {
   const boot=existsSync(capture+'.boot')?JSON.parse(readFileSync(capture+'.boot','utf8')):null;
   const frames=existsSync(capture+'.frames')?readFileSync(capture+'.frames','utf8').trim().split('\n').slice(-20).map(JSON.parse):[];
   t.diagnostic(JSON.stringify({vendor,elapsedMs:Date.now()-started,wrapperStarted:existsSync(capture+'.launch'),nodeBootMs:boot?boot.at-started:null,frames,stderr:error.stderr?.toString().slice(-2000),stdout:error.stdout?.toString().slice(-2000)}));
   throw error;
  }
  assert.ok(existsSync(capture),`${vendor}: prompt not captured; boot=${existsSync(capture+'.boot')}; ${JSON.stringify(events)}`);
  const prompt=JSON.parse(readFileSync(capture,'utf8'));
  assert.equal(prompt.split('# Agents communication').length,2);
  assert.ok(prompt.includes('complete'));
  const workerSkill=readFileSync(new URL('../OPERATING.md',import.meta.url),'utf8').replace(/^---\n[\s\S]*?\n---\n/,'').replace(/## Edit with ownership[\s\S]*?(?=## Share evidence)/,'').replace(/```mermaid[\s\S]*?(?=## Discover)/,'');
  assert.ok(prompt.includes(workerSkill.trim()), 'worker receives the canonical core for its selected tool profile');
  assert.ok(prompt.includes('Skip independent solo tasks') === false, 'frontmatter must stay out of worker instructions');
  assert.ok(!prompt.includes('## Host setup'));
  assert.ok(!prompt.includes('## CLI command map'));
  assert.equal(prompt.split('\n')[0], 'Available communication tools: ["peers","send_message","complete"]');
  assert.match(prompt, /Communication binding:/);
  const binding=JSON.parse(prompt.match(/Communication binding: (.*)/)[1]);
  assert.equal(binding.workspace,realpathSync(f.workspace));assert.equal(binding.coordinationScope,realpathSync(f.workspace));
  assert.deepEqual(binding.tools,['peers','send_message','complete']);
  assert.equal(binding.instructionsVersion,1);
  assert.equal(binding.instructionsSha256,createHash('sha256').update(workerSkill).digest('hex'));
  assert.ok(!prompt.includes('## Edit with ownership'));
  assert.ok(prompt.includes(events.find(e=>e.type==='ready').session));
  assert.ok(prompt.endsWith('User task:\nTask sentinel'));
  const boot=JSON.parse(readFileSync(capture+'.boot','utf8'));
  assert.notEqual(boot.cwd,f.workspace);assert.equal(existsSync(boot.cwd),false,'temporary vendor directory survived teardown');
  assert.equal(events.filter(e=>e.type==='usage').length,1);
  const stateDb=new DatabaseSync(f.database,{readOnly:true});
  assert.equal(stateDb.prepare('SELECT status FROM sessions WHERE id=?').get(events.find(e=>e.type==='ready').session).status,'available');
  stateDb.close();
  if(vendor==='codex'){
   const params=JSON.parse(readFileSync(capture+'.thread','utf8'));
   assert.equal(params.cwd,boot.cwd);assert.equal(params.config.project_doc_max_bytes,0);
   assert.equal(params.config.features.shell_tool,false);assert.equal(params.ephemeral,true);
   assert.equal(params.config.plugins['unrelated@test'].enabled,false);
   assert.equal(params.config.mcp_servers.unrelated.enabled,false);
   assert.equal(params.config.features.code_mode.enabled,false);
   assert.deepEqual(params.config.skills.config,[{path:'/unrelated/OPERATING.md',enabled:false}]);
   assert.equal(params.config.features.skill_search,false);
   assert.match(params.baseInstructions,/Initiate messages, broadcasts or subscriptions only when the task authorizes them/);
  }
  if(vendor==='pi'||vendor==='claude')assert.match(boot.args[boot.args.indexOf('--system-prompt')+1],/Initiate messages, broadcasts or subscriptions only when the task authorizes them/);
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
 if(process.argv.includes('--fixture-preflight'))process.exit(0);
 const fs=require('node:fs');fs.writeFileSync(process.env.COMMUNICATION_PID,String(process.pid));
 require('node:readline').createInterface({input:process.stdin}).on('line',()=>{
  console.log(JSON.stringify({type:'result',is_error:false}));
  setTimeout(()=>fs.renameSync(process.env.COMMUNICATION_DB,process.env.COMMUNICATION_DB+'.removed'),100);
 });
 `,{mode:0o755});
 const script=join(bin,'claude.cjs');writeFileSync(script,readFileSync(join(bin,'claude')));
 const quote=value=>"'"+value.replaceAll("'","'\\''")+"'";
 writeFileSync(join(bin,'claude'),`#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`,{mode:0o755});
 const env=fastHeartbeatEnv({...process.env,PATH:bin,COMMUNICATION_PID:pidFile,COMMUNICATION_DB:f.database,...stableVendor(bin,'claude',script)});delete env.NODE_TEST_CONTEXT;
 // Warm only the fixture interpreter; the guard does not create a PID or touch the DB.
 const preflightStarted=Date.now();
 execFileSync(join(bin,'claude'),['--fixture-preflight'],{env,input:'',stdio:['pipe','pipe','pipe'],timeout:30000,maxBuffer:1024*1024});
 t.diagnostic(`idle fixture interpreter preflight: ${Date.now()-preflightStarted} ms`);
 assert.equal(existsSync(pidFile),false);assert.equal(existsSync(f.database),false);
 const started=Date.now();
 try {
  assert.throws(()=>invoke(f,['run','--vendor','claude','--model','test','--prompt','wait','--duration-ms','10000'],{env,timeout:15000}),e=>{
   assert.equal(e.status,1);assert.match(e.stderr.toString(),/database disappeared/);return true;
  });
 } catch(error) {
  t.diagnostic(JSON.stringify({phase:'idle-database-removal',elapsedMs:Date.now()-started,vendorStarted:existsSync(pidFile),databaseExists:existsSync(f.database),stderr:error.stderr?.toString().slice(-2000)}));
  throw error;
 }
 assert.equal(existsSync(f.database),false);
 assert.throws(()=>process.kill(Number(readFileSync(pidFile,'utf8')),0),e=>e.code==='ESRCH');
});

test('managed run resumes a native-bound identity with its new vendor session', {skip:process.platform==='win32'}, async t=>{
 const f=fixture(t),bin=join(f.workspace,'path');mkdirSync(bin);
 const script=join(bin,'claude.cjs');
 writeFileSync(script, `const rl=require('node:readline').createInterface({input:process.stdin});
 rl.on('line',()=>{console.log(JSON.stringify({type:'system',subtype:'init',session_id:'managed-session'}));console.log(JSON.stringify({type:'result',is_error:false}));});`);
 const quote=value=>"'"+value.replaceAll("'","'\\''")+"'";
 writeFileSync(join(bin,'claude'),`#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`,{mode:0o755});
 const env={...process.env,PATH:bin};delete env.NODE_TEST_CONTEXT;
 const identity=invoke(f,['join',JSON.stringify({name:'resumed-worker',vendor:'claude'})]);
 invoke(f,['attach',JSON.stringify({transport:'claude',endpoint:join(f.workspace,'old.sock'),vendorSession:'old-native-session'}),'--session',identity.id]);
 invoke(f,['leave','--session',identity.id]);
 await runUntilTurn(['run','--vendor','claude','--model','test','--prompt','Complete assigned work','--session',identity.id,'--duration-ms','10000','--workspace',f.workspace,'--database',f.database],env);
 const db=new DatabaseSync(f.database,{readOnly:true});t.after(()=>db.close());
 assert.equal(db.prepare('SELECT vendorSession FROM sessions WHERE id=?').get(identity.id).vendorSession,'managed-session');
 assert.equal(db.prepare('SELECT transport FROM attachments WHERE session=?').get(identity.id).transport,'raw');
 assert.deepEqual(invoke(f,['peers']).items,[]);
});

test('managed Codex separates user kickoff from pending peer tools and preserves mail on startup failure', {skip:process.platform==='win32'}, async t=>{
 for(const rejectKickoff of [false,true]){
  const f=fixture(t),bin=join(f.workspace,'path'),capture=join(f.workspace,'requests.jsonl');mkdirSync(bin);
  const script=join(bin,'codex.cjs');
  writeFileSync(script,`const fs=require('node:fs');
  const send=v=>console.log(JSON.stringify(v));
  require('node:readline').createInterface({input:process.stdin}).on('line',line=>{
   const c=JSON.parse(line);if(!c.id)return;
   fs.appendFileSync(process.env.CAPTURE,JSON.stringify(c)+'\\n');
   if(c.method==='turn/start'){
    if(process.env.REJECT_KICKOFF==='true')return send({id:c.id,error:{code:-32000,message:'kickoff rejected'}});
    send({id:c.id,result:{turn:{id:'turn',status:'inProgress'}}});
    send({method:'turn/completed',params:{turn:{id:'turn',status:'completed'}}});return;
   }
   const result=c.method==='config/read'?{config:{}}:c.method==='skills/list'?{data:[]}:c.method==='thread/start'?{thread:{id:'worker'}}:{};
   send({id:c.id,result});
  });`);
  const quote=value=>"'"+value.replaceAll("'","'\\''")+"'";
  writeFileSync(join(bin,'codex'),`#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`,{mode:0o755});
  const env={...process.env,PATH:bin,CAPTURE:capture,REJECT_KICKOFF:String(rejectKickoff),...stableVendor(bin,'codex',script)};delete env.NODE_TEST_CONTEXT;
  execFileSync(join(bin,'codex'),[],{env,input:'',timeout:30000});
  const sender=invoke(f,['join',JSON.stringify({name:'supervisor',vendor:'generic',task:'DIRECTORY_SENTINEL'})]),receiver=invoke(f,['join',JSON.stringify({name:'worker',vendor:'codex'})]);
  const sent=invoke(f,['send_message',JSON.stringify({to:receiver.id,body:'PEER_SENTINEL',wake:'action'}),'--session',sender.id]);
  invoke(f,['leave','--session',receiver.id]);
  const args=['run','--vendor','codex','--model','test','--prompt','USER_SENTINEL','--session',receiver.id,'--duration-ms','3000','--workspace',f.workspace,'--database',f.database];
  if(rejectKickoff)await assert.rejects(()=>runUntilTurn(args,env),/worker exited 1/);
  else await runUntilTurn(args,env,10000,2);
  const frames=readFileSync(capture,'utf8').trim().split('\n').map(JSON.parse);
  const turns=frames.filter(x=>x.method==='turn/start');
  assert.match(turns[0].params.input[0].text,/USER_SENTINEL/);
  assert.ok(!JSON.stringify(turns[0]).includes('PEER_SENTINEL'),'peer data cannot be promoted into the user kickoff');
  assert.ok(!JSON.stringify(turns[0]).includes('DIRECTORY_SENTINEL'));
  const directory=frames.find(x=>x.method==='thread/inject_items').params.items[0];
  assert.equal(directory.type,'function_call_output');assert.match(directory.output,/DIRECTORY_SENTINEL/);
  const db=new DatabaseSync(f.database,{readOnly:true});
  try{
   const delivery=db.prepare('SELECT state FROM dispatches WHERE message=?').get(sent.id);
   if(rejectKickoff){assert.equal(turns.length,1);assert.equal(delivery,undefined);}
   else{
    assert.equal(turns.length,2);assert.deepEqual(turns[1].params.input,[]);
    assert.equal(turns[1].params.toolOutput.name,'octocode_peer_messages');
    assert.equal(turns[1].params.toolOutput.output.split('PEER_SENTINEL').length-1,1);
    assert.equal(delivery.state,'submitted');
   }
   assert.equal(db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(sent.id).acknowledgedAt,null);
  }finally{db.close();}
 }
});

test('live presence and lease filters remain separate from declared availability history',t=>{
 const f=fixture(t),a=joinAgent(f,'observer');
 const b=invoke(f,['join',JSON.stringify({name:'worker',vendor:'generic',status:'blocked'})]);
 const lease=invoke(f,['lock',JSON.stringify({path:'owned.txt'}),'--session',b.id]);
 const db=new DatabaseSync(f.database);t.after(()=>db.close());
 db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(b.id);
 const call=(name,input={})=>invoke(f,[name,JSON.stringify(input),'--session',a.id]);
 assert.deepEqual(call('peers').items.map(x=>x.id),[a.id]);
 assert.equal(call('fetch',{type:'coordinate.in',from:b.id}).items[0].data.status,'blocked');
 assert.equal(call('fetch',{type:'coordinate.out',from:b.id}).items.length,1);
 assert.equal(call('locks').items.length,0);
 assert.equal(call('locks',{presence:'expired'}).items[0].id,lease.lease.id);
 assert.throws(()=>call('locks',{status:'all'}));
});

test('native peer replay text does not break tool tracing',()=>{
 const program = `import sys
sys.path.insert(0,sys.argv[1])
from communication.proxy import trace_tools
assert trace_tools('session', {'type':'user','message':{'content':'peer context'},'isReplay':True}, {}) == []
calls = {}
records = trace_tools('session', {'type':'assistant','message':{'content':[{'type':'tool_use','id':'tool1','name':'peers','input':{}}]}}, calls)
assert len(records) == 1 and records[0]['type'] == 'tool-call' and calls == {'tool1':'peers'}
`;
 execFileSync(python(),['-B','-c',program,join(root,'scripts')],{stdio:'pipe',timeout:10000});
});
