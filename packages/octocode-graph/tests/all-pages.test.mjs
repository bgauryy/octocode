import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { AllPagesInputError, readAllPages, replyExitCode } from '../src/runtime/all-pages.ts';
import { RESPONSE_CHARS } from '../src/runtime/graph-pages.ts';
import { tempDir } from './test-tmp.mjs';

const more = (cursor) => ({ kind: 'continue', tool: 'graphQuery', input: { cursor } });
// A fake engine: the fresh call answers with `first`, each cursor with its page; every call is recorded.
const fake = (first, pages, calls = []) => async (tool, input) => {
  calls.push(input);
  return structuredClone(input.cursor ? pages[input.cursor] : first);
};
const rowsOf = (data) => data.rows;

test('--all-pages joins every page, row fragment and evidence list once, and keeps every page warning', async () => {
  const queries = [
    { path: 'src', operation: 'impact', target: 'src/a.ts' },
    { path: 'src', operation: 'cycles' },
  ];
  // Page 1: impact's first rows with counts its next lists, cycles' first part (its first cycle cut short).
  const first = {
    warnings: ['page one'],
    results: [
      { index: 0, data: { graph: { id: 'g' }, pagination: { total: 3 }, rows: ['a', 'b'], summary: { testsToRun: 2 }, next: { testsToRun: more('tests') } } },
      { index: 1, data: { graph: { id: 'g' }, rows: [{ nodes: ['x', 'y'] }] }, rowPart: { part: 1, of: 2 } },
    ],
    next: { tool: 'graphQuery', input: { cursor: 'p2' } },
  };
  const pages = {
    // Page 2: impact's last row and the rest of cycles' first cycle, then its second cycle.
    p2: {
      warnings: ['page one', 'page two'],
      results: [
        { index: 1, data: { graph: { id: 'g' }, rows: [{ nodes: ['z'] }, { nodes: ['p', 'q'] }] }, rowPart: { part: 2, of: 2, continues: { array: '/data/rows', index: 0 } } },
        { index: 0, data: { graph: { id: 'g' }, pagination: { total: 3 }, rows: ['c'], warnings: ['row page two'] } },
      ],
    },
    tests: { results: [{ index: 0, data: { graph: { id: 'g' }, rows: [{ path: 't1' }, { path: 't2' }] } }] },
  };
  const calls = [];
  const reply = await readAllPages({ queries }, { run: fake(first, pages, calls), collectionPaths: ['/data/rows', '/data/rows/*/nodes'] });
  assert.deepEqual(calls, [{ queries, responseChars: RESPONSE_CHARS }, { cursor: 'p2' }, { cursor: 'tests' }]);
  assert.deepEqual(reply.results.map((r) => r.index), [0, 1]);
  assert.deepEqual(rowsOf(reply.results[0].data), ['a', 'b', 'c']);
  assert.deepEqual(reply.results[0].data.summary.testsToRun, [{ path: 't1' }, { path: 't2' }]);
  assert.deepEqual(reply.results[0].data.warnings, ['row page two'], 'a later page’s row warning is kept');
  assert.deepEqual(reply.results[1].data.rows.map((c) => c.nodes), [['x', 'y', 'z'], ['p', 'q']]);
  assert.deepEqual(reply.warnings, ['page one', 'page two'], 'every page warning once');
  for (const r of reply.results) {
    assert.equal(r.data.next, undefined);
    assert.equal(r.data.pagination, undefined);
    assert.equal(r.rowPart, undefined);
  }
  assert.equal(reply.next, undefined);
  assert.equal(replyExitCode(reply), 0);
});

test('--all-pages keeps paths as the tool states them, an error row whole and an answer without rows', async () => {
  const first = {
    results: [
      { index: 0, status: 'error', data: { errorCode: 'notFound', errorClass: 'correctInput', error: 'no snapshot', related: [{ tool: 'graphBuild', input: { queries: [{ path: 'pkg' }] } }] } },
      { index: 1, data: { graph: { id: 'g' }, counts: { files: 2 }, related: [] } },
      { index: 2, data: { graph: { id: 'g' }, rows: [{ id: 'packages/cli/src/a.ts', path: 'packages/cli/src/a.ts' }] } },
    ],
  };
  const queries = [{ path: 'pkg', operation: 'stats' }, { path: 'packages/cli', operation: 'stats' }, { path: 'packages/cli', operation: 'find' }];
  const reply = await readAllPages({ queries, responseChars: 500 }, { run: fake(first, {}) });
  assert.equal(reply.results[0].status, 'error');
  assert.equal(reply.results[0].data.related[0].tool, 'graphBuild', 'the recovery call is kept');
  assert.deepEqual(reply.results[1].data, first.results[1].data);
  assert.equal(reply.results[2].data.rows[0].path, 'packages/cli/src/a.ts', 'no snapshot-key restatement');
  assert.equal(replyExitCode(reply), 2);
});

test('--all-pages exit codes follow the CLI rule: strictest failed class, all empty is 1', () => {
  const row = (status, errorClass) => ({ index: 0, ...(status && { status }), data: errorClass ? { errorCode: 'x', errorClass } : { rows: [] } });
  assert.equal(replyExitCode({ results: [row('empty'), row('outOfRange')] }), 1);
  assert.equal(replyExitCode({ results: [row('empty'), row()] }), 0);
  assert.equal(replyExitCode({ results: [row('error', 'transient'), row('error', 'refused')] }), 4);
  assert.equal(replyExitCode({ results: [row('error', 'transient')] }), 7);
  assert.equal(replyExitCode({ results: [row('error', 'restart'), row('error', 'unservable')] }), 6);
  assert.equal(replyExitCode({ results: [row('error', 'unknown')] }), 5);
});

test('--all-pages refuses a cursor, extra envelope fields and an incomplete walk', async () => {
  await assert.rejects(readAllPages({ cursor: 'c' }), AllPagesInputError);
  await assert.rejects(readAllPages({ queries: [{}], other: 1 }), /other/);
  await assert.rejects(readAllPages({ queries: [{}], responseChars: 0 }), /responseChars/);
  // A list whose stated total is not reached is incomplete: no reply instead of a short one.
  const short = { results: [{ index: 0, data: { graph: { id: 'g' }, pagination: { total: 2 }, rows: [1] } }] };
  await assert.rejects(readAllPages({ queries: [{ operation: 'hubs' }] }, { run: fake(short, {}) }), /required continuation/);
  // A failed evidence list fails the walk with its class.
  const first = { results: [{ index: 0, data: { graph: { id: 'g' }, rows: [], summary: { testsToRun: 1 }, next: { testsToRun: more('t') } } }] };
  const failed = { results: [{ index: 0, status: 'error', data: { errorCode: 'staleSnapshot', errorClass: 'restart', error: 'changed' } }] };
  await assert.rejects(readAllPages({ queries: [{ operation: 'impact' }] }, { run: fake(first, { t: failed }) }), (error) => error.errorClass === 'restart' && /testsToRun/.test(error.message));
});

test('the CLI flag reads its envelope from an argument or --input and rejects a cursor with exit 2', () => {
  const bin = new URL('../bin/octocode-graph.mjs', import.meta.url).pathname;
  const dir = tempDir('graph-all-pages-');
  const run = (args) => spawnSync(process.execPath, [bin, 'graphQuery', ...args], { encoding: 'utf8' });
  const cursor = run(['{"cursor":"c"}', '--all-pages']);
  assert.equal(cursor.status, 2);
  assert.match(cursor.stderr, /fresh \{"queries"/);
  const file = join(dir, 'q.json');
  writeFileSync(file, '{"cursor":"c"}');
  assert.equal(run(['--input', file, '--all-pages']).status, 2);
  assert.match(run(['{"queries":[]}', '--all-pages', '--nope']).stderr, /unknown option --nope/);
  assert.match(run(['not json', '--all-pages']).stderr, /does not parse/);
});
