#!/usr/bin/env node
import {mkdirSync,readFileSync,writeFileSync,readdirSync,existsSync} from 'node:fs';
import {dirname,resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
const root=dirname(fileURLToPath(import.meta.url)), repo=resolve(root,'../../../..');
const [arm,id,kind,...args]=process.argv.slice(2);
const ids=['R37637','R37619','L40592','L40590','O151639','O151637'];
if(!['baseline','treatment','curator'].includes(arm)||!ids.includes(id)||!['start','finish','schema','octocode','jev','check'].includes(kind)) throw Error('Usage: run.mjs baseline|treatment|curator CASE start|finish|schema|octocode|jev|check args');
const dir=join(root,'runs',arm,id); mkdirSync(dir,{recursive:true});
const events=readdirSync(dir).filter(n=>/^event-\d+\.json$/.test(n)).map(n=>JSON.parse(readFileSync(join(dir,n))));
const count=k=>events.filter(e=>e.kind===k).length;
const sha=x=>createHash('sha256').update(x).digest('hex');
if(kind==='start'||kind==='finish'){
 const path=join(dir,`${kind}.json`); if(existsSync(path)) throw Error(`${kind} already recorded`);
 if(kind==='finish'&&!existsSync(join(dir,'answer.md')))throw Error('Write answer first');
 writeFileSync(path,JSON.stringify({arm,id,at:new Date().toISOString()},null,2)+'\n'); process.exit(0);
}
if(!existsSync(join(dir,'start.json')))throw Error('Record start first');
if(kind==='octocode'&&count(kind)>=(arm==='curator'?8:16))throw Error('Research cap reached');
if(kind==='jev'&&(arm!=='treatment'||count(kind)>=1))throw Error('One treatment Jev call only');
if(kind==='check'&&count(kind)>=2)throw Error('Two check invocations only');
let command,request=null;
const cli=join(repo,'packages/octocode/out/octocode.js');
if(kind==='jev'){
 request=JSON.parse(readFileSync(resolve(args[0]),'utf8'));if(request.model!=='jev-1.13.0')throw Error('Pin model');
 if(!existsSync(join(dir,'decision-before.json')))throw Error('Save prior decision before call');
 command=[process.execPath,join(repo,'skills/octocode-jev-logical-if/scripts/jev.mjs'),'evaluate','--input',resolve(args[0]),'--retries','0','--timeout-ms','10000'];
}else if(kind==='schema')command=[process.execPath,cli,'tools',...args,'--scheme','--json','--compact'];
else if(kind==='check'){
 if(!['node','python3'].includes(args[0])||!resolve(args[1]||'').startsWith(dir+'/'))throw Error('Checks require node/python3 and a script in your case directory');
 command=args;
}else{
 if(!['ghSearch','ghGetFileContent','ghGetHistoryItem','ghSearchHistory','artifactSearch','ghCloneRepo','localFetch','localSearch','astSearch','lspSearch'].includes(args[0]))throw Error('Unknown research tool');
 const qi=args.indexOf('--queries');if(qi>=0){try{const q=JSON.parse(args[qi+1]);const rows=Array.isArray(q)?q:(q.queries||[q]);if(rows.length>5)throw Error('Maximum five queries');}catch(e){if(e.message==='Maximum five queries')throw e;}}
 command=[process.execPath,cli,'tools',...args];
}
const startedAt=new Date().toISOString(),start=performance.now();
const result=spawnSync(command[0],command.slice(1),{cwd:repo,encoding:'utf8',timeout:90000,maxBuffer:8*1024*1024});
const stdout=result.stdout||'',stderr=result.stderr||'',seq=String(events.length+1).padStart(3,'0');
const event={arm,id,kind,args,startedAt,endedAt:new Date().toISOString(),elapsedMs:Math.round(performance.now()-start),exitCode:result.status,signal:result.signal,stdoutSha256:sha(stdout),requestSha256:request?sha(JSON.stringify(request)):null};
writeFileSync(join(dir,`event-${seq}.json`),JSON.stringify(event,null,2)+'\n');
writeFileSync(join(dir,`event-${seq}.stdout.txt`),stdout);writeFileSync(join(dir,`event-${seq}.stderr.txt`),stderr);
if(request)writeFileSync(join(dir,`event-${seq}.request.json`),JSON.stringify(request,null,2)+'\n');
process.stdout.write(stdout);process.stderr.write(stderr);process.exitCode=result.status??3;
