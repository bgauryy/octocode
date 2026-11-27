import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';

async function probe(t, { mode, category, resolvedSymbol, roles = [], declarations = [], partial = '', partialIndex, sameIndex = false, calls = [], symbols = [], batchSize = 2 } = {}) {
  const root = tempDir('graph-lsp-pages-');
  const script = join(root, 'fake.mjs');
  // The fake CLI and the runner are fixed code; what each probe varies reaches them as data (a file, argv).
  writeFileSync(join(root, 'probe.json'), JSON.stringify({ partial, partialIndex, resolvedSymbol, target: join(root, 'target.ts'), category: category ?? '', all: mode === 'all', sameIndex }));
  writeFileSync(script, `import { readFileSync } from 'node:fs';
const P = JSON.parse(readFileSync(new URL('./probe.json', import.meta.url), 'utf8'));
const [tool,,json] = process.argv.slice(2);
if (tool === 'config') { console.log(JSON.stringify({key:'WORKSPACE_ROOT',value:null})); process.exit(0); }
const input = JSON.parse(json ?? '{}');
const reason = P.partial;
const partialIndex = P.partialIndex;
const row = (index, name = 'target') => ({index, data: {kind: 'matches', resolvedSymbol: P.resolvedSymbol, ...(reason && (partialIndex === undefined || index === partialIndex) ? {isPartial: true, partialReasons: [reason]} : {}), files: [{path: P.target, matches: [{symbolName: name, line: 1, sites: [{line: 3}]}]}]}});
if(tool==='schema') console.log(JSON.stringify({queries:{maxItems:2}}));
else if(input.queries?.[0]?.operation === 'documentSymbols') { console.error('outline unavailable'); process.exit(5); }
else if (P.category) console.log(JSON.stringify({results:input.queries.map((_,index)=>({index,status:'empty',data:{category:P.category,reason:'definition outside scope'}}))}));
else if (P.all) console.log(JSON.stringify({results:input.queries.map((_, index)=>row(index))}));
else if(input.cursor) console.log(JSON.stringify({results:[P.sameIndex ? row(0, 'other') : row(1)]}));
else console.log(JSON.stringify({results:[row(0)], next:{kind:'continue',tool:'lspSearch',input:{cursor:'tail'}}}));
`);
  const D = { files: [{id:'source.ts', roles}, {id:'target.ts', roles:['test']}], symbols: [{id:'source.ts#first',file:'source.ts',name:'first',kind:'function',line:1},{id:'source.ts#second',file:'source.ts',name:'second',kind:'function',line:4},{id:'target.ts#target',file:'target.ts',name:'target',kind:'function',line:1}], calls, hubFiles:['source.ts'], coverage:{} };
  D.symbols.push(...symbols);
  if (sameIndex) D.symbols.splice(1, 1);
  const source = `const [url, input] = process.argv.slice(1);
const { lspPhase } = await import(url);
const { D, options } = JSON.parse(input);
const events = [];
const result = await lspPhase(D, { ...options, emit: e => events.push(e) });
console.log(JSON.stringify({ D, result, events }));`;
  const options = { root, mode, declarations, jobs: 2, batchSize };
  const child = spawnSync(process.execPath, ['--input-type=module', '-e', source, new URL('../src/runtime/lsp.ts', import.meta.url).href, JSON.stringify({ D, options })], { encoding: 'utf8', env: {...process.env, OCTOCODE_CLI: `${process.execPath} ${script}`} });
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

test('an answer whose only gap is withheld locations is complete for the graph and counts the omission', async t => {
  const source = 'source.ts#first';
  const target = 'target.ts#target';
  const overload = 'target.ts#Other.target';
  const {D, result} = await probe(t, {
    partial: 'locationsWithheld',
    calls: [{source, target, via:'graph'}, {source, target:overload, via:'graph'}],
    symbols: [{id:overload, file:'target.ts', name:'target', kind:'method', line:9}],
  });
  // Withheld locations are outside every allowed root: no graph file can be among them.
  assert.equal(result.resolved, 2);
  assert.equal(result.withheld, 2);
  assert.equal(result.lsp, 'ok');
  assert.deepEqual(result.unanswered, []);
  // The graph's overload call is now a real disagreement with a complete answer.
  assert.equal(result.graphOnly, 1);
  assert.equal(result.both, 1);
  assert.deepEqual(D.calls.map(edge => [edge.target, edge.via]).filter(([, via]) => via !== 'lsp'), [[target,'both'], [overload,'graph']]);
});

test('each unanswered probe is streamed once and stays in the final graph', async t => {
  const { result, events } = await probe(t, {partial:'buildScriptsOff', batchSize:1});
  const batches = events.filter(e => e.type === 'lsp');
  assert.equal(batches.length, 2);
  assert.deepEqual(batches.flatMap(e => e.unansweredAdded), result.unanswered);
  assert.ok(batches.every(e => !Object.hasOwn(e.stats, 'unanswered')));
});


test('mixed complete and incomplete LSP probes report partial coverage', async t => {
  const { result } = await probe(t, { partial: 'buildScriptsOff', partialIndex: 1 });
  assert.equal(result.resolved, 1);
  assert.equal(result.unanswered.length, 1);
  assert.equal(result.lsp, 'partial');
  // A withheld-only probe beside a complete one leaves nothing unanswered.
  const withheld = (await probe(t, { partial: 'locationsWithheld', partialIndex: 1 })).result;
  assert.deepEqual([withheld.resolved, withheld.withheld, withheld.unanswered.length, withheld.lsp], [2, 1, 0, 'ok']);
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

test('all probes declaration-only callers exactly once and records their source nodes', async t => {
  const only = {id:'source.ts#leaf',file:'source.ts',name:'leaf',kind:'function',line:10};
  const {D,result} = await probe(t, {mode:'all', declarations:[only,only]});
  assert.equal(result.probed,4);
  assert.ok(D.calls.some(c=>c.source===only.id));
  assert.ok(D.symbols.some(s=>s.id===only.id));
  assert.ok(D.calls.some(c=>c.source===c.target));
});

test('an all-withheld answer resolves for the graph and keeps the omission counted', async t => {
  // Every location was outside the allowed roots, so none is a graph file: nothing in the graph is a callee.
  const {result} = await probe(t,{category:'locationsWithheld'});
  assert.equal(result.unanswered.length, 0);
  assert.equal(result.withheld, result.probed);
  assert.equal(result.resolved, result.probed);
  // A different empty category is still not an answer.
  const other = (await probe(t,{category:'languageServerIndexing'})).result;
  assert.ok(other.unanswered.length && other.unanswered.every(x=>x.reasons.includes('languageServerIndexing')));
});

test('a moved anchor or two same-line caller declarations cannot confirm outgoing edges', async t => {
  const moved = await probe(t, { mode:'all', resolvedSymbol:{lineDeviation:1} });
  assert.equal(moved.D.calls.length, 0);
  assert.ok(moved.result.unanswered.every(x => x.reasons.includes('movedSource')));
  const rival = {id:'source.ts#Other.first',file:'source.ts',name:'first',kind:'method',line:1};
  const collision = await probe(t, {mode:'all', declarations:[rival]});
  assert.deepEqual(collision.result.unanswered.map(x=>x.id).sort(), [rival.id,'source.ts#first'].sort());
  assert.ok(collision.result.unanswered.every(x=>x.reasons.includes('ambiguousSource')));
  assert.ok(collision.D.calls.every(c=>c.source !== rival.id && c.source !== 'source.ts#first'));
});

test('a function holding a receiver call no rule bound to a declared method is probed for its callees', async () => {
  const root = tempDir('graph-lsp-receivers-');
  const script = join(root, 'fake.mjs');
  // usesIt (a test outside the probe scope) holds `client.open(..)`, which no rule bound: it is probed, and the server
  // resolves the receiver to Client.open. idle holds no such call and is not probed.
  writeFileSync(script, `const [tool,,json] = process.argv.slice(2);
if (tool === 'config') { console.log(JSON.stringify({key:'WORKSPACE_ROOT',value:null})); process.exit(0); }
const input = JSON.parse(json ?? '{}');
const { appendFileSync } = await import('node:fs');
appendFileSync(${JSON.stringify(join(root, 'asked'))}, JSON.stringify(input.queries) + '\\n');
const callees = {files: [{path: ${JSON.stringify(join(root, 'lib.ts'))}, matches: [{symbolName: 'open', line: 1, sites: [{line: 3}, {line: 5}]}]}]};
console.log(JSON.stringify({results: input.queries.map((q, index) => ({index, data: q.symbolName === 'usesIt' ? callees : {files: []}}))}));
`);
  const D = {
    files: [{id: 'lib.ts'}, {id: 'uses.test.ts', roles: ['test']}],
    symbols: [
      {id: 'lib.ts#Client.open', file: 'lib.ts', name: 'open', kind: 'method', line: 1},
      {id: 'uses.test.ts#usesIt', file: 'uses.test.ts', name: 'usesIt', kind: 'function', line: 1},
      {id: 'uses.test.ts#idle', file: 'uses.test.ts', name: 'idle', kind: 'function', line: 9},
    ],
    calls: [], hubFiles: [], coverage: {},
  };
  const source = `import { lspPhase } from ${JSON.stringify(new URL('../src/runtime/lsp.ts', import.meta.url).href)}; const D = ${JSON.stringify(D)}; const events = []; const result = await lspPhase(D, {root:${JSON.stringify(root)},emit:e=>events.push(e),receiverSources:['uses.test.ts#usesIt'],jobs:2,batchSize:2}); console.log(JSON.stringify({D,result,events}));`;
  const child = spawnSync(process.execPath, ['--input-type=module', '-e', source], { encoding: 'utf8', env: {...process.env, OCTOCODE_CLI: `${process.execPath} ${script}`} });
  assert.equal(child.status, 0, child.stderr);
  const { D: out, result } = JSON.parse(child.stdout);
  const asked = (await import('node:fs')).readFileSync(join(root, 'asked'), 'utf8').trim().split('\n').flatMap((l) => JSON.parse(l));
  assert.deepEqual(asked.map((q) => [q.operation, q.symbolName]), [['callees', 'usesIt']]);
  assert.equal(result.receiverSources, 1);
  assert.deepEqual(out.calls.map((c) => [c.source, c.target, c.via, c.lines, Boolean(c.test)]), [
    ['uses.test.ts#usesIt', 'lib.ts#Client.open', 'lsp', [3, 5], true],
  ]);
});

test('receiver call sources: a one-receiver call or the head of its chain naming a declared method; never a path, a field chain or a std name', async () => {
  const { receiverCallSources } = await import('../src/runtime/lsp.ts');
  const sources = receiverCallSources([
    { from: 'a.rs#chained', targetName: 'client.open_document_and_wait().await.expect' },
    { from: 'a.rs#own', targetName: 'self.run' },
    { from: 'a.rs#field', targetName: 'client.inner.clock.now' },
    { from: 'a.rs#path', targetName: 'Continuation::related_input' },
    { from: 'a.rs#std', targetName: 'items.len' },
    { from: 'a.rs#macro', targetName: 'format!' },
    // Typed by a type declared here: selected; typed by a std type (`HashMap<..>`): a std method, never selected.
    { from: 'a.rs#typed', targetName: 'client.run', receiverType: 'engine::Client' },
    { from: 'a.rs#foreign', targetName: 'seen.run', receiverType: 'HashMap<String, u8>' },
  ], new Set(['open_document_and_wait', 'run', 'now', 'related_input']), new Set(['Client']));
  assert.deepEqual([...sources].sort(), ['a.rs#chained', 'a.rs#own', 'a.rs#typed']);
});

test('a refresh carries an answered probe unless its file or a file it calls into changed', async () => {
  const { carriedProbes } = await import('../src/runtime/lsp.ts');
  const previous = {
    symbols: [
      { id: 'a.rs#kept', file: 'a.rs', name: 'kept', kind: 'function', probed: true },
      { id: 'b.rs#edited', file: 'b.rs', name: 'edited', kind: 'function', probed: true },
      { id: 'a.rs#callsEdited', file: 'a.rs', name: 'callsEdited', kind: 'function', probed: true },
      { id: 'a.rs#neverAnswered', file: 'a.rs', name: 'neverAnswered', kind: 'function' },
    ],
    calls: [
      { source: 'a.rs#kept', target: 'c.rs#helper', via: 'both', lines: [3] },
      { source: 'a.rs#kept', target: 'c.rs#other', via: 'graph', lines: [4] },
      { source: 'a.rs#callsEdited', target: 'b.rs#edited', via: 'lsp', lines: [9] },
    ],
  };
  const carried = carriedProbes(previous, (file) => file === 'b.rs');
  assert.deepEqual([...carried.sources], ['a.rs#kept']);
  assert.deepEqual(carried.calls.map((c) => [c.source, c.target]), [['a.rs#kept', 'c.rs#helper']]);
});

test('carried answers are applied and never probed again; a new answer marks its function probed', async () => {
  const root = tempDir('graph-lsp-carried-');
  const script = join(root, 'fake.mjs');
  writeFileSync(script, `const [tool,,json] = process.argv.slice(2);
if (tool === 'config') { console.log(JSON.stringify({key:'WORKSPACE_ROOT',value:null})); process.exit(0); }
const input = JSON.parse(json ?? '{}');
const { appendFileSync } = await import('node:fs');
appendFileSync(${JSON.stringify(join(root, 'asked'))}, JSON.stringify(input.queries) + '\\n');
console.log(JSON.stringify({results: input.queries.map((q, index) => ({index, data: {files: []}}))}));
`);
  const D = {
    files: [{id: 'a.ts'}, {id: 'c.ts'}],
    symbols: [
      {id: 'a.ts#kept', file: 'a.ts', name: 'kept', kind: 'function', line: 1},
      {id: 'a.ts#fresh', file: 'a.ts', name: 'fresh', kind: 'function', line: 5},
      {id: 'c.ts#helper', file: 'c.ts', name: 'helper', kind: 'function', line: 1},
      {id: 'c.ts#only', file: 'c.ts', name: 'only', kind: 'function', line: 4},
    ],
    calls: [{source: 'a.ts#kept', target: 'c.ts#helper', line: 2, lines: [2], count: 1, via: 'graph', resolution: ['import']}],
    hubFiles: ['a.ts'], coverage: {},
  };
  const carried = { sources: ['a.ts#kept'], calls: [
    {source: 'a.ts#kept', target: 'c.ts#helper', via: 'both', lines: [2], resolution: ['import']},
    {source: 'a.ts#kept', target: 'c.ts#only', via: 'lsp', lines: [3], resolution: ['lsp-callee'], confidence: 'high', count: 1, line: 3},
  ] };
  const source = `import { lspPhase } from ${JSON.stringify(new URL('../src/runtime/lsp.ts', import.meta.url).href)}; const D = ${JSON.stringify(D)}; const c = ${JSON.stringify(carried)}; const result = await lspPhase(D, {root:${JSON.stringify(root)},emit:()=>{},carried:{sources:new Set(c.sources),calls:c.calls},jobs:2,batchSize:2}); console.log(JSON.stringify({D,result}));`;
  const child = spawnSync(process.execPath, ['--input-type=module', '-e', source], { encoding: 'utf8', env: {...process.env, OCTOCODE_CLI: `${process.execPath} ${script}`} });
  assert.equal(child.status, 0, child.stderr);
  const { D: out, result } = JSON.parse(child.stdout);
  const asked = (await import('node:fs')).readFileSync(join(root, 'asked'), 'utf8').trim().split('\n').flatMap((l) => JSON.parse(l));
  assert.deepEqual(asked.map((q) => q.symbolName), ['fresh'], 'only the uncarried function is probed');
  assert.equal(result.carried, 1);
  assert.deepEqual(out.calls.map((c) => [c.source, c.target, c.via]).sort(), [['a.ts#kept', 'c.ts#helper', 'both'], ['a.ts#kept', 'c.ts#only', 'lsp']]);
  assert.deepEqual(out.symbols.filter((s) => s.probed).map((s) => s.id).sort(), ['a.ts#fresh', 'a.ts#kept']);
});
