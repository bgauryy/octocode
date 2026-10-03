import { startServer } from '../../../../../harness/mcp-client.mjs';
import fs from 'node:fs';
const D='/Users/bgaryy/code/octocode/octocode-local-testing/bench/validate/features/probes/local';
const FX=D+'/fx';
const out={}; const X=process.env.DRIFT?{OCTOCODE_ALLOW_CONTRACT_DRIFT:'1'}:{};
const save=(k,e)=>{out[k]={isError:e.isError,bytes:e.bytes,text:e.text.slice(0,2500),sc:e.sc}; console.log(`[${k}] isError=${e.isError} bytes=${e.bytes}`, e.text.slice(0,500).replace(/\n/g,' | '));};
for (const [tag,env] of [['nobeta',{OCTOCODE_BETA:'',...X}],['beta',{OCTOCODE_BETA:'1',...X}]]) {
  let s;
  try { s = await startServer({env,timeoutMs:60000}); } catch (e) { console.log(tag,'START FAIL',e.message); continue; }
  const names=s.tools.map(t=>t.name); out[tag+'_tools']=names; console.log(tag,'tools',names.length,names.join(','));
  if (names.includes('astTopology')) { save(tag+'_topology', await s.call('astTopology',{analysis:'cycles',path:FX})); }
  else save(tag+'_topology_call', await s.call('astTopology',{analysis:'cycles',path:FX}));
  save(tag+'_rewrite_call', await s.call('astRewrite',{path:FX+'/src',langType:'typescript',ruleKind:'pattern',pattern:'console.log($A)',rewrite:'x($A)'}));
  if (tag==='nobeta') {
    save('mcp_bulk_mixed', await s.raw('localFetch',{queries:[{goal:'g',reasoning:'r',path:FX+'/src/a.ts',startLine:1,endLine:1},{goal:'g',reasoning:'r',path:'/etc/hosts'},{goal:'g',reasoning:'r',path:FX+'/src/b.ts',startLine:1,endLine:1}]}));
    save('mcp_bulk_missing_goal_row', await s.raw('localFetch',{queries:[{goal:'g',reasoning:'r',path:FX+'/src/a.ts',startLine:1,endLine:1},{path:FX+'/src/b.ts'}]}));
    save('mcp_rp', await s.raw('localFetch',{queries:[{goal:'g',reasoning:'r',path:FX+'/src/a.ts'}],responseCharLength:300}));
    save('mcp_redact', await s.call('localFetch',{path:FX+'/sec/creds.ts'}));
    save('mcp_redact_ls', await s.call('localSearch',{path:FX+'/sec',searchText:'ghp_',contextLines:0}));
    save('mcp_sandbox_etc', await s.call('localFetch',{path:'/etc/hosts'}));
    save('mcp_sandbox_symlink', await s.call('localFetch',{path:FX+'/sec/hosts_link'}));
    save('mcp_lf_default', await s.call('localFetch',{path:FX+'/src/a.ts',startLine:1,endLine:3}));
    save('mcp_ls_stale_restart_follow', await s.raw('localSearch',{queries:[{followUp:true,path:FX+'/many',searchText:'needle',pageSize:10,contextLines:0,matchContentLength:200}]}));
  }
  s.close();
}
{ const s=await startServer({env:{WORKSPACE_ROOT:FX,...X}}); save('mcp_ws_rel', await s.call('localFetch',{path:'src/a.ts',startLine:1,endLine:1})); s.close(); }
fs.writeFileSync(D+'/mcp2.json',JSON.stringify(out,null,1));
