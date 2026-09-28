import assert from 'node:assert/strict';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {cases} from './cases.mjs';
import {normalize,evaluate} from './grader.mjs';
import {repairRequest} from './transport.mjs';
const root = process.argv[2] ? path.resolve(process.argv[2]) : undefined;
const schema = root
  ? await import(pathToFileURL(path.join(root,'baseline/core-dist/schema.js')))
  : await import('@octocodeai/config/schema');
// Root mode intentionally accepts immutable pre-split benchmark snapshots.
const specs = schema.DIRECT_TOOL_DEFINITIONS ?? (root ? schema.DIRECT_TOOL_SPECIFICATIONS : undefined);
if (!Array.isArray(specs)) throw Error('Tool definitions are unavailable');
const normalized = output => ({sensor:true,transport:true,output});
const caseById = id => cases.find(test => test.id===id);
const check = (id,output) => evaluate(caseById(id),normalized(output),specs);
let assertions = 0;
for (const test of cases) {assert.equal(evaluate(test,normalized(test.reference),specs).success,true,`${test.id}: reference invalid`);assertions++;}
function mutation(id,change,metric='semantic') {
  const output = structuredClone(caseById(id).reference);change(output);
  assert.equal(check(id,output)[metric],false,`${id}: mutation not rejected`);assertions++;
}
mutation('remote-anchored-read',o=>o.calls[0].arguments.queries[0].branch='wrong');
mutation('remote-anchored-read',o=>o.calls[0].arguments.queries[0].minify='symbols');
mutation('local-exact',o=>o.calls[0].arguments.queries[0].minify='standard');
mutation('remote-symbol-search',o=>o.calls[0].arguments.queries[0].keywords=['unrelated']);
mutation('rust-discovery',o=>o.calls[0].arguments.queries[0].keywords=['unicode emoji']);
mutation('clone-known',o=>o.calls[0].arguments.queries[0].sparsePath='src');
mutation('independent-package-batch',o=>o.calls[0].arguments.queries.pop());
mutation('independent-package-batch',o=>o.calls[0].arguments.queries.push(o.calls[0].arguments.queries[0]));
mutation('local-text',o=>o.calls[0].arguments.queries[0].unique='list','schema');
mutation('unsupported-python-discovery',o=>o.answer='I will search by name using an unofficial scraper.');
mutation('unsupported-python-discovery',o=>o.answer='PyPI keyword discovery is unsupported. Give an exact package name. I will use a web search instead.');
assert.equal(check('unsupported-python-discovery',{calls:[],answer:'PyPI keyword discovery is unsupported. I cannot use an unofficial scraper. Please give an exact package name.'}).success,true);assertions++;
const batch = structuredClone(caseById('independent-package-batch').reference);batch.calls[0].arguments.queries.reverse();assert.equal(check('independent-package-batch',batch).success,true);assertions++;
for (const arm of ['native','emulated']) {
  const valid = caseById('python-exact').reference;
  const raw = {done:true,done_reason:'stop',message:arm==='native'
    ? {role:'assistant',tool_calls:valid.calls.map(c=>({function:{name:c.tool,arguments:c.arguments}})),content:''}
    : {role:'assistant',content:JSON.stringify(valid)}};
  assert.equal(evaluate(caseById('python-exact'),normalize(raw,arm),specs).success,true);assertions++;
  raw.done=false;assert.equal(evaluate(caseById('python-exact'),normalize(raw,arm),specs).success,false);assertions++;
}
assert.equal(normalize({done:true,message:{role:'assistant',content:'{'}},'emulated').transport,false);assertions++;
assert.equal(normalize({done:true,message:{role:'assistant',tool_calls:[{function:{name:'artifactSearch',arguments:'{}'}}]}},'native').transport,false);assertions++;
assert.equal(check('clone-disabled',{calls:[{tool:'ghCloneRepo',arguments:{queries:[{owner:'rust-lang',repo:'rust'}]}}],answer:''}).unnecessaryCalls,1);assertions++;
for (const output of [{calls:[null]},{calls:[1]},{calls:[{}]},
  {calls:[{tool:'artifactSearch'}]},{calls:[{tool:'artifactSearch',arguments:{queries:[null]}}]},
  {calls:[{tool:'artifactSearch',arguments:{queries:['bad']}}]},{calls:'bad'},null]) {
  const raw={done:true,done_reason:'stop',message:{role:'assistant',content:JSON.stringify(output)}};
  assert.equal(evaluate(caseById('python-exact'),normalize(raw,'emulated'),specs).success,false);assertions++;
}
for (const raw of [null,{}, {done:true,message:null},{done:true,message:{role:'assistant',tool_calls:[null]}}]) {
  assert.equal(evaluate(caseById('python-exact'),normalize(raw,'native'),specs).success,false);assertions++;
}
for (const malformed of [null,{function:{name:'artifactSearch',arguments:'{}'}},{function:{name:'artifactSearch',arguments:null}}]) {
  const request=repairRequest({messages:[{role:'user',content:'Original request'}]},
    {raw:{message:{role:'assistant',tool_calls:[malformed]}}},
    {normalized:{transportError:'Invalid call'},metrics:{errors:[]}},'native');
  assert(request.messages.every(message=>!message.tool_calls));
  assert.equal(request.messages.at(-1).role,'user');assertions++;
}
const repaired=repairRequest({messages:[{role:'user',content:'Original request'}]},
  {raw:{message:{role:'assistant',content:'',tool_calls:[{function:{name:'artifactSearch',arguments:{queries:[{}]}}}]}}},
  {normalized:{},metrics:{errors:[{tool:'artifactSearch',issues:[{path:['queries',0,'type'],message:'Required'}]}]}},'native');
assert.equal(repaired.messages.at(-1).role,'tool');
assert.equal(repaired.messages.at(-1).tool_name,'artifactSearch');assertions++;
for (const test of cases) {
  const output=structuredClone(test.reference);
  if(test.expected.tool==='none') output.calls=[{tool:'artifactSearch',arguments:{queries:[{type:'pypi',keywords:['invented']}]}}];
  else output.calls[0].arguments.responseSnapshot='invented';
  assert.equal(evaluate(test,normalized(output),specs).success,false,`${test.id}: unexpected continuation accepted`);assertions++;
}
const forbiddenByTool={artifactSearch:['cursor','invented'],ghSearchCode:['path','unrequested'],ghGetFileContent:['matchString','unrequested'],
  ghSearchHistory:['author','unrequested'],ghGetHistoryItem:['charOffset',12],ghCloneRepo:['sparsePath','src'],
  localSearch:['include',['*.md']],structureSearch:['names',['not-requested.ts']],localFetch:['matchString','unrequested'],
  lspSearch:['workspaceRoot','/unrelated/repo']};
for(const test of cases.filter(test=>test.expected.tool!=='none')) {
  const output=structuredClone(test.reference), [key,value]=forbiddenByTool[test.expected.tool];
  output.calls[0].arguments.queries[0][key]=value;
  assert.equal(evaluate(test,normalized(output),specs).semantic,false,`${test.id}: scope-changing ${key} accepted`);assertions++;
  const positive=structuredClone(test.reference);positive.calls[0].arguments.responseCharOffset=0;
  positive.calls[0].arguments.queries[0].goal='Resolve the immediate question';
  assert.equal(evaluate(test,normalized(positive),specs).success,true,`${test.id}: harmless metadata rejected`);assertions++;
}
for(const [id,key,value] of [['lsp-identity','includeDeclaration',false],['clone-known','forceRefresh',true],
  ['local-text','page',1],['local-text','include',[]],['local-exact','minify','none'],['structure-files','detail','full'],
  ['remote-symbol-search','concise',true],['local-exact','fullContent',false],['remote-anchored-read','forceRefresh',true]]) {
  const output=structuredClone(caseById(id).reference);output.calls[0].arguments.queries[0][key]=value;
  assert.equal(check(id,output).success,true,`${id}: harmless ${key} rejected`);assertions++;
}
console.log(JSON.stringify({status:'PASS',references:cases.length,assertions}));
