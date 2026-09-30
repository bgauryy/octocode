import {startServer,rowData,nextHints,checks,writeResults,inventoryRows} from './mcp-client.mjs';
const c=await startServer();const {check,summary}=checks('review-large');const calls=[];
const q={operation:'pullRequest',owner:'microsoft',repo:'TypeScript',number:51387};
async function run(label,args,raw=false){const e=raw?await c.raw('ghGetHistoryItem',args):await c.call('ghGetHistoryItem',args);calls.push({...e,label,bytes:Buffer.byteLength(e.text)});check(label+' succeeds',!e.isError&&!e.rowErrors);return e}
try {
 const m=rowData(await run('metadata',q)).pullRequests[0];
 check('fixture exceeds 100 changed files',m.changedFilesCount>100,m.changedFilesCount);
 check('metadata excludes changed-file bodies',!m.changedFiles);
 check('metadata offers file inventory',!!m.next?.getChangedFiles);
 let e=await run('inventory page 1',{...q,content:{changedFiles:true},pageSize:100});let files=[];let pages=0;
 for(;e&&pages<40;pages++){
  const pr=rowData(e)?.pullRequests?.[0];
  check('page '+(pages+1)+' has file metadata',Array.isArray(pr?.changedFiles)&&pr.changedFiles.length>0);
  check('page '+(pages+1)+' excludes patches',inventoryRows(pr?.changedFiles).every(f=>f.path&&!Object.hasOwn(f,'patch')));
  check('page '+(pages+1)+' preserves head SHA',pr?.sourceSha===m.sourceSha);
  files.push(...inventoryRows(pr?.changedFiles));
  const next=nextHints(e.sc).find(h=>h.path.endsWith('nextChangedFilesPage'));
  e=next?await run('inventory page '+(pages+2),next.query,true):null;
 }
 check('inventory terminates',!e);
 check('all changed files returned exactly once',files.length===m.changedFilesCount&&new Set(files.map(f=>f.path)).size===m.changedFilesCount,files.length);
 const chosen=files[110];
 if(chosen){const selected=rowData(await run('selected patch beyond file 100',{...q,content:{patches:{mode:'selected',files:[chosen.path]}},minify:'none'})).pullRequests[0];
 check('selection finds requested late file only',selected.changedFiles?.length===1&&selected.changedFiles[0].path===chosen.path,chosen.path);
 check('selected patch or explicit unavailability',typeof selected.changedFiles?.[0]?.patch==='string'||!!selected.changedFiles?.[0]?.patchUnavailable);
 }
 const apiFiles=[];
 for(let page=1;page<=Math.ceil(m.changedFilesCount/100);page++){
 const res=await fetch(`https://api.github.com/repos/microsoft/TypeScript/pulls/51387/files?per_page=100&page=${page}`,{headers:{'User-Agent':'octocode-review-audit'}});
 if(!res.ok)throw Error('independent GitHub files oracle HTTP '+res.status);
 apiFiles.push(...await res.json());
 }
 check('inventory paths and stats equal independent API',apiFiles.length===files.length&&apiFiles.every((f,i)=>files[i].path===f.filename&&files[i].additions===f.additions&&files[i].deletions===f.deletions));
} catch(error){check('audit completes',false,error.stack)} finally {const result=summary();writeResults('review-large',{at:new Date().toISOString(),...result,calls});console.table(calls.map(({label,ms,bytes})=>({label,ms,bytes})));c.close();process.exitCode=result.failed.length?1:0;}
