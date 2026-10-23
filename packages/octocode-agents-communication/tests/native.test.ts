import { test } from 'node:test';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from './helpers.ts';
import { realpathSync, existsSync, rmSync, readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import type { JsonRecord } from './json-record.ts';
import { binary, only, fastHeartbeatEnv, isChildFailure } from './helpers.ts';
import { fixture, invoke, joinAgent, skill, stableVendor } from './native-support.ts';

/** What `runUntilTurn` rejects with when the worker does not stop after its turn. */
type WorkerFailure = Error & { stdout: string; stderr: string };
const isWorkerFailure = (error: unknown): error is WorkerFailure => error instanceof Error && 'stdout' in error && 'stderr' in error;
type PiTool = { name: string; description: string; parameters: unknown; execute(id: string, params: JsonRecord): Promise<JsonRecord> };
type PiHandler = (event: JsonRecord) => unknown;

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
 const {default:register}=await import(pathToFileURL(join(skill,'dist/hosts/pi-extension.js')).href);
 const tools: PiTool[]=[],handlers=new Map<string,PiHandler>();register({registerTool:(tool: PiTool)=>tools.push(tool),on:(event: string,handler: PiHandler)=>handlers.set(event,handler)});
 const catalog=invoke(f,['schema']);
 for(const tool of tools){
  const definition=catalog.tools.find((item: JsonRecord)=>item.name===tool.name);
  assert.equal(tool.description,definition.description);
  const expected=structuredClone(definition.inputSchema);
  for(const keyword of ['oneOf','anyOf','allOf','enum','const','not'])delete expected[keyword];
  assert.deepEqual(tool.parameters,expected);
  assert.deepEqual(catalog.tools.find((item: JsonRecord)=>item.name===tool.name).inputSchema,definition.inputSchema);
 }
 const cacheWarming=handlers.get('cache_warming_decision');assert.ok(cacheWarming);
 assert.deepEqual(cacheWarming({type:'cache_warming_decision',action:'warm',warmCost:0.01,missCost:1,continuationProbability:1}),{action:'stop'});
 const inbox=tools.find(tool=>tool.name==='inbox');assert.ok(inbox);
 let after=0;const seen=[];
 do {
  const result: JsonRecord=await inbox.execute('page',{after});
  assert.ok(Buffer.byteLength(result.content[0].text)<=256*1024);
  assert.ok(result.details.items.length>0);
  for(const item of result.details.items as JsonRecord[]){assert.equal(item.body,body);seen.push(item.id);}
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
  assert.throws(()=>invoke(f,['run','--vendor',vendor,'--model','test','--prompt','test','--duration-ms','1000'],{env:{...process.env,PATH:bin},timeout:6000}),(error: unknown)=>{
   assert.ok(isChildFailure(error));assert.equal(error.status,1);assert.match(error.stderr.toString(),/Timed out/);return true;
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
 const env: NodeJS.ProcessEnv={...process.env,PATH:bin,...stableVendor(bin,'pi',script)};delete env.NODE_TEST_CONTEXT;
 // Separate first execution of the fresh fixture from the provider-error deadline.
 execFileSync(join(bin,'pi'),[],{env,input:'',stdio:['pipe','pipe','pipe'],timeout:30000});
 assert.throws(()=>invoke(f,['run','--vendor','pi','--model','test','--prompt','test','--duration-ms','3000'],{env,timeout:6000}),(error: unknown)=>{
  assert.ok(isChildFailure(error));assert.equal(error.status,1);assert.match(error.stderr.toString(),/Pi turn failed.*provider unavailable/);return true;
 });
 assert.deepEqual(invoke(f,['peers']).items,[]);
});

test('duration also bounds a blocked vendor stdin write', {skip:process.platform==='win32'},t=>{
 const f=fixture(t);const bin=join(f.workspace,'path');mkdirSync(bin);
 writeFileSync(join(bin,'claude'),'#!/bin/sh\nexec /bin/sleep 60\n',{mode:0o755});
 assert.throws(()=>invoke(f,['run','--vendor','claude','--model','test','--prompt','x'.repeat(128*1024),'--duration-ms','1000'],{env:{...process.env,PATH:bin},timeout:6000}),(error: unknown)=>{
  assert.ok(isChildFailure(error));assert.equal(error.status,1);assert.match(error.stderr.toString(),/Timed out writing/);return true;
 });
 assert.deepEqual(invoke(f,['peers']).items,[]);
});


// Stop a worker once its first turn completes instead of waiting out --duration-ms.
function runUntilTurn(args: string[],env: NodeJS.ProcessEnv,timeout=15000,turns=1){return new Promise<JsonRecord[]>((resolve,reject)=>{
 const child=spawn(binary,args,{env});let stdout='',stderr='',interrupted=false;
 let forceTimer: NodeJS.Timeout|undefined;const timer=setTimeout(()=>{child.kill('SIGTERM');forceTimer=setTimeout(()=>child.kill('SIGKILL'),2000);},timeout);
 child.stdout.on('data',chunk=>{stdout+=chunk;
  if(!interrupted&&stdout.split('\n').filter(line=>line.includes('"turn-completed"')).length>=turns){interrupted=true;child.kill('SIGINT');}});
 child.stderr.on('data',chunk=>stderr+=chunk);child.on('error',reject);
 child.on('close',code=>{clearTimeout(timer);clearTimeout(forceTimer);
  if(code===0&&interrupted)resolve(stdout.trim().split('\n').map((line): JsonRecord=>JSON.parse(line)));
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
  const quote = (value: string) => "'" + value.replaceAll("'", "'\\''") + "'";
  writeFileSync(join(bin, vendor), `#!/bin/sh\nprintf 'started' > ${quote(capture+'.launch')}\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`, { mode: 0o755 });
  // These Node programs emulate vendor CLIs; they must not inherit the test runner protocol.
  const vendorEnv: NodeJS.ProcessEnv={...process.env,PATH:bin,COMMUNICATION_CAPTURE:capture,...stableVendor(bin,vendor,script)};delete vendorEnv.NODE_TEST_CONTEXT;
  // Separate the host's first-execution checks on this freshly written fixture
  // from protocol assertions. Dedicated tests above cover production startup deadlines.
  const preflightStarted=Date.now();
  try {
   execFileSync(join(bin,vendor),[],{env:vendorEnv,input:'',encoding:'utf8',stdio:['pipe','pipe','pipe'],timeout:30000,maxBuffer:1024*1024});
  } catch(error) {
   t.diagnostic(JSON.stringify({vendor,phase:'fixture-interpreter-preflight',elapsedMs:Date.now()-preflightStarted,wrapperStarted:existsSync(capture+'.launch'),nodeStarted:existsSync(capture+'.boot'),stderr:isChildFailure(error)?error.stderr.toString().slice(-2000):undefined}));
   throw error;
  }
  t.diagnostic(`${vendor} fixture interpreter preflight: ${Date.now()-preflightStarted} ms`);
  for(const suffix of ['.launch','.boot','.frames','.thread',''])rmSync(capture+suffix,{force:true});
  const started=Date.now();
  let events: JsonRecord[];
  try {
   events=await runUntilTurn(['run','--vendor',vendor,'--model','test','--prompt','Task sentinel','--tools','peers,send_message,complete','--trace','--duration-ms','10000','--workspace',f.workspace,'--database',f.database],vendorEnv);
  } catch(error) {
   const boot=existsSync(capture+'.boot')?JSON.parse(readFileSync(capture+'.boot','utf8')):null;
   const frames=existsSync(capture+'.frames')?readFileSync(capture+'.frames','utf8').trim().split('\n').slice(-20).map((line): JsonRecord=>JSON.parse(line)):[];
   t.diagnostic(JSON.stringify({vendor,elapsedMs:Date.now()-started,wrapperStarted:existsSync(capture+'.launch'),nodeBootMs:boot?boot.at-started:null,frames,stderr:isWorkerFailure(error)?error.stderr.slice(-2000):undefined,stdout:isWorkerFailure(error)?error.stdout.slice(-2000):undefined}));
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
  const ready=events.find(e=>e.type==='ready');assert.ok(ready);
  assert.ok(prompt.includes(ready.session));
  assert.ok(prompt.endsWith('User task:\nTask sentinel'));
  const boot=JSON.parse(readFileSync(capture+'.boot','utf8'));
  assert.notEqual(boot.cwd,f.workspace);assert.equal(existsSync(boot.cwd),false,'temporary vendor directory survived teardown');
  assert.equal(events.filter(e=>e.type==='usage').length,1);
  const stateDb=new DatabaseSync(f.database,{readOnly:true});
  assert.equal(only(stateDb.prepare('SELECT status FROM sessions WHERE id=?').get(ready.session)).status,'available');
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
 const quote=(value: string)=>"'"+value.replaceAll("'","'\\''")+"'";
 writeFileSync(join(bin,'claude'),`#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`,{mode:0o755});
 const env: NodeJS.ProcessEnv=fastHeartbeatEnv({...process.env,PATH:bin,COMMUNICATION_PID:pidFile,COMMUNICATION_DB:f.database,...stableVendor(bin,'claude',script)});delete env.NODE_TEST_CONTEXT;
 // Warm only the fixture interpreter; the guard does not create a PID or touch the DB.
 const preflightStarted=Date.now();
 execFileSync(join(bin,'claude'),['--fixture-preflight'],{env,input:'',stdio:['pipe','pipe','pipe'],timeout:30000,maxBuffer:1024*1024});
 t.diagnostic(`idle fixture interpreter preflight: ${Date.now()-preflightStarted} ms`);
 assert.equal(existsSync(pidFile),false);assert.equal(existsSync(f.database),false);
 const started=Date.now();
 try {
  assert.throws(()=>invoke(f,['run','--vendor','claude','--model','test','--prompt','wait','--duration-ms','10000'],{env,timeout:15000}),(e: unknown)=>{
   assert.ok(isChildFailure(e));assert.equal(e.status,1);assert.match(e.stderr.toString(),/database disappeared/);return true;
  });
 } catch(error) {
  t.diagnostic(JSON.stringify({phase:'idle-database-removal',elapsedMs:Date.now()-started,vendorStarted:existsSync(pidFile),databaseExists:existsSync(f.database),stderr:isChildFailure(error)?error.stderr.toString().slice(-2000):undefined}));
  throw error;
 }
 assert.equal(existsSync(f.database),false);
 assert.throws(()=>process.kill(Number(readFileSync(pidFile,'utf8')),0),(e: unknown)=>e instanceof Error&&'code' in e&&e.code==='ESRCH');
});

test('managed run resumes a native-bound identity with its new vendor session', {skip:process.platform==='win32'}, async t=>{
 const f=fixture(t),bin=join(f.workspace,'path');mkdirSync(bin);
 const script=join(bin,'claude.cjs');
 writeFileSync(script, `const rl=require('node:readline').createInterface({input:process.stdin});
 rl.on('line',()=>{console.log(JSON.stringify({type:'system',subtype:'init',session_id:'managed-session'}));console.log(JSON.stringify({type:'result',is_error:false}));});`);
 const quote=(value: string)=>"'"+value.replaceAll("'","'\\''")+"'";
 writeFileSync(join(bin,'claude'),`#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`,{mode:0o755});
 const env: NodeJS.ProcessEnv={...process.env,PATH:bin};delete env.NODE_TEST_CONTEXT;
 const identity=invoke(f,['join',JSON.stringify({name:'resumed-worker',vendor:'claude'})]);
 invoke(f,['attach',JSON.stringify({transport:'claude',endpoint:join(f.workspace,'old.sock'),vendorSession:'old-native-session'}),'--session',identity.id]);
 invoke(f,['leave','--session',identity.id]);
 await runUntilTurn(['run','--vendor','claude','--model','test','--prompt','Complete assigned work','--session',identity.id,'--duration-ms','10000','--workspace',f.workspace,'--database',f.database],env);
 const db=new DatabaseSync(f.database,{readOnly:true});t.after(()=>db.close());
 assert.equal(only(db.prepare('SELECT vendorSession FROM sessions WHERE id=?').get(identity.id)).vendorSession,'managed-session');
 assert.equal(only(db.prepare('SELECT transport FROM attachments WHERE session=?').get(identity.id)).transport,'raw');
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
  const quote=(value: string)=>"'"+value.replaceAll("'","'\\''")+"'";
  writeFileSync(join(bin,'codex'),`#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`,{mode:0o755});
  const env: NodeJS.ProcessEnv={...process.env,PATH:bin,CAPTURE:capture,REJECT_KICKOFF:String(rejectKickoff),...stableVendor(bin,'codex',script)};delete env.NODE_TEST_CONTEXT;
  execFileSync(join(bin,'codex'),[],{env,input:'',timeout:30000});
  const sender=invoke(f,['join',JSON.stringify({name:'supervisor',vendor:'generic',task:'DIRECTORY_SENTINEL'})]),receiver=invoke(f,['join',JSON.stringify({name:'worker',vendor:'codex'})]);
  const sent=invoke(f,['send_message',JSON.stringify({to:receiver.id,body:'PEER_SENTINEL',wake:'action'}),'--session',sender.id]);
  invoke(f,['leave','--session',receiver.id]);
  const args=['run','--vendor','codex','--model','test','--prompt','USER_SENTINEL','--session',receiver.id,'--duration-ms','3000','--workspace',f.workspace,'--database',f.database];
  if(rejectKickoff)await assert.rejects(()=>runUntilTurn(args,env),/worker exited 1/);
  else await runUntilTurn(args,env,10000,2);
  const frames=readFileSync(capture,'utf8').trim().split('\n').map((line): JsonRecord=>JSON.parse(line));
  const turns=frames.filter(x=>x.method==='turn/start');
  assert.match(turns[0].params.input[0].text,/USER_SENTINEL/);
  assert.ok(!JSON.stringify(turns[0]).includes('PEER_SENTINEL'),'peer data cannot be promoted into the user kickoff');
  assert.ok(!JSON.stringify(turns[0]).includes('DIRECTORY_SENTINEL'));
  const injected=frames.find(x=>x.method==='thread/inject_items');assert.ok(injected);
  const directory=injected.params.items[0];
  assert.equal(directory.type,'function_call_output');assert.match(directory.output,/DIRECTORY_SENTINEL/);
  const db=new DatabaseSync(f.database,{readOnly:true});
  try{
   const delivery=db.prepare('SELECT state FROM dispatches WHERE message=?').get(sent.id);
   if(rejectKickoff){assert.equal(turns.length,1);assert.equal(delivery,undefined);}
   else{
    assert.ok(delivery);
    assert.equal(turns.length,2);assert.deepEqual(turns[1].params.input,[]);
    assert.equal(turns[1].params.toolOutput.name,'octocode_peer_messages');
    assert.equal(turns[1].params.toolOutput.output.split('PEER_SENTINEL').length-1,1);
    assert.equal(delivery.state,'submitted');
   }
   assert.equal(only(db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(sent.id)).acknowledgedAt,null);
  }finally{db.close();}
 }
});

test('live presence and lease filters remain separate from declared availability history',t=>{
 const f=fixture(t),a=joinAgent(f,'observer');
 const b=invoke(f,['join',JSON.stringify({name:'worker',vendor:'generic',status:'blocked'})]);
 const lease=invoke(f,['lock',JSON.stringify({path:'owned.txt'}),'--session',b.id]);
 const db=new DatabaseSync(f.database);t.after(()=>db.close());
 db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(b.id);
 const call=(name: string,input: Record<string, unknown>={})=>invoke(f,[name,JSON.stringify(input),'--session',a.id]);
 assert.deepEqual(call('peers').items.map((x: JsonRecord)=>x.id),[a.id]);
 assert.equal(call('fetch',{type:'coordinate.in',from:b.id}).items[0].data.status,'blocked');
 assert.equal(call('fetch',{type:'coordinate.out',from:b.id}).items.length,1);
 assert.equal(call('locks').items.length,0);
 assert.equal(call('locks',{presence:'expired'}).items[0].id,lease.lease.id);
 assert.throws(()=>call('locks',{status:'all'}));
});

test('native peer replay text does not break tool tracing',async()=>{
 const {proxy}=await import('../dist/runtime.js');
 assert.deepEqual(proxy.traceTools('session',{type:'user',message:{content:'peer context'},isReplay:true},new Map()),[]);
 const calls=new Map();
 const records=proxy.traceTools('session',{type:'assistant',message:{content:[{type:'tool_use',id:'tool1',name:'peers',input:{}}]}},calls);
 assert.equal(records.length,1);assert.equal(records[0].type,'tool-call');assert.deepEqual([...calls],[['tool1','peers']]);
});
