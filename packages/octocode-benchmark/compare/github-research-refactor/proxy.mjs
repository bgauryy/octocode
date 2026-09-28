import { appendFileSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
export function admit(name, args, allowed, scope) {
  if (!allowed.includes(name)) throw new Error('Tool outside trial scope');
  const rows = args.queries ?? [args]; let cells = 0;
  if (!Array.isArray(rows) || !rows.length) throw new Error('Invalid rows');
  for (const row of rows) {
    if (name === 'clasify') {
      if (!Array.isArray(row.resources) || !Array.isArray(row.questions)) throw new Error('Matrix required');
      cells += row.resources.length * row.questions.length;
      for (const resource of row.resources) {
        const context = resource.context;
        if (Object.hasOwn(context ?? {}, 'value')) continue;
        if (context?.tool !== 'ghGetFileContent' || context?.query?.queries) throw new Error('Only pinned GitHub read resources');
        admit(context.tool, context.query, allowed, scope);
      }
    } else {
      if (row.owner !== scope.owner || (name !== 'ghSearchRepo' && row.repo !== scope.repo)) throw new Error('Repository outside scope');
      if (name === 'ghGetFileContent' && row.branch !== scope.ref) throw new Error('Pinned source ref required');
      if (row.materialize) throw new Error('Materialization prohibited');
      if (name === 'ghStructure' && row.branch !== scope.ref) throw new Error('Pinned tree ref required');
      if (name === 'ghGetHistoryItem' && (row.operation !== 'commit' || row.ref !== scope.ref)) throw new Error('Pinned commit required');
    }
  }
  return { cells };
}
async function main() {
 const config = JSON.parse(readFileSync(process.env.FLOW_BENCH_CONFIG));
 const subject = JSON.parse(readFileSync(config.subject));
 const log = value => appendFileSync(resolve(config.runDir,'calls.jsonl'),JSON.stringify(value)+'\n',{mode:0o600});
 const downstream = new Client({name:'github-research-eval',version:'1'});
 const env = {...process.env,TOOLS_TO_RUN:subject.tools.join(','),ENABLE_LOCAL:'false',ENABLE_CLONE:'false',OCTOCODE_BETA:'false',ALLOWED_PATHS:config.fixture,WORKSPACE_ROOT:config.fixture,OCTOCODE_STORAGE_MODE:'memory',OCTOCODE_ENABLE_STATS:'false'};
 delete env.FLOW_BENCH_CONFIG;
 const transport = new StdioClientTransport({command:process.execPath,args:[config.entrypoint],cwd:config.fixture,env,stderr:'pipe'}); transport.stderr?.resume();
 let server, closing=false; const close=async()=>{if(closing)return;closing=true;await Promise.allSettled([downstream.close(),server?.close()]);};
 for(const signal of ['SIGINT','SIGTERM'])process.once(signal,()=>void close()); process.stdin.once('end',()=>void close());
 try {
  await downstream.connect(transport);const catalog=await downstream.listTools();
  if(catalog.nextCursor || catalog.tools.length!==subject.tools.length || catalog.tools.some(t=>t.outputSchema) || subject.tools.some(n=>!catalog.tools.some(t=>t.name===n)))throw new Error('Unexpected catalog');
  log({event:'catalog',tools:catalog.tools,instructions:subject.instructions});
  server=new Server({name:'github-research-eval',version:'1'},{capabilities:{tools:{}},instructions:subject.instructions});
  server.setRequestHandler(ListToolsRequestSchema,async()=>catalog);
  let calls=0,cells=0;
  server.setRequestHandler(CallToolRequestSchema,async(request,extra)=>{
   const start=performance.now(),{name,arguments:args={}}=request.params;let result,admitted=false;
   try {const cost=admit(name,args,subject.tools,config.scope);if(++calls>12||cells+cost.cells>50)throw new Error('Budget');cells+=cost.cells;admitted=true;result=await downstream.callTool({name,arguments:args},undefined,{timeout:90000,signal:extra.signal});}
   catch {result={isError:true,content:[{type:'text',text:'Trial scope, budget, or downstream request failed.'}]};}
   log({event:'call',name,args,admitted,durationMs:performance.now()-start,result});return result;
  });await server.connect(new StdioServerTransport());
 } catch {await close();throw new Error('GitHub evaluation proxy startup failed');}
}
if(process.env.FLOW_BENCH_CONFIG)main().catch(e=>{console.error(e.message);process.exitCode=1;});
