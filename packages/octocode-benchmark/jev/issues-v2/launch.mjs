import {spawn} from 'node:child_process';
import {createWriteStream,mkdirSync,writeFileSync,existsSync} from 'node:fs';
import {dirname,resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=dirname(fileURLToPath(import.meta.url)),repo=resolve(root,'../../../..');
const [arm,id]=process.argv.slice(2),issues={R37637:'react/react/37637',R37619:'react/react/37619',L40592:'langchain-ai/langchain/40592',L40590:'langchain-ai/langchain/40590',O151639:'openclaw/openclaw/151639',O151637:'openclaw/openclaw/151637'};
if(!['baseline','treatment'].includes(arm)||!issues[id])throw Error('arm case required');
const dir=join(root,'runs',arm,id);mkdirSync(dir,{recursive:true});if(existsSync(join(dir,'host.json')))throw Error('No replacement runs');
const [owner,name,num]=issues[id].split('/');
const wrapper='node '+root+'/run.mjs '+arm+' '+id;
const prompt=[
 'Fresh Terra benchmark worker. Triage https://github.com/'+owner+'/'+name+'/issues/'+num+' and propose concrete safe fix IF bug supported; otherwise correct disposition. Case '+id+' arm '+arm+'. Repository '+repo+'.',
 'Read AGENTS.md and '+root+'/CONTRACT.md plus EXECUTION-AMENDMENT.md completely. Read /Users/bgaryy/.agents/skills/octocode-research/SKILL.md and relevant external/debug references.',
 arm==='treatment'?'Also read skills/octocode-jev-logical-if/SKILL.md and routed research/protocol/context/configuration and request template. Exactly one actual Jev call after initial evidence and saved decision-before.json. Include concise PUBLIC reasoning summary, exact deciding excerpts, realistic hypotheses/alternatives/none, discriminating checks; NOT private chain-of-thought. Save request.json. Independently verify selected check; corroboration is not changed direction.':'Jev disabled. Do not read Jev skill or call it.',
 'Never read sibling workers, curator/gold, prior benchmarks or parent findings. No agents, upstream writes, installs/build scripts or shell/API/web research bypass. Only write inside '+dir+'. Setup skill/contract reads allowed directly.',
 'All public/code research through '+wrapper+' schema TOOL... or octocode TOOL --queries JSON --compact. Record start now using wrapper start; finish after artifacts using wrapper finish. 16 research invocations max,5rows/batch,target8min; at10calls focus decisive proof. Inspect live schema; reasoning now required.',
 'Optional2 self-authored reviewed isolated checks via wrapper check node|python3 /absolute/owned/script; distinguish model/extraction from actual package runtime.',
 arm==='treatment'?'Jev command: '+wrapper+' jev '+dir+'/request.json; pin jev-1.13.0. Skill dry-run allowed.':'',
 'Write answer.md/result.json/decision-before.json/decision-after.json per contract. Include supported actual/expected contract, classification,trigger,mechanism with pinned URLs,alternate checked,concrete edit/no-fix disposition,regression/negative controls,actual tests vs pending and uncertainty. No invented timestamps/runtime claims.',
 'Do not edit shared GOTCHAS: record friction in own answer. Use apply_patch for edits. Final return status,result,<=8anchors,verification,confidence,next.'
].join('\n');
writeFileSync(join(dir,'prompt.txt'),prompt);
const args=['exec','--ephemeral','--ignore-user-config','--json','-m','gpt-5.6-terra','-c','model_reasoning_effort="high"','-c','approval_policy="never"','--sandbox','danger-full-access','-C',repo,'-o',join(dir,'host-final.md'),prompt];
const startedAt=new Date().toISOString();writeFileSync(join(dir,'host.json'),JSON.stringify({arm,id,model:'gpt-5.6-terra',effort:'high',startedAt,status:'running'},null,2)+'\n');
const out=createWriteStream(join(dir,'host.jsonl')),err=createWriteStream(join(dir,'host.stderr.txt'));
const child=spawn('codex',args,{cwd:repo,stdio:['ignore','pipe','pipe']});child.stdout.pipe(out);child.stderr.pipe(err);
const timer=setTimeout(()=>child.kill('SIGTERM'),15*60*1000);
child.on('error',e=>console.error(e.message));
child.on('close',(code,signal)=>{clearTimeout(timer);writeFileSync(join(dir,'host.json'),JSON.stringify({arm,id,model:'gpt-5.6-terra',effort:'high',startedAt,endedAt:new Date().toISOString(),exitCode:code,signal,status:code===0?'complete':'failed'},null,2)+'\n');console.log(JSON.stringify({arm,id,exitCode:code,signal}));process.exitCode=code??1;});
