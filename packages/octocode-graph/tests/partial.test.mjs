import assert from 'node:assert/strict';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { stitch } from '../src/runtime/stitch.ts';
import { writeBrief } from '../src/runtime/render.ts';
import { tempDir } from './test-tmp.mjs';

test('query partial metadata survives section projection, stitching and both briefs', () => {
  const folder = tempDir('graph-partial-');
  const script = join(folder, 'fake-cli.mjs');
  writeFileSync(script, `
    if (process.argv[2] === '__graph') process.argv.splice(2, 1);
    const tool = process.argv[2];
    if (tool === 'config') { console.log(JSON.stringify({ key:'WORKSPACE_ROOT', value:null })); process.exit(0); }
    if (tool === 'schema') {
      console.log(JSON.stringify({querySchema:{properties:{target:{type:'array',maxItems:100}}}}));
    } else {
      const query = JSON.parse(process.argv.at(-1)).queries[0];
      if (tool === 'graphQuery') {
        const { appendFileSync } = await import('node:fs');
        appendFileSync(${JSON.stringify(join(folder, 'queries'))}, query.operation + '\\n');
      }
      const data = tool === 'graphBuild'
        ? {graph:'snapshot',root:query.path,counts:{nodes:1,edges:0},linkage:{ratio:null}}
        : {graph:{id:'snapshot',root:query.path,isPartial:true,partialReasons:['filesSkipped']},rows:query.operation === 'find' ? [{id:'a.ts',kind:'file',language:'typescript'}] : []};
      console.log(JSON.stringify({results:[{index:0,data}]}));
    }
  `);
  const source = `
    import { buildSection } from ${JSON.stringify(new URL('../src/runtime/section.ts', import.meta.url).href)};
    const data = await buildSection(${JSON.stringify(folder)}, {out:${JSON.stringify(join(folder, 'section'))},jobs:1});
    console.log(JSON.stringify(data));
  `;
  const built = spawnSync(process.execPath, ['--input-type=module', '-e', source], {
    encoding: 'utf8', env: { ...process.env, OCTOCODE_CLI: `${process.execPath} ${script}` },
  });
  assert.equal(built.status, 0, built.stderr);
  const section = JSON.parse(built.stdout);
  const operations = readFileSync(join(folder, 'queries'), 'utf8').trim().split('\n');
  for (const operation of ['witnesses', 'symbols']) {
    assert.equal(operations.filter((op) => op === operation).length, 1, 'a complete single page needs one walk');
  }
  assert.equal(section.snapshot.isPartial, true);
  assert.deepEqual(section.snapshot.partialReasons, ['filesSkipped']);
  assert.equal(section.coverage.isPartial, true);
  assert.deepEqual(section.coverage.partialReasons, ['filesSkipped']);
  writeBrief(section, join(folder, 'section'));
  assert.match(readFileSync(join(folder, 'section', 'brief.txt'), 'utf8'), /COVERAGE PARTIAL: filesSkipped/);
  const { whole } = stitch({
    root: folder, chunks: [{ id: 'src', path: 'src' }, { id: 'missing', path: 'missing' }],
    datas: new Map([['src', section]]), files: ['src/a.ts'],
  });
  assert.equal(whole.snapshot.isPartial, true);
  assert.equal(whole.coverage.isPartial, true);
  assert.deepEqual(whole.coverage.notBuilt, ['missing']);
  assert.ok(whole.coverage.partialReasons.includes('filesSkipped'));
  assert.ok(whole.coverage.partialReasons.includes('chunksNotBuilt'));
  assert.deepEqual(whole.coverage.partialChunks, [{ id: 'src', path: 'src', partialReasons: ['filesSkipped'] }]);
  writeBrief(whole, folder);
  assert.match(readFileSync(join(folder, 'brief.txt'), 'utf8'), /symbols in the call graph/);
  const brief = readFileSync(join(folder, 'brief.txt'), 'utf8');
  assert.match(brief, /COVERAGE PARTIAL/);
  assert.match(brief, /filesSkipped/);
  assert.match(brief, /missing/);
  assert.equal(existsSync(join(folder, 'brief-full.txt')), false, 'one brief');
});

test('complete chunks keep complete coverage and briefs', () => {
  const folder = tempDir('graph-complete-');
  const section = { files: [{ id: 'a.ts', group: '.', fanIn: 0, fanOut: 0 }], imports: [], calls: [], symbols: [], snapshot: { id: 'complete' }, coverage: {} };
  const { whole } = stitch({ root: folder, chunks: [{ id: 'src', path: 'src' }], datas: new Map([['src', section]]), files: ['src/a.ts'] });
  assert.equal(whole.coverage.isPartial, false);
  assert.deepEqual(whole.coverage.partialReasons, []);
  writeBrief(whole, folder);
  assert.doesNotMatch(readFileSync(join(folder, 'brief.txt'), 'utf8'), /COVERAGE PARTIAL/);
});

test('missing chunks alone mark the stitched analysis partial', () => {
  const section = { files: [], imports: [], calls: [], symbols: [] };
  const { whole } = stitch({ root: '/repo', chunks: [{ id: 'built', path: 'built' }, { id: 'missing', path: 'missing' }], datas: new Map([['built', section]]), files: [] });
  assert.equal(whole.coverage.isPartial, true);
  assert.deepEqual(whole.coverage.partialReasons, ['chunksNotBuilt']);
  assert.deepEqual(whole.coverage.partialChunks, []);
  assert.deepEqual(whole.coverage.notBuilt, ['missing']);
});

test('either snapshot or coverage partial metadata survives stitching without missing chunks', () => {
  for (const key of ['snapshot', 'coverage']) {
    const section = { files: [], imports: [], calls: [], symbols: [], [key]: { isPartial: true, partialReasons: ['filesSkipped'] } };
    const { whole } = stitch({ root: '/repo', chunks: [{ id: 'src', path: 'src' }], datas: new Map([['src', section]]), files: [] });
    assert.equal(whole.coverage.isPartial, true);
    assert.equal(whole.snapshot.isPartial, true);
    assert.deepEqual(whole.coverage.partialReasons, ['filesSkipped']);
    assert.deepEqual(whole.coverage.notBuilt, []);
  }
});

test('call gaps are read from every file of a chunk, so an inline crate path with no `use` can be stitched', () => {
  const folder = tempDir('graph-gaps-');
  const script = join(folder, 'fake-cli.mjs');
  // run.rs imports past the chunk root; inline.rs names another crate only in a call path
  // (`other_crate::config::read(..)`): nothing imports it, yet bindCalls binds such a crate path.
  writeFileSync(script, `
    if (process.argv[2] === '__graph') process.argv.splice(2, 1);
    const tool = process.argv[2];
    if (tool === 'config') { console.log(JSON.stringify({ key:'WORKSPACE_ROOT', value:null })); process.exit(0); }
    if (tool === 'schema') {
      console.log(JSON.stringify({querySchema:{properties:{target:{type:'array',maxItems:100}}}}));
    } else {
      const query = JSON.parse(process.argv.at(-1)).queries[0];
      if (query.operation === 'unresolved') {
        const { appendFileSync } = await import('node:fs');
        appendFileSync(${JSON.stringify(join(folder, 'targets'))}, JSON.stringify(query.target) + '\\n');
      }
      const rows = {
        find: [{id:'src/run.rs',kind:'file',language:'rust'},{id:'src/inline.rs',kind:'file',language:'rust'}],
        diagnostics: [{code:'outside-root',path:'src/run.rs',line:1,message:'"other_crate::memo"'}],
      }[query.operation] ?? [];
      const data = tool === 'graphBuild'
        ? {graph:'snapshot',root:query.path,counts:{nodes:2,edges:0},linkage:{ratio:null}}
        : {graph:{id:'snapshot',root:query.path},rows};
      console.log(JSON.stringify({results:[{index:0,data}]}));
    }
  `);
  const source = `
    import { buildSection } from ${JSON.stringify(new URL('../src/runtime/section.ts', import.meta.url).href)};
    await buildSection(${JSON.stringify(folder)}, {out:${JSON.stringify(join(folder, 'section'))},jobs:1,crossPackages:new Set(['other_crate']),maxTargets:100});
  `;
  const built = spawnSync(process.execPath, ['--input-type=module', '-e', source], {
    encoding: 'utf8', env: { ...process.env, OCTOCODE_CLI: `${process.execPath} ${script}` },
  });
  assert.equal(built.status, 0, built.stderr);
  const targets = readFileSync(join(folder, 'targets'), 'utf8').trim().split('\n').flatMap((l) => JSON.parse(l));
  assert.deepEqual(targets.map((t) => t.split('/').slice(-2).join('/')).sort(), ['src/inline.rs', 'src/run.rs']);
});
