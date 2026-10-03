import { startServer } from '../../../../../harness/mcp-client.mjs';
import fs from 'node:fs';
const D='/Users/bgaryy/code/octocode/octocode-local-testing/bench/validate/features/probes/local';
const FX=D+'/fx';
const s = await startServer();
const out={};
const save=(k,e)=>{out[k]={isError:e.isError,bytes:e.bytes,text:e.text.slice(0,3000),sc:e.sc}; console.log(`[${k}] isError=${e.isError} bytes=${e.bytes}`, e.text.slice(0,400));};
console.log('tools:', s.tools.map(t=>t.name).join(','));
let e=await s.call('localSearch',{path:FX+'/many',searchText:'needle',pageSize:10}); save('mcp_ls_page1',e);
const nq=e.sc.results[0].data.next.nextPage.query;
e=await s.raw('localSearch',{queries:[nq]}); save('mcp_ls_page2_follow',e);
e=await s.call('localSearch',{path:FX+'/src',searchText:'needle'}); save('mcp_ls_min',e);
e=await s.call('localSearch',{path:FX+'/src',searchText:'needle',debug:true}); save('mcp_ls_debug',e);
// 6 queries
e=await s.call('localSearch',Array.from({length:6},()=>({path:FX+'/src',searchText:'needle'}))); save('mcp_ls_6q',e);
// no goal
e=await s.raw('localSearch',{queries:[{path:FX+'/src',searchText:'needle'}]}); save('mcp_ls_nogoal',e);
// raw structuredContent for shared
const r=await s.rpc('tools/call',{name:'structureSearch',arguments:{queries:[{goal:'g',reasoning:'r',operation:'files',path:FX+'/many',names:['*.txt'],detail:'full'}]}});
out.mcp_ss_raw=r.result; console.log('[mcp_ss_raw]', JSON.stringify(r.result.structuredContent).slice(0,700));
fs.writeFileSync(D+'/mcp1.json',JSON.stringify(out,null,1));
s.close();
