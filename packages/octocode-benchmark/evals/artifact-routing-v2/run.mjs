import {mkdirSync,readFileSync,writeFileSync,statSync,openSync,readSync,closeSync} from 'node:fs';
import {homedir} from 'node:os';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {cases} from './cases.mjs';
import {normalize,evaluate,GRADER_REVISION} from './grader.mjs';
import {repairRequest} from './transport.mjs';
import {verify,sha,sourceRoot} from './freeze.mjs';
import {spawnSync} from 'node:child_process';

const args = Object.fromEntries(process.argv.slice(2).map(arg => {
  const [key,...value] = arg.replace(/^--/,'').split('=');return [key,value.join('=')];
}));
const allowed = ['root','subjects','arms','repair','case-ids','max-requests','wall-ms','timeout-ms','context','order','server-log'];
for (const key of Object.keys(args)) if (!allowed.includes(key)) throw Error(`Unknown option: ${key}`);
const root = path.resolve(args.root ?? '.octocode/octocode-eval-benchmark/artifact-search-v2');
if (sourceRoot !== path.join(root,'harness')) {
  const result=spawnSync(process.execPath,[path.join(root,'harness/run.mjs'),...process.argv.slice(2)],{stdio:'inherit'});
  if(result.error) throw result.error;
  process.exit(result.status ?? 1);
}
const frozen = verify(root);
const subjects = (args.subjects ?? 'candidate,baseline').split(',');
const arms = (args.arms ?? 'native,emulated').split(',');
if (subjects.some(s=>!['baseline','candidate'].includes(s)) || arms.some(a=>!['native','emulated'].includes(a))) throw Error('Unsupported subject or arm');
if (new Set(subjects).size!==subjects.length || new Set(arms).size!==arms.length) throw Error('Duplicate subjects or arms');
const repair = Number(args.repair ?? 0);
if (![0,1].includes(repair)) throw Error('Repair must be zero or one');
const maxRequests = Number(args['max-requests'] ?? 80), wallMs = Number(args['wall-ms'] ?? 1080000);
const timeoutMs = Number(args['timeout-ms'] ?? 240000);
for (const [name,value,maximum] of [['max-requests',maxRequests,160],['wall-ms',wallMs,1080000],['timeout-ms',timeoutMs,240000]])
  if (!Number.isSafeInteger(value) || value<1 || value>maximum) throw Error(`Invalid bounded ${name}`);
const selectedIds = args['case-ids']?.split(',');
if (selectedIds?.some(id=>!cases.some(test=>test.id===id))) throw Error('Unknown case identifier');
const selectedCases = selectedIds?cases.filter(test=>selectedIds.includes(test.id)):cases;
const context=Number(args.context ?? 65536), ordering=args.order ?? 'grouped';
if(!Number.isSafeInteger(context) || context<2048 || context>131072) throw Error('Context must be2048–131072');
if(!['grouped','interleaved'].includes(ordering)) throw Error('Order must be grouped or interleaved');
const options = {temperature:0,seed:42,num_ctx:context,num_predict:512};
const settings = {model:'gemma4:latest',think:false,stream:false,truncate:false,shift:false,keep_alive:'5m',options};
const receipts = {};
for (const subject of subjects) {
  const subjectRoot = path.join(root,subject);
  const frozenSchema = await import(pathToFileURL(path.join(subjectRoot,'core-dist/schema.js')));
  // A frozen baseline can predate the public/internal catalog split. Keep that
  // compatibility inside the immutable benchmark reader, never in live core.
  const specs = frozenSchema.DIRECT_TOOL_DEFINITIONS ?? frozenSchema.DIRECT_TOOL_SPECIFICATIONS;
  if (!Array.isArray(specs)) throw Error(`${subject}: frozen core has no tool definitions`);
  receipts[subject] = {snapshot:JSON.parse(readFileSync(path.join(subjectRoot,'snapshot.json'),'utf8')),
    metadata:JSON.parse(readFileSync(path.join(subjectRoot,'metadata.json'),'utf8')),specs};
  for (const test of cases) {
    const reference = evaluate(test,{sensor:true,transport:true,output:test.reference},receipts[subject].specs);
    if (!reference.success) throw Error(`${subject}/${test.id}: reference rejected by preserved validator: ${JSON.stringify(reference)}`);
  }
}
const installed = await (await fetch('http://127.0.0.1:11434/api/tags',{signal:AbortSignal.timeout(5000)})).json();
const digest = installed.models?.find(model=>model.name===settings.model || model.model===settings.model)?.digest;
if (!digest) throw Error('Configured installed model missing');
for (const subject of subjects) {
  const original = receipts[subject].metadata.models.models.find(model=>model.name===settings.model || model.model===settings.model)?.digest;
  if (digest!==original) throw Error(`Model digest changed since ${subject} snapshot`);
  if (!receipts[subject].metadata.model.capabilities.includes('tools')) throw Error('Model does not advertise native tool capability');
}
const runRoot = path.join(root,'runs',new Date().toISOString().replaceAll(':','-'));
mkdirSync(runRoot,{recursive:true});
const runReceipt = {protocol:'canonical-input-routing-v2',graderRevision:GRADER_REVISION,ordering,contextReason:'Full-catalog native rendering measured above32K;64K is the default for this fixture, not a global setting',startedAt:new Date().toISOString(),frozenAt:frozen.createdAt,freezeSha256:sha(path.join(root,'freeze.json')),
  subjects,arms,caseIds:selectedCases.map(test=>test.id),repair,maxRequests,wallMs,timeoutMs,settings,modelDigest:digest,
  limitations:['First-action and validator-repair only; no provider calls are executed.',
    'The all-tools fixture exposes cloning except in its disabled-tool case; live local catalog availability may differ.',
    'Input schemas use canonical Zod input representation; unrepresentable refinements remain checked by preserved executable Zod.',
    'Native tool transport and emulated JSON transport use their normal formatting mechanisms; these are host-mode comparisons.']};
writeFileSync(path.join(runRoot,'receipt.json'),JSON.stringify(runReceipt,null,2));
const start = Date.now(), results = [];let requests = 0,stopReason;
function budgetAvailable() {return requests<maxRequests && Date.now()-start<wallMs;}
function buildRequest(test,subject,arm) {
  const snapshot = receipts[subject].snapshot;
  const tools = snapshot.tools.filter(tool=>!test.disabledTools?.includes(tool.name)).map(tool=>({type:'function',function:{name:tool.name,description:tool.description,parameters:tool.parameters}}));
  const task = 'Choose the immediate next action for a code research assistant. Do not execute tools or claim fetched results. Preserve explicit identifiers, scope, and revision. Include only calls needed now. Tool arguments use the actual queries array envelope. If no available supported tool can answer, briefly explain the capability limit and needed input.';
  const transport = arm==='native'
    ? 'Return native tool calls when needed; otherwise respond briefly in text.'
    : 'Return JSON only: {"calls":[{"tool":"toolName","arguments":{"queries":[{}]}}],"answer":""}. If no call is supported, use calls:[] and explain in answer. Arguments must be objects.';
  const request = {...settings,messages:[{role:'system',content:task+'\n'+transport+'\n'+snapshot.instructions
    +(arm==='emulated'?'\nAVAILABLE TOOL DECLARATIONS:\n'+JSON.stringify(tools):'')},
    {role:'user',content:test.prompt}]};
  if (arm==='native') request.tools=tools;else request.format='json';
  return request;
}
const serverLog = path.resolve(args['server-log'] ?? path.join(homedir(),'.ollama/logs/server.log')); 
statSync(serverLog); // Fail before inference if the scoped truncation sensor is unavailable.
async function generate(request) {
  requests++;const began = Date.now();let raw,error;
  const logStart=statSync(serverLog);
  try {
    const remaining = Math.max(1,Math.min(timeoutMs,wallMs-(Date.now()-start)));
    const response = await fetch('http://127.0.0.1:11434/api/chat',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request),signal:AbortSignal.timeout(remaining)});
    raw = await response.json();if (!response.ok || raw.error) throw Error(raw.error ?? `HTTP ${response.status}`);
  } catch (caught) {error=String(caught);}
  let logEvidence;
  try {
    const logEnd=statSync(serverLog);
    if(logEnd.ino!==logStart.ino || logEnd.size<logStart.size || logEnd.size-logStart.size>4000000) throw Error('Log rotated or exceeded bounded sensor range');
    const fd=openSync(serverLog,'r'), bytes=Buffer.alloc(logEnd.size-logStart.size);
    try {readSync(fd,bytes,0,bytes.length,logStart.size);} finally {closeSync(fd);}
    const warnings=bytes.toString().split('\n').filter(line=>/truncating input prompt|shifting context|context shift.*n_discard/i.test(line));
    logEvidence={path:serverLog,start:logStart.size,end:logEnd.size,warnings};
  } catch(caught) {logEvidence={monitorError:String(caught)};}
  return {raw,error,elapsedMs:Date.now()-began,logEvidence};
}
function scored(test,subject,arm,response) {
  const normalized = normalize(response.raw,arm);
  const raw = response.raw;
  // Near-window inputs cannot establish an untruncated prompt. Never silently score them as model failures.
  const contextRisk = !!response.logEvidence?.monitorError || !!response.logEvidence?.warnings?.length || (Number.isFinite(raw?.prompt_eval_count) && raw.prompt_eval_count>=options.num_ctx-options.num_predict);
  const countersMissing = !Number.isFinite(raw?.prompt_eval_count) || !Number.isFinite(raw?.eval_count);
  if (response.error || contextRisk || countersMissing) normalized.sensor=false;
  const metrics = evaluate(test,normalized,receipts[subject].specs);
  return {normalized,metrics,contextRisk,countersMissing};
}

const schedule = ordering==='grouped'
  ? subjects.flatMap(subject=>arms.flatMap(arm=>selectedCases.map(test=>({subject,arm,test}))))
  : selectedCases.flatMap((test,index)=>{const pairs=subjects.flatMap(subject=>arms.map(arm=>({subject,arm,test})));return index%2?pairs.reverse():pairs;});
outer: for (const {subject,arm,test} of schedule) {
  {
    if (!budgetAvailable()) {stopReason='Configured total request or wall-clock budget reached';break outer;}
    verify(root);
    const request = buildRequest(test,subject,arm);
    const firstResponse = await generate(request), first = scored(test,subject,arm,firstResponse);
    const trace = {id:test.id,subject,arm,request,first:{...firstResponse,...first}};
    let final = first;
    // Repair is triggered only by observable transport/schema feedback, never by expected answers or semantic grade.
    if (repair && first.metrics.sensor && (!first.metrics.schema || !first.metrics.transport) && budgetAvailable()) {
      const request2 = repairRequest(request,firstResponse,first,arm);
      const response2 = await generate(request2), state2 = scored(test,subject,arm,response2);
      trace.repair={request:request2,...response2,...state2};final=state2;
    }
    const row = {id:test.id,subject,arm,first:first.metrics,final:final.metrics,repaired:!!trace.repair,
      elapsedMs:firstResponse.elapsedMs+(trace.repair?.elapsedMs??0)};
    results.push(row);
    writeFileSync(path.join(runRoot,`${test.id}-${subject}-${arm}.json`),JSON.stringify(trace,null,2));
    writeFileSync(path.join(runRoot,'results.json'),JSON.stringify(results,null,2));
    console.log(JSON.stringify(row));
    if (first.contextRisk || final.contextRisk) {stopReason='Truncation, context pressure, or log-monitor failure; instrumentation review required';break outer;}
    if (results.slice(-3).length===3 && results.slice(-3).every(result=>!result.first.sensor)) {stopReason='Three consecutive sensor failures';break outer;}
  }
}
verify(root);
const summaries = subjects.flatMap(subject=>arms.map(arm=>{
  const rows=results.filter(row=>row.subject===subject&&row.arm===arm);
  const sum=stage=>Object.fromEntries(['selection','semantic','schema','transport','sensor','success'].map(metric=>[metric,rows.filter(row=>row[stage][metric]).length]));
  return {subject,arm,total:rows.length,expected:selectedCases.length,first:sum('first'),final:sum('final'),
    repairs:rows.filter(row=>row.repaired).length,unnecessaryCalls:rows.reduce((n,row)=>n+row.first.unnecessaryCalls,0)};
}));
const complete=results.length===selectedCases.length*subjects.length*arms.length;
const summary={runRoot,complete,stopReason,requests,elapsedMs:Date.now()-start,summaries,
  verdict:complete?'Diagnostic completed; compare joint success and all guards before an acceptance claim.':'INCONCLUSIVE: bounded partial diagnostic',
  inferenceScope:'Fresh prospective diagnostic with one model and seed; no end-to-end provider execution.'};
writeFileSync(path.join(runRoot,'summary.json'),JSON.stringify(summary,null,2));
console.log(JSON.stringify(summary));
