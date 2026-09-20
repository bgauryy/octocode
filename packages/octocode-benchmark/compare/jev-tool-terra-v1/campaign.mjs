import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import {spawn,execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {build} from 'esbuild';
import {propagateOctocodeEnv,loadOctocodeEnv,getOctocodeHome} from '@octocodeai/config';

const here=path.dirname(fileURLToPath(import.meta.url));
const root=path.resolve(here,'../../../..');
const home=path.join(root,'.octocode/octocode-eval-benchmark/jev-tool-terra-30-2026-09-20-v4');
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
  await build({...options,entryPoints:[path.join(root,'packages/octocode-mcp/dist/index.js')],outfile:path.join(home,'snapshot/mcp.mjs'),external:['@octocodeai/octocode-native/runtime']});
  await build({...options,entryPoints:[path.join(here,'mcp-proxy.mjs')],outfile:path.join(home,'snapshot/proxy.mjs')});
  const suffix=`${process.platform}-${process.arch}`;
  if(suffix!=='darwin-arm64')throw new Error('Explicit snapshot mapping required for this host');
  for(const [source,target]of [
    ['packages/octocode-native/octocode-native.darwin-arm64.node','runtime.node'],
    ['packages/octocode-native/npm/darwin-arm64/octocode','octocode'],
    ['packages/octocode-native/npm/darwin-arm64/octocode-regex-worker','octocode-regex-worker'],
  ])fs.copyFileSync(path.join(root,source),path.join(home,'snapshot',target));
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
  for(const key of ['OCTOCODE_JEV_KEY','OCTOCODE_JEV_MODEL','OCTOCODE_JEV_BASE_URL'])if(!env[key]&&globalConfig[key])env[key]=globalConfig[key];
  if(!env.OCTOCODE_JEV_KEY)throw new Error('Jev credential unavailable before launch');
  // Resolve the already-authorized GitHub CLI credential without writing/logging it.
  if(!env.GITHUB_TOKEN&&!env.GH_TOKEN){try{env.GITHUB_TOKEN=execFileSync('gh',['auth','token'],{encoding:'utf8',stdio:['ignore','pipe','pipe']}).trim();}catch{throw new Error('GitHub credential unavailable before launch');}}
  Object.assign(env,{OCTOCODE_HOME:path.join(runDir,'octocode-home'),OCTOCODE_NATIVE_BINDING:path.join(home,'snapshot/runtime.node'),OCTOCODE_REGEX_WORKER:path.join(home,'snapshot/octocode-regex-worker'),OCTOCODE_JEV_MODEL:'jev-1.13.0',ENABLE_LOCAL:'false',ENABLE_CLONE:'false',MAX_RETRIES:'0',OCTOCODE_ENABLE_STATS:'true',OCTOCODE_STORAGE_MODE:'persistent'});
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
  const question=id==='preflight'?'Infrastructure check: use ghGetFileContent to read the first three lines of README.md in octocat/Hello-World. Report what the tool actually returns, including any error. Do not use Jev unless it helps.':fs.readFileSync(path.join(home,'questions',`${id}.md`),'utf8');
  const prompt=`Answer this GitHub research question using the available Octocode MCP tools.\n\n${question}\n\nUse the actual tool descriptions and schemas; no skills, shell, browser, other connectors, or subagents. No benchmark files, historical answers, or grading material are available through the tools. Research efficiently and stop when sufficient evidence answers the question. Cite deciding source lines and resolved revisions where available. Preserve partial coverage and errors; do not infer global absence from a bounded search.\n\nJev is optional. Use it when its judgment changes the next action or avoids substantial reading; do not call it to satisfy a quota. Source evidence and checks establish facts. This is a tool-only evaluation with no Jev skill.\n\nBudget: ${budget.seconds}s, ${budget.ordinaryQueries} ordinary queries including nested Jev retrievals, ${budget.jevQueries} Jev questions. No minimum calls. Return a concise answer, citations, limitations, and a brief Jev-use assessment. Do not narrate these benchmark instructions.`;
  fs.writeFileSync(path.join(runDir,'prompt.txt'),prompt);
  const args=['exec','--ephemeral','--ignore-user-config','--ignore-rules','--skip-git-repo-check','--sandbox','read-only','--model',model,'-c','model_reasoning_effort="medium"','-c','web_search="disabled"','-c','project_doc_max_bytes=0','-c','tool_output_token_limit=12000','--enable','skip_host_skill_discovery'];
  for(const feature of ['shell_tool','apps','plugins','browser_use','browser_use_external','computer_use','multi_agent','hooks','image_generation','view_image','workspace_dependencies','skill_search','tool_suggest','sleep_tool','goals'])args.push('--disable',feature);
  args.push('-c',`mcp_servers.octocode.command=${JSON.stringify(process.execPath)}`,'-c',`mcp_servers.octocode.args=${JSON.stringify([path.join(home,'snapshot/proxy.mjs')])}`,'-c',`mcp_servers.octocode.env_vars=${JSON.stringify(['JEV_BENCH_CONFIG','OCTOCODE_HOME','OCTOCODE_NATIVE_BINDING','OCTOCODE_REGEX_WORKER','OCTOCODE_JEV_KEY','OCTOCODE_JEV_MODEL','OCTOCODE_JEV_BASE_URL','GITHUB_TOKEN','GH_TOKEN','ENABLE_LOCAL','ENABLE_CLONE','MAX_RETRIES','OCTOCODE_ENABLE_STATS','OCTOCODE_STORAGE_MODE'])}`,'-c','mcp_servers.octocode.startup_timeout_sec=30','-c','mcp_servers.octocode.tool_timeout_sec=120','--json','--output-schema',path.join(home,'answer-schema.json'),'--output-last-message',path.join(runDir,'answer.json'),'-');
  args.splice(1,0,'-c','mcp_servers.octocode.default_tools_approval_mode="approve"');
  const started=Date.now();let stdout='',stderr='',timedOut=false;
  const child=spawn('codex',args,{cwd,env,detached:true,stdio:['pipe','pipe','pipe']});
  const kill=()=>{try{process.kill(-child.pid,'SIGTERM');}catch{}setTimeout(()=>{try{process.kill(-child.pid,'SIGKILL');}catch{}},1500).unref();};
  const timer=setTimeout(()=>{timedOut=true;kill();},budget.seconds*1000);
  child.stdout.on('data',b=>{stdout+=b;fs.appendFileSync(path.join(runDir,'events.jsonl'),b);});
  child.stderr.on('data',b=>{stderr+=b;fs.appendFileSync(path.join(runDir,'stderr.log'),b);});
  child.stdin.end(prompt);
  const completion=await new Promise(resolve=>child.on('close',(exitCode,signal)=>resolve({exitCode,signal})));clearTimeout(timer);
  const events=stdout.split('\n').flatMap(line=>{try{return[JSON.parse(line)]}catch{return[]}});
  const usage=events.filter(e=>e.type==='turn.completed').map(e=>e.usage);
  const types=[...new Set(events.flatMap(e=>e.item?.type?[e.item.type]:[]))];
  const prohibited=events.filter(e=>['command_execution','web_search','collab_tool_call','file_change'].includes(e.item?.type)||
    (e.item?.type==='mcp_tool_call'&&e.item.server!=='octocode'&&
      !(e.item.server==='codex'&&['list_mcp_resources','list_mcp_resource_templates'].includes(e.item.tool))));
  const unexpectedResources=events.some(e=>e.type==='item.completed'&&e.item?.server==='codex'&&
    ['list_mcp_resources','list_mcp_resource_templates'].includes(e.item.tool)&&e.item.result?.content?.some(c=>{
      if(c.type!=='text')return true;try{const v=JSON.parse(c.text);return (v.resources??v.resourceTemplates??[]).length>0;}catch{return true;}
    }));
  const receipts=fs.existsSync(path.join(runDir,'calls.jsonl'))?fs.readFileSync(path.join(runDir,'calls.jsonl'),'utf8').trim().split('\n').map(JSON.parse):[];
  const catalogPresent=receipts.some(r=>r.event==='catalog'&&r.tools?.some(t=>t.name==='jev'));
  let answerValid=false;try{const a=read(path.join(runDir,'answer.json'));answerValid=typeof a.answer==='string'&&a.answer.trim().length>0&&typeof a.jevAssessment==='string'&&Array.isArray(a.limitations)&&a.limitations.every(x=>typeof x==='string')&&Array.isArray(a.citations)&&a.citations.every(x=>typeof x.url==='string'&&typeof x.claim==='string');}catch{}
  const hostTokenEligible=usage.length===1&&Number.isFinite(usage[0]?.input_tokens)&&Number.isFinite(usage[0]?.output_tokens)&&usage[0].input_tokens+usage[0].output_tokens<=budget.hostTokenEligibilityCap;
  let intact=true;try{verify()}catch{intact=false}
  const record={id,arm:'candidate',requestedModel:model,effort:'medium',...completion,timedOut,elapsedMs:Date.now()-started,usage,itemTypes:types,prohibitedToolEvents:prohibited.length,runtimeUnchanged:intact,catalogPresent,answerValid,hostTokenEligible,answerPresent:fs.existsSync(path.join(runDir,'answer.json')),modelAttestation:'Pinned CLI argument/config; no independent model-bearing provider receipt claimed.'};
  save(path.join(runDir,'record.json'),record);console.log(JSON.stringify(record));
  if(!intact||prohibited.length||unexpectedResources||usage.length!==1||!catalogPresent)throw new Error('Instrumentation/adherence gate failed; inspect preserved run');
  if(id==='preflight'&&(completion.exitCode!==0||timedOut||!answerValid||!hostTokenEligible||!receipts.some(r=>r.event==='call'&&r.name==='ghGetFileContent'&&!r.isError&&r.errorRows?.length===0)))throw new Error('Infrastructure preflight failed; inspect preserved run');
}

const [command,id]=process.argv.slice(2);
if(command==='freeze')await freeze();
else if(command==='run')await run(id);
else if(command==='verify')console.log(JSON.stringify({pass:true,...verify()}));
else throw new Error('Use freeze, verify, or run Q1..Q30/preflight. No default launches.');
