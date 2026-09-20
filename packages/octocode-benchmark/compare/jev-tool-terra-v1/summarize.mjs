import fs from 'node:fs';
import path from 'node:path';

const home=path.resolve(process.argv[2]??'');
if(!process.argv[2]||!fs.existsSync(path.join(home,'freeze.json')))throw new Error('Pass a frozen campaign directory');
const read=file=>JSON.parse(fs.readFileSync(file,'utf8'));
const rows=[];
const tools={};
for(let n=1;n<=30;n++){
  const id=`Q${n}`,dir=path.join(home,'runs',id);
  if(!fs.existsSync(path.join(dir,'record.json'))){rows.push({id,status:'not_completed'});continue;}
  const r=read(path.join(dir,'record.json'));
  const receipts=fs.existsSync(path.join(dir,'calls.jsonl'))?fs.readFileSync(path.join(dir,'calls.jsonl'),'utf8').trim().split('\n').map(JSON.parse):[];
  const calls=receipts.filter(x=>x.event==='call');
  const started=receipts.filter(x=>x.event==='callStarted');
  const interrupted=started.filter(s=>!calls.some(c=>c.id===s.id));
  const jev=calls.filter(c=>c.name==='jev'&&c.admitted);
  const questions=jev.flatMap(c=>c.input.queries??[c.input]);
  const types={},contexts={};
  for(const q of questions){types[q.question?.type]=(types[q.question?.type]??0)+1;const k=q.context?.tool??'value';contexts[k]=(contexts[k]??0)+1;}
  for(const c of calls){const t=tools[c.name]??={calls:0,admittedQueries:0,errorRows:0,callErrors:0,latencyMs:0};t.calls++;t.admittedQueries+=c.admitted?(c.cost?.jev||c.cost?.ordinary||0):0;t.errorRows+=c.errorRows?.length??0;t.callErrors+=Number(c.isError);t.latencyMs+=c.durationMs;}
  const knownProvider=jev.reduce((v,c)=>({input:v.input+(c.providerUsage?.knownInputTokens??0),output:v.output+(c.providerUsage?.knownOutputTokens??0)}),{input:0,output:0});
  const unknownJevRows=jev.reduce((v,c)=>v+(c.providerUsage?.unknownRows?.length??c.cost.jev),0)+interrupted.filter(c=>c.name==='jev').reduce((v,c)=>v+c.cost.jev,0);
  const u=r.usage?.length===1?r.usage[0]:null;
  rows.push({id,status:r.exitCode===0&&!r.timedOut&&r.answerValid?'answered':'failed',hostTokenEligible:r.hostTokenEligible,elapsedMs:r.elapsedMs,host:u?{input:u.input_tokens,cachedInput:u.cached_input_tokens,output:u.output_tokens,total:u.input_tokens+u.output_tokens}:null,toolCalls:calls.length,ordinaryQueries:calls.filter(c=>c.admitted).reduce((v,c)=>v+c.cost.ordinary,0),jevCalls:jev.length,jevQuestions:questions.length,jevTypes:types,jevContexts:contexts,providerKnown:knownProvider,unknownJevRows,interruptedCalls:interrupted.length});
}
const finished=rows.filter(r=>r.host);
const totals={answered:rows.filter(r=>r.status==='answered').length,completed:finished.length,tokenEligible:rows.filter(r=>r.hostTokenEligible).length,jevUsingCases:rows.filter(r=>r.jevQuestions>0).length,hostInput:0,hostCachedInput:0,hostOutput:0,jevQuestions:0,providerKnownInput:0,providerKnownOutput:0,unknownJevRows:0};
for(const r of finished){totals.hostInput+=r.host.input;totals.hostCachedInput+=r.host.cachedInput??0;totals.hostOutput+=r.host.output;totals.jevQuestions+=r.jevQuestions;totals.providerKnownInput+=r.providerKnown.input;totals.providerKnownOutput+=r.providerKnown.output;totals.unknownJevRows+=r.unknownJevRows;}
const summary={generated:new Date().toISOString(),campaign:home,totals,tools,rows};
fs.writeFileSync(path.join(home,'measurements.json'),JSON.stringify(summary,null,2)+'\n');
console.log(JSON.stringify({totals,tools}));
