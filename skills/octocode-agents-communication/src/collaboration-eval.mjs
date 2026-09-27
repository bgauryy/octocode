// Opt-in real editing task. The deterministic judge lives outside the agents' repository.
import assert from 'node:assert/strict';
import {spawn, execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdirSync, mkdtempSync, readFileSync, writeFileSync, realpathSync, copyFileSync, watch, existsSync, lstatSync} from 'node:fs';
import {join, resolve, relative} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {createServer} from 'node:net';
import {createInterface} from 'node:readline';
import {setTimeout as delay} from 'node:timers/promises';
import {DatabaseSync} from 'node:sqlite';
import { installedBinary } from './artifact-checks.mjs';
import { hookMessages } from './hook-messages.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const variant = process.env.COMMUNICATION_EVAL_VARIANT ?? 'candidate';
const output = resolve(process.env.COMMUNICATION_OUTPUT ?? join(root, '../../.octocode/benchmarks/communication-collaboration/results', `${new Date().toISOString().replaceAll(':','-')}-${variant}`));
mkdirSync(output, {recursive:true});
const workspace = realpathSync(mkdtempSync('/tmp/communication-collaboration-'));
const database = join(workspace, 'audit.sqlite');
const binary = join(workspace, 'communication');
copyFileSync(process.env.COMMUNICATION_BINARY ?? installedBinary(), binary);
const skill = readFileSync(process.env.COMMUNICATION_SKILL ?? join(root, 'SKILL.md'), 'utf8');
const hash = text => createHash('sha256').update(text).digest('hex');
const binding = ['--workspace',workspace,'--database',database];
const call = (name, input={}, session) => JSON.parse(execFileSync(binary,[name,JSON.stringify(input),...binding,...(session?['--session',session]:[])],{encoding:'utf8',timeout:15000}));
const children=[], sockets=[], agents=[];
const report={passed:false,variant,workspace,startedAt:new Date().toISOString(),binarySha256:hash(readFileSync(binary)),skillSha256:hash(skill),skillBytes:Buffer.byteLength(skill),harnessSha256:hash(readFileSync(fileURLToPath(import.meta.url))),models:{claude:'haiku',codex:'gpt-6-luna'},changes:[],scope:'Two real vendor agents edit a disposable shared repository with production communication APIs and tools. One task smoke test; not a statistical reliability or speed comparison.'};
writeFileSync(join(output,'harness.mjs'),readFileSync(fileURLToPath(import.meta.url)));
writeFileSync(join(output,'skill.md'),skill);
mkdirSync(join(workspace,'src'));
const source=join(workspace,'src/invoice.mjs');
writeFileSync(source,'export function subtotal(lines) { return 0; }\n');
report.initialHash=hash(readFileSync(source));
execFileSync('git',['init','--quiet'],{cwd:workspace});
async function until(predicate,label,timeout=240000){const deadline=Date.now()+timeout;while(Date.now()<deadline){for(const item of children)if(item.error||item.child.exitCode!==null||item.child.signalCode!==null)throw Error(`${item.name} exited: ${item.error??item.stderr}`);const value=await predicate();if(value)return value;await delay(100);}throw Error(`Timed out: ${label}`);}
function start(name,command,args,env={}){const child=spawn(command,args,{cwd:workspace,detached:true,env:{...process.env,...env}});const item={name,child,events:[],stderr:'',send:v=>child.stdin.write(`${JSON.stringify(v)}\n`)};children.push(item);child.on('error',e=>{item.error=e.message;});child.stdin.on('error',()=>{});child.stderr.on('data',d=>{item.stderr=(item.stderr+d).slice(-32768);});createInterface({input:child.stdout}).on('line',line=>{try{item.events.push(JSON.parse(line));}catch{}});return item;}
async function connect(url){const socket=new WebSocket(url);sockets.push(socket);await new Promise((r,j)=>{socket.addEventListener('open',r,{once:true});socket.addEventListener('error',j,{once:true});});const events=[];let sequence=0;socket.addEventListener('message',e=>events.push(JSON.parse(e.data)));const send=v=>socket.send(JSON.stringify(v));return{events,send,async request(method,params){const id=++sequence;send({id,method,params});const reply=await until(()=>events.find(e=>e.id===id),method);if(reply.error)throw Error(JSON.stringify(reply.error));return reply.result;}};}
let controller,db,timer,watcher;
try{
  execFileSync(binary,['--help'],{timeout:60000,stdio:'ignore'});
  controller=call('join',{name:'controller',vendor:'test-host'});
  call('attach',{transport:'raw'},controller.id);
  const producer={...call('join',{name:'subtotal-engineer',vendor:'claude'}),vendor:'claude'};
  const consumer={...call('join',{name:'discount-engineer',vendor:'codex'}),vendor:'codex'};
  agents.push(producer,consumer);
  db=new DatabaseSync(database,{readOnly:true});
  timer=setInterval(()=>{try{for(const agent of [controller,...agents])call('heartbeat',{},agent.id);}catch(e){report.heartbeatError=e.message;}},10000);
  const previous=new Map([[source,report.initialHash]]);
  const ignored=path=>['.git','.octocode','communication','claude.sock'].includes(path.split('/')[0])||path.startsWith('audit.sqlite');
  watcher=watch(workspace,{recursive:true},(_event,filename)=>{try{
    if(!filename||ignored(String(filename)))return;
    const path=join(workspace,String(filename));
    if(existsSync(path)&&!lstatSync(path).isFile())return;
    const next=existsSync(path)?hash(readFileSync(path)):null;
    if(next===(previous.get(path)??null))return;
    previous.set(path,next);
    const leases=db.prepare('SELECT owner,path,kind,reasoning,expiresAt FROM leases WHERE expiresAt>?').all(Date.now());
    const covering=leases.filter(l=>l.path===path||(l.kind==='tree'&&path.startsWith(l.path+'/')));
    report.changes.push({at:Date.now(),path:relative(workspace,path),hash:next,leases:covering});
  }catch(error){report.watcherError=error.message;}});
  const requirements='Authorized repository task: edit only this temporary workspace. Implement src/invoice.mjs ES-module functions. subtotal(lines) sums priceCents * quantity; require an array, nonnegative safe-integer priceCents and positive safe-integer quantity, and reject unsafe aggregate totals. total(lines, discountPercent=0) uses subtotal, accepts integer discounts from0 through100 and rounds the discounted subtotal to the nearest integer cent. Invalid inputs must throw. Preserve collaborators\' changes. You may create/run local tests. Never git commit or stash. Discover peers and coordinate overlapping work according to the supplied skill.';
  const common=`${skill}\n\n${requirements}\nThis is an existing bound identity with automatic native delivery. Initially respond READY to the host, without sending messages or editing. Act when a message assigns your stage. End each turn after handling its work; do not poll. Send DONE plus evidence as a notice (replyRequired:false) to controller ${controller.id} after your stage. No external config changes or new agents.`;
  const producerTask=`${common}\nYour stage: on START, implement and test subtotal only. Coordinate with discount-engineer, who will later extend the same file. Publish an immutable handoff document including the interface, tests and any remaining work, release your write leases and send its name to discount-engineer with an actionable request to implement total. Do not implement total yourself. Handle later informational responses without reply loops.`;
  const consumerTask=`${common}\nYour stage: wait for subtotal-engineer's handoff. Read the referenced document, then implement and test total in the shared module while preserving subtotal. Complete the producer request with an evidence reply; send the controller a DONE notice with replyRequired:false. The producer's handoff is your work request; no extra host prompt will follow.`;
  const mcp=session=>({command:binary,args:['mcp',...binding,'--session',session]});
  const endpoint=join(workspace,'claude.sock');
  producer.process=start('claude','claude',['-p','--model','haiku','--input-format','stream-json','--output-format','stream-json','--verbose','--setting-sources','','--strict-mcp-config','--mcp-config',JSON.stringify({mcpServers:{communication:mcp(producer.id)}}),'--tools','Read,Write,Edit,Bash','--allowedTools','Read','Write','Edit','Bash','mcp__communication__*','--permission-mode','dontAsk','--disable-slash-commands','--no-session-persistence','--messaging-socket-path',endpoint,'--settings',JSON.stringify({disableAllHooks:true,autoMemoryEnabled:false,crossSessionInbound:'accept'}),'--system-prompt',producerTask]);
  producer.process.send({type:'user',message:{role:'user',content:'Initialize; respond READY only.'}});
  await until(()=>producer.process.events.some(e=>e.type==='result'),'Claude ready');
  producer.vendorSession=producer.process.events.find(e=>e.type==='system'&&e.subtype==='init').session_id;
  call('attach',{transport:'claude',endpoint,vendorSession:producer.vendorSession},producer.id);
  const reserve=createServer();await new Promise(r=>reserve.listen(0,'127.0.0.1',r));const port=reserve.address().port;await new Promise(r=>reserve.close(r));
  start('codex-server','codex',['app-server','--listen',`ws://127.0.0.1:${port}`]);
  await until(async()=>{try{return(await fetch(`http://127.0.0.1:${port}/readyz`)).ok;}catch{return false;}},'Codex server');
  const cx=consumer.rpc=await connect(`ws://127.0.0.1:${port}`);
  await cx.request('initialize',{clientInfo:{name:'collaboration-eval',version:'1'},capabilities:{experimentalApi:true}});cx.send({method:'initialized',params:{}});
  const {config}=await cx.request('config/read',{includeLayers:false});
  const disabled=v=>Object.fromEntries(Object.keys(v||{}).map(k=>[k,{enabled:false}]));
  const skills=await cx.request('skills/list',{cwds:[workspace],forceReload:true});
  const {thread}=await cx.request('thread/start',{model:'gpt-6-luna',cwd:workspace,ephemeral:true,approvalPolicy:'never',sandbox:'workspace-write',baseInstructions:consumerTask,developerInstructions:'',config:{mcp_servers:{...disabled(config.mcp_servers),communication:{...mcp(consumer.id),enabled:true}},plugins:disabled(config.plugins),project_doc_max_bytes:0,skills:{config:skills.data.flatMap(e=>e.skills.map(s=>({path:s.path,enabled:false})))},web_search:'disabled',features:{code_mode:{enabled:false},shell_tool:true,apply_patch_freeform:true,multi_agent:false,memories:false,hooks:false,apps:false,skill_search:false}}});
  consumer.vendorSession=thread.id;
  await until(async()=>{const response=await cx.request('mcpServerStatus/list',{threadId:thread.id,detail:'toolsAndAuthOnly'});const server=response.data.find(s=>s.name==='communication');return server&&!server.toolsError&&Object.values(server.tools).some(t=>t.name==='lock');},'Codex tools');
  call('attach',{transport:'codex',endpoint:`ws://127.0.0.1:${port}`,vendorSession:thread.id},consumer.id);
  for(const agent of agents){agent.listener=start(`${agent.vendor}-listener`,binary,['listen',...binding,'--session',agent.id]);await until(()=>agent.listener.events.some(e=>e.type==='listening'),'listener ready');}
  const began=performance.now();
  call('send_message',{to:producer.id,replyRequired:false,body:'START: implement your subtotal stage, then hand off to discount-engineer as assigned.',key:'start',reasoning:'Begin the shared repository feature and dependency handoff',wake:'action'},controller.id);
  const done=new Set();
  await until(()=>{for(const message of hookMessages(call('hook',{format:'json'},controller.id))){if(message.body.startsWith('DONE'))done.add(message.sender);call('complete',{message:message.id},controller.id);}return done.size===2;},'both collaborators finish');
  await until(()=>{for(const message of call('hook',{format:'json'},controller.id).items)call('complete',{message:message.id},controller.id);return db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n===0;},'all messages handled',90000);
  report.taskMs=performance.now()-began;
  const module=await import(`${pathToFileURL(source)}?v=${Date.now()}`);
  assert.equal(module.subtotal([{priceCents:199,quantity:2},{priceCents:250,quantity:1}]),648);
  assert.equal(module.subtotal([]),0);assert.equal(module.total([{priceCents:105,quantity:1}],10),95);assert.equal(module.total([{priceCents:999,quantity:3}],100),0);assert.equal(module.total([{priceCents:137,quantity:3}]),411);
  for(const value of [null,{},'items'])assert.throws(()=>module.subtotal(value));
  for(const item of [{priceCents:-1,quantity:1},{priceCents:1.5,quantity:1},{priceCents:1,quantity:0},{priceCents:1,quantity:1.5},{priceCents:Number.MAX_SAFE_INTEGER,quantity:2}])assert.throws(()=>module.subtotal([item]));
  assert.throws(()=>module.subtotal([{priceCents:Number.MAX_SAFE_INTEGER,quantity:1},{priceCents:1,quantity:1}]));
  for(const discount of [-1,101,0.5,'10',NaN])assert.throws(()=>module.total([],discount));
  report.functionalTestsPassed=true;report.testCases=19;report.finalHash=hash(readFileSync(source));assert.notEqual(report.finalHash,report.initialHash);
  report.audit=db.prepare('SELECT * FROM audit ORDER BY id').all().map(r=>({...r,data:JSON.parse(r.data)}));
  report.uncoveredWrites=report.changes.filter(c=>c.leases.length===0);
  for(const agent of agents){assert.ok(report.audit.some(e=>e.session===agent.id&&e.kind==='lease.acquired'&&e.data.reasoning),'Each editor must acquire an intentional lease');assert.ok(report.changes.some(c=>c.leases.some(l=>l.owner===agent.id)),'Observe each editor changing the file while holding a lease');}
  assert.equal(db.prepare('SELECT count(*) n FROM leases').get().n,0,'All leases explicitly released');
  const documents=report.audit.filter(e=>e.kind==='document.created');assert.ok(documents.length>0,'Shared handoff document');
  const completed=cx.events.filter(e=>e.method==='item/completed').map(e=>e.params.item);
  report.documentReadObserved=completed.some(item=>(item.tool==='read_document'&&item.result&&!item.error)||(item.type==='commandExecution'&&item.exitCode===0&&item.command.includes('read_document')&&item.aggregatedOutput.includes('\"document\"')));
  assert.ok(report.documentReadObserved,'Consumer actually read handoff using MCP or audited CLI');
  report.messages=db.prepare('SELECT * FROM messages').all();assert.ok(report.messages.every(m=>m.reasoning?.trim()));
  for(const event of producer.process.events)if(event.type==='result'&&event.usage)(report.claudeUsage??=[]).push(event.usage);
  report.codexUsage=cx.events.findLast(e=>e.method==='thread/tokenUsage/updated')?.params.tokenUsage;
  assert.equal(report.heartbeatError,undefined);assert.equal(report.watcherError,undefined);assert.equal(report.uncoveredWrites.length,0,'No observed unleased repository writes');report.passed=true;
}catch(error){report.error=error.stack;process.exitCode=1;}
finally{
  clearInterval(timer);watcher?.close();for(const socket of sockets)socket.close();
  for(const item of children.reverse()){try{process.kill(-item.child.pid,'SIGTERM');}catch{}const end=Date.now()+2000;while(item.child.exitCode===null&&item.child.signalCode===null&&Date.now()<end)await delay(50);if(item.child.exitCode===null&&item.child.signalCode===null){try{process.kill(-item.child.pid,'SIGKILL');}catch{}await delay(100);}writeFileSync(join(output,`${item.name}-events.json`),JSON.stringify(item.events));writeFileSync(join(output,`${item.name}-stderr.txt`),item.stderr);}
  report.childrenReaped=children.every(item=>!item.child.pid||item.child.exitCode!==null||item.child.signalCode!==null);if(!report.childrenReaped){report.passed=false;process.exitCode=1;}
  for(const agent of [controller,...agents].filter(Boolean)){try{call('leave',{},agent.id);}catch(e){report.cleanupError=e.message;report.passed=false;process.exitCode=1;}}
  if(db){report.pending=db.prepare('SELECT message,recipient FROM deliveries WHERE acknowledgedAt IS NULL').all();report.audit??=db.prepare('SELECT * FROM audit ORDER BY id').all().map(r=>({...r,data:JSON.parse(r.data)}));report.messages??=db.prepare('SELECT * FROM messages').all();report.uncoveredWrites=report.changes.filter(c=>c.leases.length===0);db.close();}
  try{execFileSync(binary,['db','export',JSON.stringify({path:join(output,'audit.sqlite')}),'--database',database]);copyFileSync(source,join(output,'invoice.mjs'));}catch(e){report.snapshotError=e.message;report.passed=false;process.exitCode=1;}
  for(const agent of agents.filter(a=>a.rpc))writeFileSync(join(output,`${agent.vendor}-api-events.json`),JSON.stringify(agent.rpc.events));
  report.completedAt=new Date().toISOString();writeFileSync(join(output,'result.json'),JSON.stringify(report,null,2));console.log(JSON.stringify({output,passed:report.passed,error:report.error,taskMs:report.taskMs,skillBytes:report.skillBytes,childrenReaped:report.childrenReaped}));
}
