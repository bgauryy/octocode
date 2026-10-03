import {test} from 'node:test';
import assert from 'node:assert/strict';
import {registerBoundTools} from '../scripts/pi-extension.mjs';
function setup(){
 let hook,registered;
 const schema={type:'object',properties:{to:{type:'string'},topic:{type:'string'}},required:['to'],additionalProperties:false};
 registerBoundTools({on:(name,fn)=>{assert.equal(name,'before_provider_request');hook=fn;},registerTool:tool=>registered=tool},{tools:[{name:'send_message',description:'Send',inputSchema:schema}]});
 return {hook,schema,registered};
}
test('Pi preserves optional canonical fields with explicit non-strict own function descriptors',()=>{
 const {hook,schema,registered}=setup();
 const unrelated={type:'function',name:'other',strict:true,parameters:schema};
 const custom={type:'custom',name:'send_message',format:{type:'grammar'}};
 const flat={type:'function',name:'send_message',parameters:schema};
 const nested={type:'function',function:{name:'send_message',strict:true,parameters:schema}};
 const payload={model:'test',tools:[flat,nested,unrelated,custom],input:[]};
 const before=structuredClone(payload),result=hook({payload});
 assert.deepEqual(payload,before,'Never mutate provider input');
 assert.equal(result.tools[0].strict,false);assert.equal(result.tools[1].function.strict,false);
 assert.equal(result.tools[0].parameters,schema);assert.equal(result.tools[1].function.parameters,schema);
 assert.equal(registered.parameters,schema);assert.deepEqual(schema.required,['to']);
 assert.equal(result.tools[2],unrelated);assert.equal(result.tools[3],custom);assert.equal(result.input,payload.input);
 assert.equal(hook({payload:result}),result,'Already compatible payload is unchanged');
});
test('Pi preserves payload identity without own OpenAI function tools',()=>{
 const {hook}=setup();
 for(const payload of [{}, {tools:[]}, {tools:[{name:'send_message',input_schema:{}}]}, {tools:[{type:'function',name:'other'}]}])assert.equal(hook({payload}),payload);
});
