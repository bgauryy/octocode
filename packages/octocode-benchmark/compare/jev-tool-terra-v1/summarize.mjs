import fs from 'node:fs';
import path from 'node:path';

const home=path.resolve(process.argv[2]??'');
if(!process.argv[2]||!fs.existsSync(path.join(home,'freeze.json')))throw new Error('Pass a frozen campaign directory');
const read=file=>JSON.parse(fs.readFileSync(file,'utf8'));
const counter=value=>Number.isSafeInteger(value)&&value>=0;
const rows=[];
const tools={};
for(let n=1;n<=30;n++){
  const id=`Q${n}`,dir=path.join(home,'runs',id);
  const recordPresent=fs.existsSync(path.join(dir,'record.json'));
  const r=recordPresent?read(path.join(dir,'record.json')):{};
  const receipts=fs.existsSync(path.join(dir,'calls.jsonl'))?fs.readFileSync(path.join(dir,'calls.jsonl'),'utf8').split('\n').filter(line=>line.trim()).map(JSON.parse):[];
  const calls=receipts.filter(x=>x.event==='call');
  const started=receipts.filter(x=>x.event==='callStarted');
  const interrupted=started.filter(s=>!calls.some(c=>c.id===s.id)).map(c=>({...c,admitted:true,interrupted:true}));
  const attempts=[...calls,...interrupted];
  const attempted=recordPresent||receipts.length>0||fs.existsSync(path.join(dir,'RESERVED'));
  const jev=attempts.filter(c=>c.name==='jev'&&c.admitted);
  const questions=jev.flatMap(c=>c.input.queries??[c.input]);
  const types={},contexts={};
  for(const q of questions){types[q.question?.type]=(types[q.question?.type]??0)+1;const k=q.context?.tool??'value';contexts[k]=(contexts[k]??0)+1;}
  for(const c of attempts){const t=tools[c.name]??={calls:0,admittedQueries:0,errorRows:0,callErrors:0,interruptedCalls:0,latencyMs:0,unknownLatencyCalls:0};t.calls++;t.admittedQueries+=c.admitted?(c.cost?.jev||c.cost?.ordinary||0):0;t.errorRows+=c.errorRows?.length??0;t.callErrors+=Number(c.isError===true);t.interruptedCalls+=Number(c.interrupted===true);if(Number.isFinite(c.durationMs)&&c.durationMs>=0)t.latencyMs+=c.durationMs;else t.unknownLatencyCalls++;}
  const knownProvider=jev.reduce((v,c)=>({input:v.input+(c.providerUsage?.knownInputTokens??0),output:v.output+(c.providerUsage?.knownOutputTokens??0)}),{input:0,output:0});
  const unknownJevRows=jev.reduce((v,c)=>v+(c.providerUsage?.unknownRows?.length??c.cost.jev),0);
  const u=r.usage?.length===1?r.usage[0]:null;
  const validUsage=u&&counter(u.input_tokens)&&counter(u.output_tokens)&&(u.cached_input_tokens===undefined||(counter(u.cached_input_tokens)&&u.cached_input_tokens<=u.input_tokens));
  const host=validUsage?{input:u.input_tokens,cachedInput:u.cached_input_tokens??null,output:u.output_tokens,total:u.input_tokens+u.output_tokens}:null;
  const prohibited=Array.isArray(r.prohibitedToolEvents)?r.prohibitedToolEvents.length>0:!!r.prohibitedToolEvents;
  const invalid=recordPresent&&(!host||r.runtimeUnchanged===false||r.catalogPresent===false||prohibited);
  const status=!recordPresent?(attempted?'incomplete_attempt':'not_completed'):invalid?'invalid':r.exitCode===0&&!r.timedOut&&r.answerValid?'answered':'failed';
  rows.push({id,status,recordPresent,attempted,hostTokenEligible:!!host&&r.hostTokenEligible===true,elapsedMs:r.elapsedMs??null,host,toolCalls:attempts.length,ordinaryQueries:attempts.filter(c=>c.admitted).reduce((v,c)=>v+c.cost.ordinary,0),jevCalls:jev.length,jevQuestions:questions.length,jevTypes:types,jevContexts:contexts,providerKnown:knownProvider,providerInput:unknownJevRows?null:knownProvider.input,providerOutput:unknownJevRows?null:knownProvider.output,unknownJevRows,interruptedCalls:interrupted.length});
}
const totals={answered:rows.filter(r=>r.status==='answered').length,completed:rows.filter(r=>r.recordPresent).length,attempted:rows.filter(r=>r.attempted).length,invalid:rows.filter(r=>r.status==='invalid').length,failed:rows.filter(r=>r.status==='failed').length,incompleteAttempts:rows.filter(r=>r.status==='incomplete_attempt').length,notStarted:rows.filter(r=>!r.attempted).length,tokenEligible:rows.filter(r=>r.hostTokenEligible).length,jevUsingCases:rows.filter(r=>r.jevQuestions>0).length,hostKnownInput:0,hostKnownCachedInput:0,hostKnownOutput:0,unknownHostUsageCases:rows.filter(r=>r.attempted&&!r.host).length,unknownHostCachedInputCases:rows.filter(r=>r.attempted&&(!r.host||r.host.cachedInput===null)).length,ordinaryQueries:0,jevCalls:0,jevQuestions:0,providerKnownInput:0,providerKnownOutput:0,unknownJevRows:0,interruptedCalls:0};
for(const r of rows){if(r.host){totals.hostKnownInput+=r.host.input;totals.hostKnownCachedInput+=r.host.cachedInput??0;totals.hostKnownOutput+=r.host.output;}totals.ordinaryQueries+=r.ordinaryQueries;totals.jevCalls+=r.jevCalls;totals.jevQuestions+=r.jevQuestions;totals.providerKnownInput+=r.providerKnown.input;totals.providerKnownOutput+=r.providerKnown.output;totals.unknownJevRows+=r.unknownJevRows;totals.interruptedCalls+=r.interruptedCalls;}
totals.hostInput=totals.unknownHostUsageCases?null:totals.hostKnownInput;
totals.hostOutput=totals.unknownHostUsageCases?null:totals.hostKnownOutput;
totals.hostTotal=totals.unknownHostUsageCases?null:totals.hostKnownInput+totals.hostKnownOutput;
totals.hostCachedInput=totals.unknownHostCachedInputCases?null:totals.hostKnownCachedInput;
totals.providerInput=totals.unknownJevRows?null:totals.providerKnownInput;
totals.providerOutput=totals.unknownJevRows?null:totals.providerKnownOutput;
const summary={generated:new Date().toISOString(),campaign:home,totals,tools,rows};
fs.writeFileSync(path.join(home,'measurements.json'),JSON.stringify(summary,null,2)+'\n');
console.log(JSON.stringify({totals,tools}));
