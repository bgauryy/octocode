import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {build} from 'esbuild';
import {propagateOctocodeEnv,loadOctocodeEnv,getOctocodeHome} from '@octocodeai/config';

const here=path.dirname(fileURLToPath(import.meta.url));
const root=path.resolve(here,'../../../..');
const home=path.join(root,'.octocode/octocode-eval-benchmark/jev-tool-terra-30-2026-09-20-v6');
const digest=file=>crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const read=file=>JSON.parse(fs.readFileSync(file,'utf8'));
const save=(file,value)=>fs.writeFileSync(file,JSON.stringify(value,null,2)+'\n');
const answerSchema={type:'object',additionalProperties:false,required:['answer','citations','limitations','jevAssessment'],properties:{
  answer:{type:'string'},citations:{type:'array',items:{type:'object',additionalProperties:false,required:['url','claim'],properties:{url:{type:'string'},claim:{type:'string'}}}},
  limitations:{type:'array',items:{type:'string'}},jevAssessment:{type:'string',description:'Briefly state whether Jev changed the next action; say unused when no call was useful. This is self-report, not measured savings.'}
}};
const budget={seconds:300,ordinaryQueries:40,jevQueries:20,hostTokenEligibilityCap:250000};
const model='gpt-5.6-terra';

function verify(){
  const manifest=read(path.join(home,'freeze.json'));
  for(const [file,hash]of Object.entries(manifest.files))if(digest(path.join(home,file))!==hash)throw new Error('Frozen artifact changed: '+file);
  if(digest(fileURLToPath(import.meta.url))!==manifest.files['snapshot/campaign.source.mjs'])throw new Error('Campaign controller changed after freeze');
  return manifest;
}

async function freeze(){
  if(fs.existsSync(path.join(home,'freeze.json')))throw new Error('Campaign is already frozen');
  fs.mkdirSync(path.join(home,'snapshot'),{recursive:true});
  fs.mkdirSync(path.join(home,'questions'),{recursive:true});
  const options={bundle:true,platform:'node',format:'esm',target:'node24',banner:{js:'import {createRequire as __benchRequire} from "node:module"; const require=__benchRequire(import.meta.url);'}};
  // Reuse the already verified tool build; concurrent workspace builds must not enter a case snapshot.
  const prior=path.join(root,'.octocode/octocode-eval-benchmark/jev-tool-terra-30-2026-09-20-v4');
  const priorManifest=read(path.join(prior,'freeze.json'));
  for(const name of ['mcp.mjs','runtime.node','octocode','octocode-regex-worker']){
    const source=path.join(prior,'snapshot',name);
    if(digest(source)!==priorManifest.files[`snapshot/${name}`])throw new Error('Verified tool snapshot changed: '+name);
    fs.copyFileSync(source,path.join(home,'snapshot',name));
  }
  await build({...options,entryPoints:[path.join(here,'mcp-proxy.mjs')],outfile:path.join(home,'snapshot/proxy.mjs')});
  await build({...options,entryPoints:[path.join(here,'appserver-runner.mjs')],outfile:path.join(home,'snapshot/runner.mjs')});
  for(const id of Array.from({length:30},(_,i)=>i+1))fs.copyFileSync(path.join(root,`packages/octocode-benchmark/compare/github-questions/Q${id}.md`),path.join(home,`questions/Q${id}.md`));
  fs.copyFileSync(fileURLToPath(import.meta.url),path.join(home,'snapshot/campaign.source.mjs'));
  fs.copyFileSync(path.join(here,'PROTOCOL.md'),path.join(home,'PROTOCOL.md'));
  save(path.join(home,'answer-schema.json'),answerSchema);
  const files={};
  for(const directory of ['snapshot','questions'])for(const name of fs.readdirSync(path.join(home,directory)))files[`${directory}/${name}`]=digest(path.join(home,directory,name));
  for(const name of ['PROTOCOL.md','answer-schema.json'])files[name]=digest(path.join(home,name));
  save(path.join(home,'freeze.json'),{version:1,created:new Date().toISOString(),model,effort:'medium',arm:'candidate-only',budget,node:process.version,codex:execFileSync('codex',['--version'],{encoding:'utf8'}).trim(),repoHead:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),files});
  console.log(JSON.stringify({frozen:home,files:Object.keys(files).length}));
}

function runtimeEnv(runDir){
  const env={...process.env};
  propagateOctocodeEnv({env,trusted:false,cwd:root});
  // Match native home-tier Jev policy; generic JS propagation still protects these keys.
  const globalConfig=loadOctocodeEnv({home:getOctocodeHome(),trusted:false}).map;
  for(const key of ['OCTOCODE_CLASSIFICATION_API','OCTOCODE_CLASSIFICATION_API_HOST'])if(!env[key]&&globalConfig[key])env[key]=globalConfig[key];
  if(!env.OCTOCODE_CLASSIFICATION_API)throw new Error('Jev credential unavailable before launch');
  // Resolve the already-authorized GitHub CLI credential without writing/logging it.
  if(!env.GITHUB_TOKEN&&!env.GH_TOKEN){try{env.GITHUB_TOKEN=execFileSync('gh',['auth','token'],{encoding:'utf8',stdio:['ignore','pipe','pipe']}).trim();}catch{throw new Error('GitHub credential unavailable before launch');}}
  Object.assign(env,{OCTOCODE_HOME:path.join(runDir,'octocode-home'),OCTOCODE_NATIVE_BINDING:path.join(home,'snapshot/runtime.node'),OCTOCODE_REGEX_WORKER:path.join(home,'snapshot/octocode-regex-worker'),ENABLE_LOCAL:'false',ENABLE_CLONE:'false',MAX_RETRIES:'0',OCTOCODE_ENABLE_STATS:'true',OCTOCODE_STORAGE_MODE:'persistent'});
  fs.mkdirSync(env.OCTOCODE_HOME,{recursive:true});
  return env;
}

async function run(id){
  if(!/^Q([1-9]|[12][0-9]|30)$/.test(id)&&id!=='preflight')throw new Error('Unknown case');
  verify();
  const runDir=path.join(home,'runs',id);fs.mkdirSync(runDir,{recursive:true});
  fs.writeFileSync(path.join(runDir,'RESERVED'),new Date().toISOString(),{flag:'wx'});
  const cwd=fs.mkdtempSync(path.join(os.tmpdir(),`jev-terra-${id}-`));
  const config={version:1,arm:'candidate',entrypoint:path.join(home,'snapshot/mcp.mjs'),runDir,cwd,requestTimeoutMs:120000};
  const configPath=path.join(runDir,'proxy-config.json');save(configPath,config);
  const env=runtimeEnv(runDir);env.JEV_BENCH_CONFIG=configPath;
  const question=id==='preflight'?'Infrastructure check: use ghGetFileContent to read line 1 of README in octocat/Hello-World at commit 7fd1a60b01f91b314f59955a4e4d4e80d8edf11d. Report what the tool actually returns, including any error. Do not use Jev unless it helps.':fs.readFileSync(path.join(home,'questions',`${id}.md`),'utf8');
  const prompt=`Answer this GitHub research question using the available Octocode MCP tools.\n\n${question}\n\nUse the actual tool descriptions and schemas; no skills, shell, browser, other connectors, or subagents. No benchmark files, historical answers, or grading material are available through the tools. Research efficiently and stop when sufficient evidence answers the question. Cite deciding source lines and resolved revisions where available. Preserve partial coverage and errors; do not infer global absence from a bounded search.\n\nJev is optional. Use it when its judgment changes the next action or avoids substantial reading; do not call it to satisfy a quota. Source evidence and checks establish facts. This is a tool-only evaluation with no Jev skill.\n\nBudget: ${budget.seconds}s, ${budget.ordinaryQueries} ordinary queries including nested Jev retrievals, ${budget.jevQueries} Jev questions. No minimum calls. Return a concise answer, citations, limitations, and a brief Jev-use assessment. Do not narrate these benchmark instructions.`;
  fs.writeFileSync(path.join(runDir,'prompt.txt'),prompt);
  const {runAppServer}=await import(pathToFileURL(path.join(home,'snapshot/runner.mjs')).href);
  const execution=await runAppServer({cwd,env,model,effort:'medium',prompt,outputSchema:answerSchema,runDir,proxyPath:path.join(home,'snapshot/proxy.mjs'),deadlineMs:budget.seconds*1000});
  const {usage,prohibitedToolEvents}=execution;
  const receipts=fs.existsSync(path.join(runDir,'calls.jsonl'))?fs.readFileSync(path.join(runDir,'calls.jsonl'),'utf8').trim().split('\n').map(JSON.parse):[];
  const catalogPresent=receipts.some(r=>r.event==='catalog'&&r.tools?.some(t=>t.name==='jev'));
  let answerValid=false;try{const a=read(path.join(runDir,'answer.json'));answerValid=typeof a.answer==='string'&&a.answer.trim().length>0&&typeof a.jevAssessment==='string'&&Array.isArray(a.limitations)&&a.limitations.every(x=>typeof x==='string')&&Array.isArray(a.citations)&&a.citations.every(x=>typeof x.url==='string'&&typeof x.claim==='string');}catch{}
  const usageValid=usage.length===1&&[usage[0]?.input_tokens,usage[0]?.output_tokens].every(n=>Number.isSafeInteger(n)&&n>=0)&&
    (usage[0].cached_input_tokens==null||(Number.isSafeInteger(usage[0].cached_input_tokens)&&usage[0].cached_input_tokens>=0&&usage[0].cached_input_tokens<=usage[0].input_tokens));
  const hostTokenEligible=usageValid&&usage[0].input_tokens+usage[0].output_tokens<=budget.hostTokenEligibilityCap;
  let intact=true;try{verify()}catch{intact=false}
  const record={id,arm:'candidate',requestedModel:model,effort:'medium',...execution,runtimeUnchanged:intact,catalogPresent,answerValid,hostTokenEligible,answerPresent:fs.existsSync(path.join(runDir,'answer.json'))};
  save(path.join(runDir,'record.json'),record);console.log(JSON.stringify(record));
  if(!intact||prohibitedToolEvents||!usageValid||!catalogPresent)throw new Error('Instrumentation/adherence gate failed; inspect preserved run');
  if(id==='preflight'&&(execution.exitCode!==0||execution.timedOut||!answerValid||!hostTokenEligible||!receipts.some(r=>r.event==='call'&&r.name==='ghGetFileContent'&&!r.isError&&r.errorRows?.length===0)))throw new Error('Infrastructure preflight failed; inspect preserved run');
}

const [command,id]=process.argv.slice(2);
if(command==='freeze')await freeze();
else if(command==='run'){
  try{await run(id);}catch(error){
    const dir=path.join(home,'runs',String(id));
    if(fs.existsSync(path.join(dir,'RESERVED'))&&!fs.existsSync(path.join(dir,'record.json')))
      save(path.join(dir,'record.json'),{id,arm:'candidate',requestedModel:model,exitCode:1,usage:[],answerValid:false,hostTokenEligible:false,error:'Runner setup or execution failed; inspect retained receipts. Missing usage is unknown.'});
    throw error;
  }
}
else if(command==='verify')console.log(JSON.stringify({pass:true,...verify()}));
else throw new Error('Use freeze, verify, or run Q1..Q30/preflight. No default launches.');
