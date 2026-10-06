import fs from 'node:fs';
import {startServer,rowData,nextHints,checks,writeResults,RESULTS,rawPatch,patchNumbersOk} from './mcp-client.mjs';
const prior=JSON.parse(fs.readFileSync(RESULTS+'/review-large.json'));let e=prior.calls.at(-1);
const c=await startServer();const {check,summary}=checks('review-large-patch');const calls=[];let patch='',pages=0;
try {
 while(e&&pages<20){pages++;const f=rowData(e)?.pullRequests?.[0]?.files?.[0];check('patch window '+pages+' starts at accumulated offset',f?.patchPagination?.offset===patch.length);patch+=f?.patch??'';
 const hints=nextHints(e.sc);const next=hints.find(h=>/patch/i.test(h.path)&&h.query?.offset>0)||hints.find(h=>/patch/i.test(h.path)&&h.query?.queries?.[0]?.offset>0);
 if(f?.patchPagination?.hasMore)check('unfinished window has executable continuation',!!next);
 e=next?await c.follow(next):null;if(e){calls.push(e);check('patch continuation succeeds',!e.isError&&!e.rowErrors)}
 }
 check('patch pagination terminates',!e);
 const res=await fetch('https://api.github.com/repos/microsoft/TypeScript/pulls/51387/files?per_page=100&page=2',{headers:{'User-Agent':'octocode-review-audit'}});if(!res.ok)throw Error('oracle HTTP '+res.status);
 const f=(await res.json()).find(f=>f.filename==='src/compiler/transformers/utilities.ts');
 check('assembled raw patch exactly equals independent API',rawPatch(patch)===f?.patch,'chars '+patch.length);check('assembled patch numbers every new-side line',patchNumbersOk(patch));
} catch(err){check('audit completes',false,err.message)}finally {const result=summary();writeResults('review-large-patch',{...result,pages,calls});c.close();process.exitCode=result.failed.length?1:0}
