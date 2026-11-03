import {test} from 'node:test';
import type {TestContext} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {writeFileSync} from 'node:fs';
import {join} from 'node:path';
import type { JsonRecord } from './json-record.ts';
import {binary,jsonCall,tempWorkspace,execFileSync,fakePi,isJsonRecord,boundTool} from './helpers.ts';
import type {PiContext} from '../src/hosts/shared/command.ts';
import {registerBoundTools} from '../dist/hosts/pi-extension.js';
/** The provider-request hook and the tool descriptor that `registerBoundTools` hands to the Pi host. */
const context: PiContext={cwd:'/',sessionManager:{getSessionId:()=>'pi-extension-test'}};
function setup(){
 const {host,handlers,tools}=fakePi();
 const schema={type:'object',properties:{to:{type:'string'},topic:{type:'string'}},required:['to'],additionalProperties:false};
 registerBoundTools(host,{tools:[{name:'send_message',description:'Send',inputSchema:schema}]});
 assert.deepEqual([...handlers.keys()],['before_provider_request']);
 const handler=handlers.get('before_provider_request'),[registered]=tools;
 assert.ok(handler,'the provider-request hook is registered');
 assert.ok(registered,'the tool is registered');
 const hook=(event: {payload: JsonRecord}): JsonRecord=>{const result=handler(event,context);assert.ok(isJsonRecord(result),'the hook returns a payload');return result;};
 return {hook,schema,registered};
}
test('Pi keeps optional fields and projects top-level unions for provider function descriptors',()=>{
 const {hook,schema,registered}=setup();
 const unrelated={type:'function',name:'other',strict:true,parameters:schema};
 const custom={type:'custom',name:'send_message',format:{type:'grammar'}};
 const flat={type:'function',name:'send_message',parameters:schema};
 const nested={type:'function',function:{name:'send_message',strict:true,parameters:schema}};
 const payload={model:'test',tools:[flat,nested,unrelated,custom],input:[]};
 const before=structuredClone(payload),result=hook({payload});
 assert.deepEqual(payload,before,'Never mutate provider input');
 assert.equal(result.tools[0].strict,false);assert.equal(result.tools[1].function.strict,false);
 assert.deepEqual(result.tools[0].parameters,schema);assert.deepEqual(result.tools[1].function.parameters,schema);
 assert.deepEqual(registered.parameters,schema);assert.deepEqual(schema.required,['to']);
 assert.equal(result.tools[2],unrelated);assert.equal(result.tools[3],custom);assert.equal(result.input,payload.input);
 assert.equal(hook({payload:result}),result,'Already compatible payload is unchanged');
});
test('Pi removes only top-level composition; the shared runtime retains exact validation',()=>{
 const {host,tools}=fakePi();
 const inputSchema={type:'object',properties:{to:{type:'string'},topic:{type:'string'},parts:{type:'array',items:{type:'object'}}},
  required:['parts'],additionalProperties:false,oneOf:[{required:['to']},{required:['topic']}],
  allOf:[{not:{required:['to','topic']}}]};
 registerBoundTools(host,{tools:[{name:'send_message',description:'Send',inputSchema}]});
 const [registered]=tools;
 assert.ok(registered,'the tool is registered');
 assert.deepEqual(Object.keys(registered.parameters).sort(),['additionalProperties','properties','required','type']);
 assert.equal(registered.parameters.properties,inputSchema.properties);
 assert.ok(inputSchema.oneOf && inputSchema.allOf,'canonical schema is unchanged');
});
test('Pi preserves payload identity without own OpenAI function tools',()=>{
 const {hook}=setup();
 for(const payload of [{}, {tools:[]}, {tools:[{name:'send_message',input_schema:{}}]}, {tools:[{type:'function',name:'other'}]}])assert.equal(hook({payload}),payload);
});

function senderTool(t: TestContext, executable=binary) {
 const workspace=tempWorkspace(t,'communication-pi-retry-',{real:true}),database=join(workspace,'shared.sqlite');
 const cli=jsonCall(binary,workspace,database);
 const sender=cli('join',{name:'sender',vendor:'pi'}).id,recipient=cli('join',{name:'recipient',vendor:'raw'}).id;
 const {host,tools}=fakePi();
 const definition=JSON.parse(execFileSync(binary,['schema','messages'],{encoding:'utf8'}));
 registerBoundTools(host,{binary:executable,workspace,database,session:sender,tools:[definition]});
 const [tool]=tools;
 assert.ok(tool,'the send_message tool is registered');
 return {workspace,cli,sender,recipient,tool:boundTool(tool)};
}

test('Pi validation errors retain CLI evidence and idempotency key without duplicating command flags',async t=>{
 const f=senderTool(t),key=`pi:${createHash('sha256').update('invalid-send').digest('hex')}`;
 await assert.rejects(f.tool.execute('invalid-send',{type:'send',input:{to:f.recipient,parts:[{text:'Review'}],reasoning:''}}),(error: unknown)=>{assert.ok(error instanceof Error);
  assert.match(error.message,/reasoning/);assert.ok(error.message.includes(key));
  assert.equal(error.message.includes('--workspace'),false);assert.equal(error.message.includes('--database'),false);
  return true;
 });
});

test('Pi exposes the same idempotency key after a committed send times out, preventing duplicate mail',async t=>{
 const directory=tempWorkspace(t,'communication-pi-postcommit-',{real:true}),shim=join(directory,'delayed.mjs');
 // The real CLI commits before this fixture stalls its process exit once.
 writeFileSync(shim,`import {spawnSync} from 'node:child_process';import {existsSync,readFileSync,writeFileSync} from 'node:fs';
const result=spawnSync(process.execPath,[${JSON.stringify(binary)},...process.argv.slice(2)],{input:readFileSync(0)});
process.stdout.write(result.stdout);process.stderr.write(result.stderr);
const marker=${JSON.stringify(join(directory,'stalled'))};
if(result.status===0&&!existsSync(marker)){writeFileSync(marker,'');await new Promise(resolve=>setTimeout(resolve,30000));}
process.exit(result.status);
`);
 const f=senderTool(t,shim),input={type:'send',input:{to:f.recipient,parts:[{text:'Review the API'}],reasoning:'Verify ambiguous send recovery'}};
 const key=`pi:${createHash('sha256').update('ambiguous-send').digest('hex')}`;
 await assert.rejects(f.tool.execute('ambiguous-send',input),(error: unknown)=>{assert.ok(error instanceof Error);
  assert.ok(error.message.includes(key));assert.match(error.message,/unknown/i);return true;
 });
 const original=f.cli('inbox',{},f.recipient).items[0];assert.equal(f.cli('inbox',{},f.recipient).items.length,1);
 const retry=await f.tool.execute('new-model-call',{...input,input:{...input.input,key}});assert.equal(retry.details.id,original.id);
 assert.equal(f.cli('inbox',{},f.recipient).items.length,1);
});
