import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';

async function probe(t, { roles = [], declarations = [], partial = '', partialIndex, sameIndex = false, calls = [], symbols = [], batchSize = 2 } = {}) {
  const root = tempDir('graph-lsp-pages-');
  const script = join(root, 'fake.mjs');
  writeFileSync(script, `const [tool,,json] = process.argv.slice(2);
if (tool === 'config') { console.log(JSON.stringify({key:'WORKSPACE_ROOT',value:null})); process.exit(0); }
const input = JSON.parse(json ?? '{}');
const reason = ${JSON.stringify(partial)};
const partialIndex = ${JSON.stringify(partialIndex)};
const row = (index, name = 'target') => ({index, data: {kind: 'matches', ...(reason && (partialIndex === undefined || index === partialIndex) ? {isPartial: true, partialReasons: [reason]} : {}), files: [{path: ${JSON.stringify(join(root, 'target.ts'))}, matches: [{symbolName: name, line: 1, sites: [{line: 3}]}]}]}});
if(tool==='schema') console.log(JSON.stringify({queries:{maxItems:2}}));
else if(input.queries?.[0]?.operation === 'documentSymbols') { console.error('outline unavailable'); process.exit(5); }
else if(input.cursor) console.log(JSON.stringify({results:[row(${sameIndex ? "0, 'other'" : '1'})]}));
else console.log(JSON.stringify({results:[row(0)], next:{kind:'continue',tool:'lspSearch',input:{cursor:'tail'}}}));
`);
  const D = { files: [{id:'source.ts', roles}, {id:'target.ts', roles:['test']}], symbols: [{id:'source.ts#first',file:'source.ts',name:'first',kind:'function',line:1},{id:'source.ts#second',file:'source.ts',name:'second',kind:'function',line:4},{id:'target.ts#target',file:'target.ts',name:'target',kind:'function',line:1}], calls, hubFiles:['source.ts'], coverage:{} };
  D.symbols.push(...symbols);
  if (sameIndex) D.symbols.splice(1, 1);
  const source = `import { lspPhase } from ${JSON.stringify(new URL('../skills/octocode-graph/scripts/lsp.mjs', import.meta.url).href)}; const D = ${JSON.stringify(D)}; const events = []; const result = await lspPhase(D, {root:${JSON.stringify(root)},emit:e=>events.push(e),declarations:${JSON.stringify(declarations)},jobs:2,batchSize:${batchSize}}); console.log(JSON.stringify({D,result,events}));`;
  const child = spawnSync(process.execPath, ['--input-type=module', '-e', source], { encoding: 'utf8', env: {...process.env, OCTOCODE_CLI: `${process.execPath} ${script}`} });
  assert.equal(child.status, 0, child.stderr);
  return JSON.parse(child.stdout);
}

test('LSP batch probes finish response pagination before assigning each query answer', async t => {
  const {D,result} = await probe(t);
  assert.equal(result.resolved, 2, JSON.stringify(result));
  assert.deepEqual(D.calls.map(c=>c.source).sort(), ['source.ts#first','source.ts#second']);
});

test('a nonempty partial LSP answer remains unanswered with its reasons', async t => {
  const {D,result} = await probe(t, {partial:'buildScriptsOff'});
  assert.equal(result.resolved, 0);
  assert.equal(D.calls.length, 0);
  assert.equal(result.unanswered.length, 2);
  assert.ok(result.unanswered.every(x=>x.reasons.includes('buildScriptsOff')), JSON.stringify(result));
});

test('a hoisted response walk retains every complete slice of one LSP query', async t => {
  const {D,result} = await probe(t, {sameIndex:true});
  assert.equal(result.resolved, 1);
  assert.deepEqual(D.calls.map(c=>c.target).sort(), ['target.ts#other@1','target.ts#target']);
});

test('withheld locations retain exact in-scope positives and leave other graph calls unconfirmed', async t => {
  const source = 'source.ts#first';
  const target = 'target.ts#target';
  const overload = 'target.ts#Other.target';
  const {D, result} = await probe(t, {
    partial: 'locationsWithheld',
    calls: [{source, target, via:'graph'}, {source, target:overload, via:'graph'}],
    symbols: [{id:overload, file:'target.ts', name:'target', kind:'method', line:9}],
  });
  assert.equal(result.resolved, 0);
  assert.equal(result.partialWithMatches, 2);
  assert.equal(result.lsp, 'partial');
  assert.equal(result.unanswered.length, 2);
  assert.equal(result.graphOnly, 0);
  assert.equal(result.both, 1);
  assert.equal(result.lspOnly, 1);
  assert.deepEqual(D.calls.map(edge => [edge.target, edge.via]), [[target,'both'], [overload,'graph'], [target,'lsp']]);
});

test('each unanswered probe is streamed once and stays in the final graph', async t => {
  const { result, events } = await probe(t, {partial:'locationsWithheld', batchSize:1});
  const batches = events.filter(e => e.type === 'lsp');
  assert.equal(batches.length, 2);
  assert.deepEqual(batches.flatMap(e => e.unansweredAdded), result.unanswered);
  assert.ok(batches.every(e => !Object.hasOwn(e.stats, 'unanswered')));
});


test('mixed complete and incomplete LSP probes report partial coverage', async t => {
  const { result } = await probe(t, { partial: 'locationsWithheld', partialIndex: 1 });
  assert.equal(result.resolved, 1);
  assert.equal(result.partialWithMatches, 1);
  assert.equal(result.unanswered.length, 1);
  assert.equal(result.lsp, 'partial');
});

test('LSP edges inherit the caller file test role and keep their call sites', async t => {
  const { D } = await probe(t, { roles: ['test'] });
  assert.ok(D.calls.length > 0 && D.calls.every(c => c.test && c.lines.includes(3)));
  const production = await probe(t);
  assert.ok(production.D.calls.every(c => !c.test));
});

test('a complete LSP result confirms only the declaration at the returned location', async t => {
  const source = 'source.ts#first';
  const exact = 'target.ts#target';
  const other = 'target.ts#Other.target';
  const { D } = await probe(t, {
    calls: [{source, target:other, via:'graph'}],
    symbols: [{id:other, file:'target.ts', name:'target', kind:'method', line:9}],
  });
  assert.deepEqual(D.calls.filter(e => e.source === source).map(e => [e.target, e.via]),
    [[other,'graph'], [exact,'lsp']]);
});

test('duplicate observations of one declaration agree, but a same-line name collision stays unanswered', async t => {
  const known = {id:'target.ts#target',file:'target.ts',name:'target',kind:'function',line:1};
  const unique = await probe(t, {declarations:[known]});
  assert.equal(unique.result.resolved,2);
  const ambiguous = await probe(t, {declarations:[known,{...known,id:'target.ts#Other.target'}]});
  assert.equal(ambiguous.result.resolved,0);
  assert.equal(ambiguous.D.calls.length,0);
  for (const unanswered of ambiguous.result.unanswered) {
    assert.deepEqual(unanswered.reasons,['ambiguousDefinition']);
    assert.deepEqual(unanswered.ambiguousDefinitions,[{file:'target.ts',name:'target',line:1,sites:[{line:3}]}]);
  }
});
