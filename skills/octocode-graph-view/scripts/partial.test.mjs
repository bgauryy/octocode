import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { stitch } from './stitch.mjs';
import { writeBriefs } from './render.mjs';
import { tempDir } from './test-tmp.mjs';

test('query partial metadata survives section projection, stitching and both briefs', () => {
  const folder = tempDir('graph-partial-');
  const script = join(folder, 'fake-cli.mjs');
  writeFileSync(script, `
    const tool = process.argv[2];
    if (tool === 'schema') {
      console.log(JSON.stringify({querySchema:{properties:{target:{type:'array',maxItems:100}}}}));
    } else {
      const query = JSON.parse(process.argv.at(-1)).queries[0];
      const data = tool === 'graphBuild'
        ? {id:'snapshot',root:query.path,counts:{nodes:1,edges:0},linkage:{ratio:null}}
        : {graph:{id:'snapshot',root:query.path,isPartial:true,partialReasons:['filesSkipped']},rows:query.operation === 'find' ? [{id:'a.ts',kind:'file',language:'typescript'}] : []};
      console.log(JSON.stringify({results:[{index:0,data}]}));
    }
  `);
  const source = `
    import { buildSection } from ${JSON.stringify(new URL('./section.mjs', import.meta.url).href)};
    const data = await buildSection(${JSON.stringify(folder)}, {out:${JSON.stringify(join(folder, 'section'))},jobs:1});
    console.log(JSON.stringify(data));
  `;
  const built = spawnSync(process.execPath, ['--input-type=module', '-e', source], {
    encoding: 'utf8', env: { ...process.env, OCTOCODE_CLI: `${process.execPath} ${script}` },
  });
  assert.equal(built.status, 0, built.stderr);
  const section = JSON.parse(built.stdout);
  assert.equal(section.snapshot.isPartial, true);
  assert.deepEqual(section.snapshot.partialReasons, ['filesSkipped']);
  assert.equal(section.coverage.isPartial, true);
  assert.deepEqual(section.coverage.partialReasons, ['filesSkipped']);
  writeBriefs(section, join(folder, 'section'));
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
  writeBriefs(whole, folder);
  for (const name of ['brief.txt', 'brief-full.txt']) {
    const brief = readFileSync(join(folder, name), 'utf8');
    assert.match(brief, /COVERAGE PARTIAL/);
    assert.match(brief, /filesSkipped/);
    assert.match(brief, /missing/);
  }
});

test('complete chunks keep complete coverage and briefs', () => {
  const folder = tempDir('graph-complete-');
  const section = { files: [{ id: 'a.ts', group: '.', fanIn: 0, fanOut: 0 }], imports: [], calls: [], symbols: [], snapshot: { id: 'complete' }, coverage: {} };
  const { whole } = stitch({ root: folder, chunks: [{ id: 'src', path: 'src' }], datas: new Map([['src', section]]), files: ['src/a.ts'] });
  assert.equal(whole.coverage.isPartial, false);
  assert.deepEqual(whole.coverage.partialReasons, []);
  writeBriefs(whole, folder);
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
