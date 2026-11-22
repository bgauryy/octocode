import assert from 'node:assert/strict';
import test from 'node:test';
import { join } from 'node:path';
import { writeFileSync } from 'node:fs';
import { tempDir } from './test-tmp.mjs';
import { chunkFreshness, verifyPhase } from '../skills/octocode-graph/scripts/verify.mjs';

const chunks = ['a','b','c'].map(id => ({ id, path: id }));
const datas = new Map(chunks.map(c => [c.id, { snapshot: { id:c.id, root:`/repo/${c.id}/file.rs`, queryRoot:`scope/${c.id}` } }]));
const answer = (graph, index) => ({ index, data: { graph:{id:graph}, rows:[{path:`scope/${graph}/file.rs`, status:'modified'}] } });

test('mixed-snapshot batches keep each row scope, including response continuations', async () => {
  const calls = [];
  const run = async (tool, input) => {
    calls.push(input);
    if (input.cursor) return {results:[answer('b',1)]};
    const results = input.queries.map((q,i) => answer(q.graph,i));
    if (results.length === 1) return {results};
    return { results:results.slice(0,1), next:{tool,input:{cursor:'b-tail'}} };
  };
  const result = await chunkFreshness(chunks, datas, {jobs:1, graphBatch:2, run});
  assert.deepEqual(calls.filter(c => c.queries).map(c => c.queries.length), [2,1]);
  for (const {id} of chunks) assert.deepEqual(result.get(id).files, [{file:`${id}/file.rs`,status:'modified'}]);
});

test('a failed snapshot cannot erase freshness evidence from its batch peers', async () => {
  const run = async (_, {queries}) => ({ results:queries.map((q,i) => q.graph === 'b'
    ? {index:i,data:{errorCode:'staleSnapshot',error:'snapshot expired'}} : answer(q.graph,i)) });
  const result = await chunkFreshness(chunks, datas, {jobs:2, graphBatch:3, run});
  assert.match(result.get('b').reason,/snapshot expired/);
  assert.equal(result.get('a').files[0].file,'a/file.rs');
  assert.equal(result.get('c').files[0].file,'c/file.rs');
});

test('disk checks disclose unverified freshness and every out-of-range witness', async () => {
  const root = tempDir('verify-frozen-'), frozenChunks = [{id:'root',path:'.'}];
  for (const id of ['a','b']) writeFileSync(join(root,id),'line one\n');
  const claims = Array.from({length:25}, (_,i) => i + 2);
  const D = {files:['a','b'].map(id => ({id,chunk:'root'})), symbols:[], calls:[],
    imports:[{source:'a',target:'b',lines:claims}, {source:'a',target:'a',line:30}]};
  const summary = await verifyPhase(D, {root,chunks:frozenChunks,datas:new Map([['root',{frozen:true}]]),emit(){}});
  assert.equal(summary.ok,0);
  assert.equal(D.files[1].check.status,'unverified');
  assert.match(D.files[1].check.problems[0],/frozen copy/);
  const problems=D.files[0].check.problems;
  for (const n of [...claims,30]) assert.ok(problems.some(text => text.startsWith(`line ${n} `)), `missing line ${n}`);
});
