import {readFileSync,readdirSync,existsSync,writeFileSync} from 'node:fs';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
const root=dirname(fileURLToPath(import.meta.url));
const ids=['R37637','R37619','L40592','L40590','O151639','O151637'];
const read=p=>JSON.parse(readFileSync(p,'utf8'));
const sha=x=>createHash('sha256').update(x).digest('hex');
const rows=[];
for(const arm of ['baseline','treatment'])for(const id of ids){
 const dir=join(root,'runs',arm,id);if(!existsSync(dir))continue;
 const events=readdirSync(dir).filter(x=>/^event-\d+\.json$/.test(x)).map(x=>({file:x,...read(join(dir,x))}));
 const host=existsSync(join(dir,'host.json'))?read(join(dir,'host.json')):{status:'not-launched'};
 const lines=existsSync(join(dir,'host.jsonl'))?readFileSync(join(dir,'host.jsonl'),'utf8').split('\n').filter(Boolean).flatMap(x=>{try{return[JSON.parse(x)];}catch{return[];}}):[];
 const completion=lines.filter(x=>x.type==='turn.completed').at(-1);
 let jevInput=0,jevOutput=0,jevSucceeded=0,queryRows=0;
 const receiptErrors=[];
 for(const event of events){
  const stdout=readFileSync(join(dir,event.file.replace('.json','.stdout.txt')),'utf8');
  if(sha(stdout)!==event.stdoutSha256)receiptErrors.push(event.file+': stdout hash');
  if(event.kind==='octocode'){const i=event.args.indexOf('--queries');if(i>=0)try{const q=JSON.parse(event.args[i+1]);queryRows+=(Array.isArray(q)?q:q.queries||[q]).length;}catch{}}
  if(event.kind==='jev'){
   const request=read(join(dir,event.file.replace('.json','.request.json')));
   if(sha(JSON.stringify(request))!==event.requestSha256)receiptErrors.push(event.file+': request hash');
   try{const response=JSON.parse(stdout);if(event.exitCode===0&&response.model==='jev-1.13.0'){jevSucceeded++;jevInput+=response.usage?.input_tokens||0;jevOutput+=response.usage?.output_tokens||0;}}catch{}
  }
 }
 rows.push({arm,id,status:host.status,answers:existsSync(join(dir,'answer.md')),artifactsComplete:['answer.md','result.json','decision-before.json','decision-after.json','start.json','finish.json'].every(x=>existsSync(join(dir,x))),researchCalls:events.filter(x=>x.kind==='octocode').length,queryRows,schemaCalls:events.filter(x=>x.kind==='schema').length,checkCalls:events.filter(x=>x.kind==='check').length,jevCalls:events.filter(x=>x.kind==='jev').length,jevSucceeded,jevInput,jevOutput,toolErrors:events.filter(x=>x.exitCode!==0).length,hostElapsedMs:host.endedAt?Date.parse(host.endedAt)-Date.parse(host.startedAt):null,hostUsage:completion?.usage??null,receiptErrors,lastEvent:events.at(-1)?.kind??null,hostErrors:lines.filter(x=>x.type==='error'||x.type==='turn.failed').map(x=>x.message||x.error)});
}
const frozen=read(join(root,'frozen.json'));
const integrity=frozen.files.map(f=>({path:f.path,valid:sha(readFileSync(resolve(root,f.path)))===f.sha256}));
const report={at:new Date().toISOString(),integrity,rows};
if(process.argv.includes('--save'))writeFileSync(join(root,'metrics.json'),JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(report,null,2));
