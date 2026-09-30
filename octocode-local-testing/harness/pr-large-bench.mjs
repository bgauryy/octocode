// Large-PR review benchmark: octocode (MCP) vs gh CLI, plus correctness checks.
// node pr-large-bench.mjs [label]   -> results/pr-large-<label>.json
import {execFileSync} from 'node:child_process';
import {startServer,rowData,nextHints,checks,writeResults,inventoryRows} from './mcp-client.mjs';

const LABEL=process.argv[2]??'run';
const PRS=[
  {id:'ts-51387',owner:'microsoft',repo:'TypeScript',number:51387,note:'656 files, merged, GitHub diff-limit files'},
  {id:'rust-157558',owner:'rust-lang',repo:'rust',number:157558,note:'429 files, renames/removed'},
  {id:'ts-61986',owner:'microsoft',repo:'TypeScript',number:61986,note:'huge single-file patches (70k chars, provider-omitted)'},
  {id:'next-97634',owner:'vercel',repo:'next.js',number:97634,note:'binary files'},
];
const {check,summary}=checks('pr-large');const c=await startServer();const report={};

const sh=(args)=>{const t=Date.now();let out='',err=null;try{out=execFileSync('gh',args,{encoding:'utf8',maxBuffer:1<<30,stdio:['ignore','pipe','pipe']})}catch(e){out=(e.stdout??'')+(e.stderr??'');err=String(e.stderr??e.message).slice(0,160)}return{ms:Date.now()-t,chars:out.length,out,err}};
const apiFiles=(p)=>JSON.parse(execFileSync('gh',['api','--paginate','--slurp',`repos/${p.owner}/${p.repo}/pulls/${p.number}/files?per_page=100`],{encoding:'utf8',maxBuffer:1<<30})).flat();

async function octo(p,api){
  const calls=[];const base={operation:'pullRequest',owner:p.owner,repo:p.repo,number:p.number};
  const run=async(label,q,raw=false,tool='ghGetHistoryItem')=>{const e=raw?await c.raw(tool,q):await c.call(tool,q);calls.push({label,ms:e.ms,chars:e.text.length});if(e.isError||e.rowErrors)check(`${p.id} ${label} succeeds`,false,e.text.slice(0,200));return e};
  const meta=rowData(await run('metadata',base)).pullRequests[0];
  let e=await run('inventory p1',{...base,content:{changedFiles:true}});const files=[];const shas=new Set();let pages=0;let lastPage;
  while(e&&pages<40){pages++;const pr=rowData(e).pullRequests[0];shas.add(pr.sourceSha??meta.sourceSha);files.push(...inventoryRows(pr.changedFiles));lastPage=pr.contentPagination?.changedFiles;
    const next=nextHints(e.sc).find(h=>h.path.endsWith('nextChangedFilesPage'));e=next?await run('inventory p'+(pages+1),next.query,true):null;}
  // correctness: inventory
  check(`${p.id} inventory count == gh api`,files.length===api.length,`${files.length} vs ${api.length}`);
  check(`${p.id} inventory no dupes`,new Set(files.map(f=>f.path)).size===files.length);
  check(`${p.id} inventory order/paths/status/stats == gh api`,api.every((f,i)=>files[i]?.path===f.filename&&files[i]?.status===f.status&&(files[i]?.additions??0)===f.additions&&(files[i]?.deletions??0)===f.deletions));
  check(`${p.id} sourceSha stable across pages`,shas.size===1&&shas.has(meta.sourceSha));
  const renames=api.filter(f=>f.status==='renamed');
  if(renames.length)check(`${p.id} renames carry previous path`,renames.every(r=>files.find(f=>f.path===r.filename)?.previousPath===r.previous_filename),renames.length);
  const patchless=api.filter(f=>f.patch==null);
  if(patchless.length)check(`${p.id} inventory flags patchless files (${patchless.length})`,patchless.every(a=>{const f=files.find(f=>f.path===a.filename);return (a.status==='renamed'&&a.changes===0)?!f?.patchUnavailable:!!f?.patchUnavailable}),patchless.length);
  if(meta.state==='merged')check(`${p.id} merged PR has mergeCommitSha`,/^[0-9a-f]{40}$/.test(meta.mergeCommitSha??''));
  // selected patches: first patched, largest patched, a late file
  const withPatch=api.map((f,i)=>({...f,i})).filter(f=>f.patch!=null);
  const pick=withPatch.length===0?[api[0].filename]:[...new Set([withPatch[0],[...withPatch].sort((a,b)=>b.patch.length-a.patch.length)[0],withPatch.at(-1)].filter(Boolean).map(f=>f.filename))];
  const extra=patchless.filter(f=>!(f.status==='renamed'&&f.changes===0)).slice(0,1).map(f=>f.filename);
  let pe=await run('selected patches',{...base,content:{patches:{mode:'selected',files:[...pick,...extra]}},minify:'none'});
  const got={};let windows=0;const unavailable=new Set();
  while(pe&&windows<60){windows++;for(const f of rowData(pe).pullRequests?.[0]?.changedFiles??[]){if(typeof f.patch==='string')got[f.path]=(got[f.path]??'')+f.patch;if(f.patchUnavailable||f.noPatch)unavailable.add(f.path)}
    const nx=nextHints(pe.sc).find(h=>/continuePatch/.test(h.path));pe=nx?await run('patch window '+(windows+1),nx.query,true):null;}
  if(withPatch.length)check(`${p.id} selected patches verbatim (${pick.length})`,pick.every(n=>got[n]===api.find(f=>f.filename===n).patch),pick.map(n=>`${n}:${got[n]?.length}/${api.find(f=>f.filename===n).patch?.length}`).join(' '));
  if(extra.length)check(`${p.id} patchless selected file signaled`,unavailable.has(extra[0]),extra[0]);
  // exact source read around first hunk of first pick
  const f0=api.find(f=>f.filename===pick[0]);const hunk=/@@ -\d+(?:,\d+)? \+(\d+)/.exec(f0.patch??'');const start=Math.max(1,+(hunk?.[1]??1));
  if(f0.status!=='removed'&&f0.patch){const src=await run('source @sourceSha',{owner:p.owner,repo:p.repo,path:f0.filename,branch:meta.sourceSha,startLine:start,endLine:start+60,minify:'none'},false,'ghGetFileContent');check(`${p.id} source read ok`,!src.isError)}
  return {calls:calls.length,chars:calls.reduce((a,b)=>a+b.chars,0),ms:calls.reduce((a,b)=>a+b.ms,0),pages,windows,detail:calls,pick,lastPage};
}

function ghTypical(p,pick,sha){
  const r=`${p.owner}/${p.repo}`;const steps=[
    sh(['pr','view',String(p.number),'-R',r]),
    sh(['pr','diff',String(p.number),'-R',r]),
    sh(['api','--paginate',`repos/${r}/pulls/${p.number}/files?per_page=100`]),
    sh(['api',`repos/${r}/contents/${pick[0]}?ref=${sha}`,'-H','Accept: application/vnd.github.raw']),
  ];
  return {calls:steps.length,chars:steps.reduce((a,b)=>a+b.chars,0),ms:steps.reduce((a,b)=>a+b.ms,0),diffError:steps[1].err,steps:steps.map(s=>({chars:s.chars,ms:s.ms,err:s.err}))};
}
function ghLean(p,pick,sha,start){
  const r=`${p.owner}/${p.repo}`;const sel=JSON.stringify(pick);const steps=[
    sh(['pr','view',String(p.number),'-R',r,'--json','title,state,headRefOid,changedFiles,additions,deletions,mergeCommit']),
    sh(['api','--paginate',`repos/${r}/pulls/${p.number}/files?per_page=100`,'--jq','.[]|[.status,.additions,.deletions,.filename,(.previous_filename//"")]|@tsv']),
    sh(['api','--paginate',`repos/${r}/pulls/${p.number}/files?per_page=100`,'--jq',`.[]|select(.filename as $f|${sel}|index($f))|"### "+.filename+"\\n"+(.patch//"(no patch)")`]),
  ];
  const raw=sh(['api',`repos/${r}/contents/${pick[0]}?ref=${sha}`,'-H','Accept: application/vnd.github.raw']);
  const lines=raw.out.split('\n').slice(start-1,start+60).join('\n');steps.push({...raw,chars:lines.length});
  return {calls:steps.length,chars:steps.reduce((a,b)=>a+b.chars,0),ms:steps.reduce((a,b)=>a+b.ms,0)};
}

try{
  for(const p of PRS){
    const api=apiFiles(p);const o=await octo(p,api);
    const sha=execFileSync('gh',['pr','view',String(p.number),'-R',`${p.owner}/${p.repo}`,'--json','headRefOid','--jq','.headRefOid'],{encoding:'utf8'}).trim();
    const f0=api.find(f=>f.filename===o.pick[0]);const start=+(/@@ -\d+(?:,\d+)? \+(\d+)/.exec(f0.patch??'')?.[1]??1);
    const t=ghTypical(p,o.pick,sha);const l=ghLean(p,o.pick,sha,start);
    report[p.id]={note:p.note,files:api.length,octocode:{calls:o.calls,chars:o.chars,ms:o.ms,inventoryPages:o.pages,patchWindows:o.windows},ghTypical:{calls:t.calls,chars:t.chars,ms:t.ms,diffError:t.diffError},ghLean:l,octoCalls:o.detail};
    console.log(p.id,JSON.stringify({octo:report[p.id].octocode,typical:{calls:t.calls,chars:t.chars,ms:t.ms,diffError:t.diffError},lean:l}));
  }
  // edge: >3000 files
  const big={operation:'pullRequest',owner:'rust-lang',repo:'rust',number:106458,content:{changedFiles:true},pageSize:100,filePage:30};
  const be=await c.call('ghGetHistoryItem',big);const bp=rowData(be).pullRequests[0];const pg=bp.contentPagination?.changedFiles;
  check('edge >3000 files: inventory not reported complete',pg?.countScope!=='complete'&&!!pg?.terminalLimit,JSON.stringify(pg));
}catch(err){check('bench completes',false,err.stack)}
finally{const result=summary();writeResults('pr-large-'+LABEL,{at:new Date().toISOString(),report,...result});c.close();process.exitCode=0}
