import {cpSync,existsSync,mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
export async function snapshot(subject, root, core) {
  const destination = path.join(root,subject);
  if (existsSync(destination)) throw Error(`Refusing to replace existing snapshot: ${destination}`);
  const source = path.resolve(core);
  const schemas = await import(pathToFileURL(path.join(source,'dist/schema.js')));
  const {buildMcpInstructions} = await import(pathToFileURL(path.join(source,'dist/mcp.js')));
  const require = createRequire(path.join(source,'package.json'));
  const {z} = require('zod');
  const zodPath = require.resolve('zod/package.json');
  mkdirSync(destination,{recursive:true});
  cpSync(path.join(source,'dist'),path.join(destination,'core-dist'),{recursive:true});
  mkdirSync(path.join(destination,'core-dist/node_modules'),{recursive:true});
  cpSync(path.dirname(zodPath),path.join(destination,'core-dist/node_modules/zod'),{recursive:true});
  const tools = schemas.DIRECT_TOOL_DEFINITIONS.map(tool => ({name:tool.name,description:tool.description,
    parameters:z.toJSONSchema(tool.inputSchema,{io:'input',unrepresentable:'any'}),
    querySchema:z.toJSONSchema(tool.schema,{io:'input',unrepresentable:'any'})}));
  const metadata = {capturedAt:new Date().toISOString(),node:process.version,platform:process.platform,arch:process.arch,
    corePackage:JSON.parse(readFileSync(path.join(source,'package.json'),'utf8')),
    git:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),
    zod:{version:JSON.parse(readFileSync(zodPath,'utf8')).version,preserved:'core-dist/node_modules/zod'}};
  for (const [key,url,body] of [['ollamaVersion','version'],['models','tags'],['model','show',{model:'gemma4:latest'}]]) {
    const response = await fetch(`http://127.0.0.1:11434/api/${url}`,{method:body?'POST':'GET',
      headers:{'Content-Type':'application/json'},body:body?JSON.stringify(body):undefined,signal:AbortSignal.timeout(5000)});
    if (!response.ok) throw Error(`Metadata ${url}: ${response.status}`);
    metadata[key] = await response.json();
  }
  writeFileSync(path.join(destination,'snapshot.json'),JSON.stringify({instructions:buildMcpInstructions(tools.map(t=>t.name)),tools},null,2));
  writeFileSync(path.join(destination,'metadata.json'),JSON.stringify(metadata,null,2));
  return {destination,tools:tools.length,capturedAt:metadata.capturedAt};
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const subject = process.argv[2] ?? 'candidate';
  const root = path.resolve(process.argv[3] ?? '.octocode/octocode-eval-benchmark/artifact-search-v2');
  const core = path.resolve(process.argv[4] ?? '../octocode-mcp-host/packages/octocode-core');
  console.log(JSON.stringify(await snapshot(subject,root,core)));
}
