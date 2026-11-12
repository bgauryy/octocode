import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const runner = fileURLToPath(new URL('./graph-bench.mjs', import.meta.url));
const comparer = fileURLToPath(new URL('./bench-compare.mjs', import.meta.url));
const fixtureCli = `
const [tool, , raw] = process.argv.slice(2);
const input = JSON.parse(raw);
const operation = input.queries?.[0]?.operation;
let data;
if (tool === 'graphBuild') data = { id: 'fixture' };
else if (input.cursor) {
  data = { error: 'later callers page failed', errorCode: 'executionError' };
  process.exitCode = 5;
} else if (operation === 'find') {
  data = { rows: [{ id: 'src/lib.rs#poll_proceed', path: 'src/lib.rs', selectionByteRange: { startByte: 3 } }], pagination: { total: 1 } };
} else if (operation === 'callers') {
  const fail = process.env.GRAPH_BENCH_TEST_FAILURE === 'page';
  data = { rows: [{ path: 'src/use.rs', edges: 1 }], pagination: { total: fail ? 2 : 1 } };
  if (fail) data.next = { nextPage: { kind: 'continue', tool: 'graphQuery', input: { cursor: 'next' } } };
} else data = { rows: [], pagination: { total: 0 } };
console.log(JSON.stringify({ results: [{ index: 0, data }] }));
`;

test('benchmark commands distinguish complete, failed and empty measurements', async t => {
  const root = mkdtempSync(join(tmpdir(), 'graph-bench-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const repos = join(root, 'repos');
  const source = join(repos, 'rust/tokio/src');
  mkdirSync(source, { recursive: true });
  writeFileSync(join(source, 'lib.rs'), 'fn poll_proceed() {}\n');
  writeFileSync(join(source, 'use.rs'), 'fn caller() { poll_proceed(); }\n');
  const cli = join(root, 'fixture-cli.js');
  writeFileSync(cli, fixtureCli);
  const run = (name, env = {}) => {
    const out = join(root, `${name}.json`);
    const result = spawnSync(process.execPath, [runner, '--bin', cli, '--repos', repos, '--only', 'rust', '--jobs', '1', '--out', out], {
      encoding: 'utf8', env: { ...process.env, GRAPH_BENCH_TEST_FAILURE: '', ...env },
    });
    return { ...result, out, row: JSON.parse(readFileSync(out, 'utf8'))[0] };
  };
  const baseline = run('baseline');
  assert.equal(baseline.status, 0, baseline.stderr);
  assert.equal(baseline.row.recall.precision, 1);
  const compare = (before, after) => spawnSync(process.execPath, [comparer, before.out, after.out, '--json'], { encoding: 'utf8' });

  await t.test('failed continuation refuses comparison in both directions', () => {
    const failed = run('failed-page', { GRAPH_BENCH_TEST_FAILURE: 'page' });
    assert.equal(failed.status, 1, failed.stderr);
    assert.equal(failed.row.callers.pagesComplete, false);
    assert.equal(failed.row.recall.precision, baseline.row.recall.precision);
    for (const result of [compare(baseline, failed), compare(failed, baseline)]) {
      assert.equal(result.status, 2, result.stdout);
      const [row] = JSON.parse(result.stdout);
      assert.equal(row.invalid, true);
      assert.match(JSON.stringify(row.failures), /later callers page failed/);
    }
  });

  await t.test('missing oracle fails without manufacturing ratios', () => {
    const bin = join(root, 'bin');
    mkdirSync(bin);
    symlinkSync(process.execPath, join(bin, basename(process.execPath)));
    const failed = run('missing-rg', { PATH: bin });
    assert.equal(failed.status, 1, failed.stderr);
    assert.equal(failed.row.recall.oracleExit, -1);
    assert.match(failed.row.recall.error, /ENOENT/);
    assert.equal(failed.row.recall.precision, undefined);
    assert.equal(failed.row.recall.recall, undefined);
    assert.equal(compare(baseline, failed).status, 2);
  });

  await t.test('an oracle with no matches remains a successful empty search', () => {
    writeFileSync(join(source, 'lib.rs'), 'fn unrelated() {}\n');
    writeFileSync(join(source, 'use.rs'), 'fn caller() {}\n');
    const empty = run('empty-oracle');
    assert.equal(empty.status, 0, empty.stderr);
    assert.equal(empty.row.recall.oracleExit, 1);
    assert.equal(empty.row.recall.rgFiles, 0);
    assert.equal(empty.row.recall.recall, null);
  });
});

test('rows the tool states from the workspace are read in the oracle frame', async () => {
  const { callCounts, rootRelative } = await import('./graph-bench.mjs');
  assert.equal(rootRelative('src', 'src/object.c'), 'object.c');
  assert.equal(rootRelative('.', 'src/object.c'), 'src/object.c');
  assert.equal(rootRelative('src', 'srcx/a.c'), 'srcx/a.c');
  assert.deepEqual(callCounts({ calls: { sites: 4, unresolvedByReason: { other: 1 } }, linkage: { linked: 3 } }), {
    callSites: 4,
    callLinkRate: 0.75,
    unresolvedByReason: { other: 1 },
  });
});
