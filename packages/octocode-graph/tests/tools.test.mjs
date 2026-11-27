import assert from 'node:assert/strict';
import test from 'node:test';
import { pagedTool, rowsOf } from '../src/runtime/tools.ts';
import { graphBuild, readGraph } from '../src/runtime/graph-pages.ts';

const next = (cursor, tool = 'lspSearch') => ({ kind: 'continue', tool, input: { cursor } });
const response = (results, cursor, tool) => ({ results, ...(cursor ? { next: next(cursor, tool) } : {}) });
const row = (data, part, index = 0) => ({ index, data, ...(part ? { rowPart: part } : {}) });
// The runtime serves a row's page walk as the reply's one `next` (it continues every unfinished row): a fixture
// that writes the walk on its row (`next.nextPage`) is served the same way.
const served = (page) => {
  if (!page || page.next || !Array.isArray(page.results)) return page;
  const out = structuredClone(page);
  for (const result of out.results) {
    const walk = result?.data?.next?.nextPage;
    if (!walk) continue;
    delete result.data.next.nextPage;
    if (!Object.keys(result.data.next).length) delete result.data.next;
    out.next ??= walk;
  }
  return out;
};
const fake = (pages, calls) => async (tool, input) => { calls.push(input.cursor ?? 'first'); return served(structuredClone(pages[input.cursor ?? 'first'])); };
const collectionPaths = ['/data/files','/data/files/*/matches','/data/files/*/matches/*/sites','/data/rows','/data/rows/*/nodes','/data/symbols'];

test('tool response fragments reconstruct nested sites, text and mixed batch rows before tool pages', async () => {
  const calls = [];
  const pages = {
    first: response([row({ files: [{ path: 'a.ts', matches: [{ symbolName: 'target', line: 4, detail: 'שלום', sites: [{ line: 7 }] }] }] }, { part: 1, of: 3 })], 'p2'),
    p2: response([row({ files: [{ path: 'a.ts', matches: [{ symbolName: 'target', line: 4, sites: [{ line: 8 }] }] }] }, { part: 2, of: 3, continues: { array: '/data/files/0/matches', index: 0 } })], 'p3'),
    p3: response([
      row({ files: [{ path: 'a.ts', matches: [{ symbolName: 'target', line: 4, detail: '🙂', sites: [] }] }], next: { nextPage: next('tail') } }, { part: 3, of: 3, text: '/data/files/0/matches/0/detail' }),
      row({ files: [{ path: 'b.ts', matches: [{ symbolName: 'other', line: 2 }] }] }, null, 1),
    ]),
    tail: response([row({ files: [{ path: 'c.ts', matches: [{ symbolName: 'last', line: 1 }] }] })]),
  };
  const result = await pagedTool('lspSearch', { queries: [{}, {}] }, { run: fake(pages, calls), collectionPaths });
  assert.deepEqual(calls, ['first', 'p2', 'p3', 'tail']);
  const files = rowsOf(result, 'files');
  assert.deepEqual(files.map(f => f.path), ['a.ts', 'b.ts', 'c.ts']);
  assert.equal(files[0].matches[0].detail, 'שלום🙂');
  assert.deepEqual(files[0].matches[0].sites, [{ line: 7 }, { line: 8 }]);
});

test('an unfinished or out-of-order response fragment cannot claim completeness', async () => {
  for (const part of [{ part: 1, of: 2 }, { part: 2, of: 2 }]) {
    await assert.rejects(pagedTool('astSearch', {}, { run: async () => response([row({ symbols: [] }, part)]) }), /fragment|part/i);
  }
});

test('graph response fragments join one cycle before walking its member continuation', async () => {
  const calls = [];
  const pages = {
    first: response([row({ graph: { id: 'snapshot' }, rows: [{ id: 'cycle', nodes: ['a'], detail: 'first' }], pagination: { total: 1 } }, { part: 1, of: 2 })], 'fragment', 'graphQuery'),
    fragment: response([row({ graph: { id: 'snapshot' }, rows: [{ id: 'cycle', nodes: [], detail: ' tail' }], next: { cycle0: { tool: 'graphQuery', input: { cursor: 'members' } } } }, { part: 2, of: 2, text: '/data/rows/0/detail' })]),
    members: response([row({ graph: { id: 'snapshot' }, rows: ['b'], pagination: { total: 2 } })]),
  };
  const result = await readGraph({ operation: 'cycles' }, { run: fake(pages, calls), collectionPaths });
  assert.deepEqual(calls, ['first', 'fragment', 'members']);
  assert.deepEqual(result.rows, [{ id: 'cycle', nodes: ['a', 'b'], detail: 'first tail' }]);
});

test('a paused graph build follows the exact resume query until its snapshot is published', async () => {
  const resume = { queries: [{path:'src'}] }, calls = [];
  const run = async (_tool, input) => {
    calls.push(input);
    return response([row(calls.length < 3 ? {isPartial:true,progress:{committedFiles:calls.length},next:{expandScan:{tool:'graphBuild',input:resume}}} : {graph:'finished',root:'src'})]);
  };
  const result = await graphBuild({path:'src',refresh:true}, {run});
  assert.equal(result.graph, 'finished');
  assert.deepEqual(calls.slice(1), [resume,resume], 'resumable builds can repeat a query while making progress');
});

test('a paused graph build without its continuation is an explicit failure', async () => {
  await assert.rejects(graphBuild({path:'src'}, {run:async()=>response([row({isPartial:true,progress:{committedFiles:1}})])}), /continuation|snapshot/i);
});

test('nested fragments use joined indexes and escaped JSON pointer keys without duplicating siblings', async () => {
  const original = {files:[{path:'a.ts',matches:[{symbolName:'a'}]},{path:'b.ts',matches:[{symbolName:'b'},{symbolName:'c','detail/~':'שלום',sites:[{line:1}]}]}]};
  const pages = {
    first:response([row(original,{part:1,of:3})],'sites'),
    sites:response([row({files:[{path:'b.ts',matches:[{symbolName:'c',sites:[{line:2}]},{symbolName:'d'}]}]},{part:2,of:3,continues:{array:'/data/files/1/matches',index:1}})],'text'),
    text:response([row({files:[{path:'b.ts',matches:[{symbolName:'c','detail/~':'🙂',sites:[]}]}]},{part:3,of:3,text:'/data/files/1/matches/1/detail~1~0'})]),
  };
  const [page] = await pagedTool('lspSearch', {}, {run:fake(pages,[]), collectionPaths});
  assert.deepEqual(page.results[0].data.files, [{path:'a.ts',matches:[{symbolName:'a'}]},{path:'b.ts',matches:[{symbolName:'b'},{symbolName:'c','detail/~':'שלום🙂',sites:[{line:1},{line:2}]},{symbolName:'d'}]}]);
});

test('an impact summary count leads to its full list', async () => {
  const pages = {
    first: response([row({ rows: [], summary: { testsToRun: 2 }, next: { testsToRun: { tool: 'graphQuery', input: { cursor: 'tests' } } } })]),
    tests: response([row({ rows: ['a.test.ts', 'b.test.ts'] })]),
  };
  const result = await readGraph({ operation: 'impact' }, { run: fake(pages, []), collectionPaths });
  assert.deepEqual(result.summary.testsToRun, ['a.test.ts', 'b.test.ts']);
});

test('each witness page supplies the shared source before its rows stream', async () => {
  const pages = {
    first: response([row({ from: 'a.ts', rows: [{ to: 'b.ts' }] })], 'more', 'graphQuery'),
    more: response([row({ from: 'b.ts', rows: [{ to: 'c.ts' }] })]),
  };
  const streamed = [];
  const result = await readGraph({ operation: 'witnesses' }, {
    run: fake(pages, []), collectionPaths,
    onPage: data => streamed.push(...data.rows.map(edge => edge.from)),
  });
  assert.deepEqual(streamed, ['a.ts', 'b.ts']);
  assert.deepEqual(result.rows, [{ from: 'a.ts', to: 'b.ts' }, { from: 'b.ts', to: 'c.ts' }]);
});

test('a hoisted response walk retains every complete graph slice of one query', async () => {
  const pages = {
    first:response([row({graph:{id:'s'},rows:[{id:'a'}],pagination:{offset:0,length:1,total:2 }})],'tail','graphQuery'),
    tail:response([row({graph:{id:'s'},rows:[{id:'b'}],pagination:{offset:1,length:1,total:2 }})]),
  };
  assert.deepEqual((await readGraph({operation:'find'}, {run:fake(pages,[])})).rows, [{id:'a'},{id:'b'}]);
});

test('text fragments preserve repeated atomic provenance arrays exactly once', async () => {
  const origin = {unitId:'u',segments:[{sourceRange:{start:0,end:2},branches:[]}]};
  const pages = {
    first:response([row({rows:[{id:'a',detail:'first',sourceOrigin:origin}]},{part:1,of:2})],'tail','graphQuery'),
    tail:response([row({rows:[{id:'a',detail:' last',sourceOrigin:origin}]},{part:2,of:2,text:'/data/rows/0/detail'})]),
  };
  const [page] = await pagedTool('graphQuery',{}, {run:fake(pages,[]), collectionPaths});
  assert.deepEqual(page.results[0].data.rows[0].sourceOrigin,origin);
});

test('recursive schema collection references append nested evidence, including duplicate occurrences', async () => {
  const pages = {
    first:response([row({symbols:[{symbolName:'root',members:[{symbolName:'container',members:[{symbolName:'same'}]}]}]},{part:1,of:2})],'tail','astSearch'),
    tail:response([row({symbols:[{symbolName:'root',members:[{symbolName:'container',members:[{symbolName:'same'}]}]}]},{part:2,of:2,continues:{array:'/data/symbols/0/members',index:0}})]),
  };
  const [page] = await pagedTool('astSearch',{}, {run:fake(pages,[]),collectionPaths:['/data/symbols','/data/symbols/*/members'],collectionRefs:[{path:'/data/symbols/*/members/*',target:'/data/symbols/*'}]});
  assert.deepEqual(page.results[0].data.symbols[0].members[0].members,[{symbolName:'same'},{symbolName:'same'}]);
});

test('invalid text pointers and changed atomic context fail instead of losing evidence', async () => {
  for (const [detail,part] of [
    [{rows:[{detail:'tail'}]},{text:'/data/rows/0/missing'}],
    [{rows:[{detail:'tail',sourceOrigin:{segments:[{id:'changed'}]}}]},{text:'/data/rows/0/detail'}],
  ]) {
    const pages = {first:response([row({rows:[{detail:'first',sourceOrigin:{segments:[{id:'original'}]}}]},{part:1,of:2})],'tail','graphQuery'),tail:response([row(detail,{part:2,of:2,...part})])};
    await assert.rejects(pagedTool('graphQuery',{}, {run:fake(pages,[]),collectionPaths}), /string|context/i);
  }
});

test('successive fragmented tool pages may restart part numbers at the same query index', async () => {
  const pages = {
    first:response([row({rows:[{id:'a',detail:'one'}]},{part:1,of:2})],'a2','graphQuery'),
    a2:response([row({rows:[{id:'a',detail:'two'}]},{part:2,of:2,text:'/data/rows/0/detail'})],'b1','graphQuery'),
    b1:response([row({rows:[{id:'b',detail:'three'}]},{part:1,of:2})],'b2','graphQuery'),
    b2:response([row({rows:[{id:'b',detail:'four'}]},{part:2,of:2,text:'/data/rows/0/detail'})]),
  };
  const [page] = await pagedTool('graphQuery',{}, {run:fake(pages,[]),collectionPaths});
  assert.deepEqual(page.results.map(r=>r.data.rows[0]),[{id:'a',detail:'onetwo'},{id:'b',detail:'threefour'}]);
  pages.a2 = response([row({rows:[{id:'b',detail:'new'}]},{part:1,of:2})]);
  await assert.rejects(pagedTool('graphQuery',{}, {run:fake(pages,[]),collectionPaths}), /parts/i);
});

test('a hoisted build response selects its published receipt after progress slices', async () => {
  const pages = {first:response([row({isPartial:true,progress:{committedFiles:1}})],'finished','graphBuild'),finished:response([row({graph:'snapshot',root:'src'})])};
  assert.equal((await graphBuild({path:'src'},{run:fake(pages,[])})).graph,'snapshot');
});

test('a failed continuation cannot mix old rows with a restarted snapshot', async () => {
  const calls = [];
  const pages = {
    first:response([row({files:[{dir:'.',files:['old.ts']}],next:{nextPage:next('stale','structureSearch')}})]),
    stale:response([{index:0,status:'error',data:{errorCode:'staleSnapshot',error:'scope changed',next:{restart:{kind:'continue',tool:'structureSearch',input:{cursor:'restart'}}}}}]),
    restart:response([row({files:[{dir:'.',files:['new.ts']}]})]),
  };
  await assert.rejects(pagedTool('structureSearch',{}, {run:fake(pages,calls)}), /staleSnapshot/);
  assert.deepEqual(calls,['first','stale'],'a restart requires discarding the old walk first');
});

test('top-level tool failures reject before pages or recovery calls are emitted', async () => {
  let emitted = 0, called = 0;
  await assert.rejects(pagedTool('astSearch',{}, {run:async()=>{called++;return {errorCode:'executionFailed',error:'parser failed',next:{restart:next('restart','astSearch')}};},onPage:()=>emitted++}), /executionFailed/);
  assert.equal(emitted,0);
  assert.equal(called,1);
});

test('a failed envelope on a later cursor rejects the whole evidence walk', async () => {
  const calls = [];
  const pages = {first: response([row({ rows: [{ from: 'a', to: 'b' }] })], 'tail', 'graphQuery'),
    tail: {errorCode: 'staleSnapshot', error: 'snapshot changed'}};
  await assert.rejects(readGraph({operation:'witnesses'}, {run:fake(pages, calls)}), /staleSnapshot/);
  assert.deepEqual(calls, ['first', 'tail']);
});

test('a continued edge cannot silently change source or point to a missing collection', async () => {
  const base = {rows:[{from:'a',to:'b',sites:[1]}]};
  for (const [data, continues] of [
    [{rows:[{from:'c',to:'b',sites:[2]}]}, {array:'/data/rows', index:0}],
    [{rows:[{from:'a',to:'b',sites:[2]}]}, {array:'/data/rows/0/absent', index:0}],
  ]) {
    const pages = {first:response([row(base,{part:1,of:2})],'tail','graphQuery'),
      tail:response([row(data,{part:2,of:2,continues})])};
    await assert.rejects(readGraph({operation:'witnesses'}, {run:fake(pages,[]),collectionPaths:['/data/rows','/data/rows/*/sites']}), /fragment|context/i);
  }
});

test('a large collection fragment appends every occurrence without an argument-list limit', async () => {
  const items = Array.from({length:200000}, (_, i) => i % 71);
  const pages = {first:response([row({rows:[0]},{part:1,of:2})],'tail','graphQuery'),
    tail:response([row({rows:items},{part:2,of:2})])};
  const result = await readGraph({operation:'find'}, {run:fake(pages,[]),collectionPaths:['/data/rows']});
  assert.deepEqual(result.rows, [0,...items]);
});

test('a completed graph slice streams before the next cursor and an unfinished edge stays buffered', async () => {
  const emitted = [];
  const pages = { first:response([row({rows:[{id:'whole'}]}),row({rows:[{id:'split',sites:[1]}]},{part:1,of:2})],'tail','graphQuery'),
    tail:response([row({rows:[{id:'split',sites:[2]}]},{part:2,of:2,continues:{array:'/data/rows',index:0}})]) };
  const result = await readGraph({operation:'witnesses'}, {collectionPaths:['/data/rows','/data/rows/*/sites'],
    run:async (_, input) => {
      if (input.cursor) assert.deepEqual(emitted,[{id:'whole'}]);
      return pages[input.cursor ?? 'first'];
    }, onPage:data=>emitted.push(...structuredClone(data.rows))});
  assert.deepEqual(emitted,[{id:'whole'},{id:'split',sites:[1,2]}]);
  assert.deepEqual(result.rows,emitted);
});

test('graph cursor pages must retain the selected snapshot and the exact total', async () => {
  for (const [id,total] of [['replacement',2],['original',5]]) {
    const pages = {first:response([row({graph:{id:'original'},pagination:{total:2},rows:['a']})],'tail','graphQuery'),
      tail:response([row({graph:{id},pagination:{total},rows:['b']})])};
    await assert.rejects(readGraph({operation:'find'}, {run:fake(pages,[])}), /snapshot|total/i);
  }
});
