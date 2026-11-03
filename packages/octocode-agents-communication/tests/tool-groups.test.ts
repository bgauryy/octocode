import { test } from 'node:test';
import assert from 'node:assert/strict';
const { catalog } = await import('../dist/runtime.js');

test('worker discovery exposes five typed capability families', () => {
  assert.deepEqual(catalog.selectedTools().map(tool => tool.name), ['agents', 'messages', 'leases', 'documents', 'records']);
});

test('typed calls reject hidden variants, unrelated fields and identity overrides with precise errors', () => {
  assert.throws(() => catalog.toolCommand('records',{type:'write',input:{type:'checkpoint',data:{}}},'messaging'), /type|enum/);
  assert.throws(() => catalog.toolCommand('messages',{type:'read',input:{to:'peer'}}), /to|additional/);
  assert.throws(() => catalog.toolCommand('agents',{type:'list',input:{},session:'peer'}), /session|additional/);
  assert.throws(() => catalog.toolCommand('documents',{type:'publish',input:{name:'review.md',content:'proof'}}), /reasoning/);
  assert.throws(() => catalog.toolCommand('records',{type:'write',input:{type:'event',data:{content:'missing event name'}}}), /name|required/);
});

test('only runtime-owned continuations become executable family calls', () => {
  const userData={next:{command:'fetch',input:{type:'event'}},bodyOmitted:true};
  const input={items:[{type:'checkpoint',data:userData},{type:'message',data:{contentOmitted:true,next:{command:'inbox',input:{message:7}}}}],next:{command:'fetch',input:{type:'message',after:17}}};
  const result=catalog.toolResult('fetch',input);
  assert.deepEqual(result,{items:[{type:'checkpoint',data:userData},{type:'message',data:{contentOmitted:true,next:{command:'messages',input:{type:'read',input:{message:7}}}}}],next:{command:'records',input:{type:'read',input:{type:'message',after:17}}}});
  assert.deepEqual(input.items[1].data.next,{command:'inbox',input:{message:7}},'projection must not mutate source evidence');
  assert.deepEqual(catalog.toolResult('lock_many',{ok:false,next:{command:'lock_many',input:{paths:[{path:'src'}],reasoning:'Reserve edits',wait:true}}}),{ok:false,next:{command:'leases',input:{type:'acquire',input:{paths:[{path:'src'}],reasoning:'Reserve edits',wait:true}}}});
});

test('transport retry keys are nested only for retryable writes and explicit keys survive', () => {
  assert.deepEqual(catalog.keyedInput('messages',{type:'send',input:{to:'peer',parts:[{text:'notice'}],reasoning:'coordinate'}},'stable'),{type:'send',input:{to:'peer',parts:[{text:'notice'}],reasoning:'coordinate',key:'stable'}});
  assert.deepEqual(catalog.keyedInput('messages',{type:'read',input:{}},'stable'),{type:'read',input:{}});
  assert.deepEqual(catalog.keyedInput('records',{type:'write',input:{key:'supplied'}},'transport'),{type:'write',input:{key:'supplied'}});
});
