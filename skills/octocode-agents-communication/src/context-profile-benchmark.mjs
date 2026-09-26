// Matched native trials. One source harness and binary; only skill scoping differs.
import {spawn} from 'node:child_process';
import {createHash} from 'node:crypto';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const root=fileURLToPath(new URL('../',import.meta.url));
const hash=value=>createHash('sha256').update(value).digest('hex');
const sum=(rows,key)=>rows.length&&rows.every(r=>Number.isFinite(r[key])&&r[key]>=0)?rows.reduce((n,r)=>n+r[key],0):null;
export function normalizeUsage(vendor,rows){
 const input=sum(rows,'inputTokens'),cached=sum(rows,'cachedInputTokens'),writes=sum(rows,'cacheWriteTokens'),output=sum(rows,'outputTokens');
 return {input:vendor==='claude'?(input===null||cached===null||writes===null?null:input+cached+writes):input,cached,writes,output};
}
const percentile=(values,p)=>{const x=[...values].sort((a,b)=>a-b);return x.length?x[Math.ceil(p*x.length)-1]:null;};
const median=values=>{const x=[...values].sort((a,b)=>a-b);return x.length%2?x[(x.length-1)/2]:(x[x.length/2-1]+x[x.length/2])/2;};
const successful=run=>run?.code===0&&run.passed===true&&run.childrenReaped===true&&run.pending===0;
export function evaluate(runs){
 const pairs=[];
 for(const family of ['review','handoff'])for(const pair of [1,2]){
  const a=runs.filter(r=>r.family===family&&r.pair===pair&&r.arm==='full'),b=runs.filter(r=>r.family===family&&r.pair===pair&&r.arm==='scoped');
  if(a.length===1&&b.length===1)pairs.push([a[0],b[0]]);
 }
 const result={completed:runs.length===8&&pairs.length===4,allPassed:runs.length>0&&runs.every(successful),vendors:{}};
 const timed=pairs.filter(pair=>pair.every(r=>successful(r)&&Number.isFinite(r.handledRoundMs)&&r.handledRoundMs>0));
 const fullP95=percentile(timed.map(([a])=>a.handledRoundMs),.95),scopedP95=percentile(timed.map(([,b])=>b.handledRoundMs),.95);
 result.latency={validPairs:timed.length,fullP95,scopedP95,accepted:result.completed&&result.allPassed&&timed.length===4&&scopedP95<=fullP95*1.2};
 for(const vendor of ['claude','codex','grok']){
  const ratios=pairs.filter(pair=>pair.every(successful)).flatMap(([a,b])=>{
   const x=a.usage?.[vendor]?.input,y=b.usage?.[vendor]?.input;
   return Number.isFinite(x)&&x>0&&Number.isFinite(y)&&y>=0?[1-y/x]:[];
  });
  result.vendors[vendor]={validPairs:ratios.length,pairedReductions:ratios,medianInputReduction:ratios.length?median(ratios):null,accepted:result.completed&&result.allPassed&&result.latency.accepted&&ratios.length===4&&median(ratios)>=.2};
 }
 return result;
}
export function summarize(directory){
 const report=JSON.parse(readFileSync(join(directory,'result.json')));
 const usage={};
 for(const vendor of ['claude','codex','grok']){
  const rows=(report.observedUsage??[]).filter(r=>r.vendor===vendor);
  // The Grok dispatcher sees work turns; the owning socket separately sees READY.
  if(vendor==='grok')for(const event of JSON.parse(readFileSync(join(directory,'grok-1-api-events.json')))){
   const u=event.result?._meta?.usage;
   if(event.result?.stopReason&&u)rows.push({inputTokens:u.inputTokens,outputTokens:u.outputTokens,cachedInputTokens:u.cachedReadTokens,cacheWriteTokens:u.cacheCreationTokens});
  }
  usage[vendor]=normalizeUsage(vendor,rows);
 }
 return {passed:report.passed,childrenReaped:report.childrenReaped,pending:report.pending.length,usage,questionRoundMs:report.questionRoundMs,handledRoundMs:report.handledRoundMs,toolCalls:report.recipientToolCalls,profiles:report.profiles,error:report.error};
}
export async function main(){
 const output=resolve(process.env.COMMUNICATION_OUTPUT??join(root,'../../.octocode/benchmarks/communication-context-v2/profiles'));
 const binary=resolve(process.env.COMMUNICATION_BINARY??join(root,'scripts/octocode-agents-communication'));
 mkdirSync(output,{recursive:true});
 const schedule=[];
 for(const family of ['review','handoff'])for(const pair of [1,2])for(const arm of pair===1?['full','scoped']:['scoped','full'])schedule.push({family,pair,arm});
 const contract={version:1,binarySha256:hash(readFileSync(binary)),harnessSha256:hash(readFileSync(join(root,'src/service-mesh.mjs'))),driverSha256:hash(readFileSync(fileURLToPath(import.meta.url))),schedule,turnObserverSha256:hash(readFileSync(join(root,'src/native-turns.mjs'))),primary:'Per-vendor median paired reduction of observed total input >=20%, all work successful',guards:['All directed questions/replies, peer docs and broadcast handled','No pending ACKs or duplicate messages','Owned children reaped','No retry; retain failed/incomplete trials','Shared group completion p95 must not regress more than 20%; four samples per arm make this a coarse guard, not a stable population percentile'],scope:'Three native participants per task; task/mode/binary/tools/hooks fixed within pairs; only vendor-scoped skill differs. Shared provider caches and system load are not controlled. Exploratory AB/BA, not a sealed production reliability certification.',timeoutMsPerRun:420000};
 writeFileSync(join(output,'contract.json'),JSON.stringify(contract,null,2));
 const runs=[];
 for(const item of schedule){
  if(hash(readFileSync(binary))!==contract.binarySha256||hash(readFileSync(join(root,'src/service-mesh.mjs')))!==contract.harnessSha256||hash(readFileSync(join(root,'src/native-turns.mjs')))!==contract.turnObserverSha256)throw Error('Frozen subject/harness changed');
  const directory=join(output,`${item.family}-${item.pair}-${item.arm}`);mkdirSync(directory,{recursive:true});
  const child=spawn(process.execPath,[join(root,'src/service-mesh.mjs')],{cwd:root,env:{...process.env,COMMUNICATION_BINARY:binary,COMMUNICATION_OUTPUT:directory,COMMUNICATION_VENDORS:'claude,codex,grok',COMMUNICATION_COPIES:'1',COMMUNICATION_AGENT_ORIGINATED:'1',COMMUNICATION_COMPLETION_CHECK:'1',COMMUNICATION_SCOPED_SKILL:item.arm==='scoped'?'1':'0',COMMUNICATION_TASK_FAMILY:item.family,COMMUNICATION_EXPECTED_BINARY_SHA256:contract.binarySha256},stdio:['ignore','pipe','pipe']});
  let stderr='';child.stdout.resume();child.stderr.on('data',v=>stderr=(stderr+v).slice(-65536));
  const deadline=setTimeout(()=>child.kill('SIGTERM'),contract.timeoutMsPerRun);
  const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('close',resolve);});clearTimeout(deadline);
  writeFileSync(join(directory,'driver-stderr.txt'),stderr);
  let result;try{result=summarize(directory);}catch(error){result={passed:false,error:error.message};}
  runs.push({...item,code,...result});writeFileSync(join(output,'runs.json'),JSON.stringify(runs,null,2));
  console.log(JSON.stringify(runs.at(-1)));
  // A failed task cannot establish token savings; stop spending on an invalid comparison.
  if(code!==0||!result.passed||!result.childrenReaped||result.pending)break;
 }
 const result=evaluate(runs);
 writeFileSync(join(output,'result.json'),JSON.stringify(result,null,2));console.log(JSON.stringify({output,...result}));
}
if(process.argv[1]===fileURLToPath(import.meta.url))await main();
