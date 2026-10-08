import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {binary,jsonCall,tempWorkspace,execFileSync} from './helpers.mjs';
import {registerBoundTools} from '../scripts/pi-extension.mjs';
function setup(){
 let hook,registered;
 const schema={type:'object',properties:{to:{type:'string'},topic:{type:'string'}},required:['to'],additionalProperties:false};
 registerBoundTools({on:(name,fn)=>{assert.equal(name,'before_provider_request');hook=fn;},registerTool:tool=>registered=tool},{tools:[{name:'send_message',description:'Send',inputSchema:schema}]});
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
 let registered;
 const inputSchema={type:'object',properties:{to:{type:'string'},topic:{type:'string'},body:{type:'string'}},
  required:['body'],additionalProperties:false,oneOf:[{required:['to']},{required:['topic']}],
  allOf:[{not:{required:['to','topic']}}]};
 registerBoundTools({registerTool:tool=>registered=tool},{tools:[{name:'send_message',description:'Send',inputSchema}]});
 assert.deepEqual(Object.keys(registered.parameters).sort(),['additionalProperties','properties','required','type']);
 assert.equal(registered.parameters.properties,inputSchema.properties);
 assert.ok(inputSchema.oneOf && inputSchema.allOf,'canonical schema is unchanged');
});
test('Pi preserves payload identity without own OpenAI function tools',()=>{
 const {hook}=setup();
 for(const payload of [{}, {tools:[]}, {tools:[{name:'send_message',input_schema:{}}]}, {tools:[{type:'function',name:'other'}]}])assert.equal(hook({payload}),payload);
});

function senderTool(t, executable=binary) {
 const workspace=tempWorkspace(t,'communication-pi-retry-',{real:true}),database=join(workspace,'shared.sqlite');
 const cli=jsonCall(binary,workspace,database);
 const sender=cli('join',{name:'sender',vendor:'pi'}).id,recipient=cli('join',{name:'recipient',vendor:'raw'}).id;
 let tool;
 const definition=JSON.parse(execFileSync(binary,['schema','send_message'],{encoding:'utf8'}));
 registerBoundTools({on(){},registerTool(value){tool=value;}},{binary:executable,workspace,database,session:sender,tools:[definition]});
 return {workspace,cli,sender,recipient,tool};
}

test('Pi validation errors retain CLI evidence and retry key without duplicating command flags',async t=>{
 const f=senderTool(t),key=`pi:${createHash('sha256').update('invalid-send').digest('hex')}`;
 await assert.rejects(f.tool.execute('invalid-send',{to:f.recipient,body:'Review',reasoning:''}),error=>{
  assert.match(error.message,/reasoning/);assert.ok(error.message.includes(key));
  assert.equal(error.message.includes('--workspace'),false);assert.equal(error.message.includes('--database'),false);
  return true;
 });
});

test('Pi exposes the same retry key after a committed send times out, preventing duplicate mail',async t=>{
 const directory=tempWorkspace(t,'communication-pi-postcommit-',{real:true}),shim=join(directory,'delayed.py');
 // The real CLI commits before this fixture stalls its process exit once.
 writeFileSync(shim,`import subprocess,sys,pathlib,time\nresult=subprocess.run([sys.executable,'-B',${JSON.stringify(binary)}]+sys.argv[1:],input=sys.stdin.buffer.read(),capture_output=True)\nsys.stdout.buffer.write(result.stdout);sys.stdout.buffer.flush()\nsys.stderr.buffer.write(result.stderr);sys.stderr.buffer.flush()\nmarker=pathlib.Path(${JSON.stringify(join(directory,'stalled'))})\nif result.returncode==0 and not marker.exists():\n marker.touch();time.sleep(30)\nsys.exit(result.returncode)\n`);
 const f=senderTool(t,shim),input={to:f.recipient,body:'Review the API',reasoning:'Verify ambiguous send recovery'};
 const key=`pi:${createHash('sha256').update('ambiguous-send').digest('hex')}`;
 await assert.rejects(f.tool.execute('ambiguous-send',input),error=>{
  assert.ok(error.message.includes(key));assert.match(error.message,/unknown/i);return true;
 });
 const original=f.cli('inbox',{},f.recipient).items[0];assert.equal(f.cli('inbox',{},f.recipient).items.length,1);
 const retry=await f.tool.execute('new-model-call',{...input,key});assert.equal(retry.details.id,original.id);
 assert.equal(f.cli('inbox',{},f.recipient).items.length,1);
});
