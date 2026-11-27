import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { collectionRoute, walk } from './walk.mjs';

const SCRIPT = path.join(path.dirname(fileURLToPath(import.meta.url)), 'walk.mjs');
const SCHEMA = { collectionPaths: ['/data/files', '/data/files/*/matches'] };
const FIRST = { queries: [{ matchString: ['x'] }] };

/** A CLI stand-in: replies keyed by the input each call sends; `calls` records what was sent. */
function server(replies) {
  const calls = [];
  const run = ([tool, input]) => {
    calls.push([tool, JSON.parse(input)]);
    const reply = replies[input];
    return reply === undefined ? { status: 2, stdout: JSON.stringify({ error: `no page for ${input}`, errorCode: 'invalidInput' }) } : { status: 0, stdout: JSON.stringify(reply) };
  };
  return { run, calls };
}
const next = cursor => ({ tool: 'localSearch', input: { cursor } });
const key = input => JSON.stringify(input);
const codes = result => result.violations.map(item => item.code);

test('collection paths match one step per *, and a reference reduces to its target', () => {
  const route = collectionRoute({ collectionPaths: ['/data/files', '/data/files/*/matches'], collectionRefs: [{ path: '/data/files/*/files', target: '/data/files' }] });
  assert.equal(route('/data/files'), true);
  assert.equal(route('/data/files/3/matches'), true);
  assert.equal(route('/data/files/3/files/1/matches'), true);
  assert.equal(route('/data/skippedPaths'), false);
});

test('a walk joins parts at their continued element and text, applies shared facts, and passes', () => {
  const { run, calls } = server({
    [key(FIRST)]: {
      shared: { data: { commitSha: 'abc' } },
      results: [{ index: 0, rowPart: { part: 1, of: 3 }, data: { pagination: { unit: 'lines', offset: 0, length: 3 }, files: [{ path: 'a', matches: [{ line: 1 }] }, { path: 'b', matches: [{ line: 1, value: 'lo' }] }] } }],
      next: next('c1'),
    },
    [key({ cursor: 'c1' })]: {
      results: [{ index: 0, rowPart: { part: 2, of: 3, text: '/data/files/1/matches/0/value' }, data: { commitSha: 'abc', pagination: { unit: 'lines', offset: 0, length: 3 }, files: [{ path: 'b', matches: [{ line: 1, value: 'ng' }] }] } }],
      next: next('c2'),
    },
    [key({ cursor: 'c2' })]: {
      results: [{ index: 0, rowPart: { part: 3, of: 3, continues: { array: '/data/files', index: 1 } }, data: { commitSha: 'abc', pagination: { unit: 'lines', offset: 0, length: 3 }, files: [{ path: 'b', matches: [{ line: 2 }] }, { path: 'c', matches: [{ line: 9 }] }] } }],
    },
  });
  const result = walk({ tool: 'localSearch', input: FIRST, run, schema: SCHEMA });
  assert.deepEqual(result.violations, []);
  assert.equal(result.ok, true);
  assert.equal(result.pages, 3);
  assert.deepEqual(calls.map(([, input]) => input), [FIRST, { cursor: 'c1' }, { cursor: 'c2' }], 'each next.input is sent unchanged');
  assert.deepEqual(result.rows, [{ index: 0, slices: 1, parts: 3, collections: { '/data/files': 3, '/data/files/*/matches': 4 } }]);
  assert.deepEqual(result.results[0].data.files, [
    { path: 'a', matches: [{ line: 1 }] },
    { path: 'b', matches: [{ line: 1, value: 'long' }, { line: 2 }] },
    { path: 'c', matches: [{ line: 9 }] },
  ]);
  assert.equal(result.results[0].data.commitSha, 'abc');
});

test('a later row page is its own slice, and its text window must start where the last ended', () => {
  const page = (offset, length, total, nextCursor) => ({ results: [{ index: 0, data: { pagination: { unit: 'lines', offset, length, ...(total && { total }) }, files: [{ path: `f${offset}`, matches: [{ line: offset }] }] } }], ...(nextCursor && { next: next(nextCursor) }) });
  const tiled = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({ [key(FIRST)]: page(0, 5, 0, 'p2'), [key({ cursor: 'p2' })]: page(5, 2, 7) }).run });
  assert.deepEqual(codes(tiled), []);
  assert.equal(tiled.rows[0].slices, 2);

  const gap = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({ [key(FIRST)]: page(0, 5, 0, 'p2'), [key({ cursor: 'p2' })]: page(6, 2, 8) }).run });
  assert.deepEqual(codes(gap), ['paginationGap']);
  const overlap = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({ [key(FIRST)]: page(0, 5, 0, 'p2'), [key({ cursor: 'p2' })]: page(4, 3, 7) }).run });
  assert.deepEqual(codes(overlap), ['paginationOverlap']);
  const short = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({ [key(FIRST)]: page(0, 5, 9) }).run });
  assert.deepEqual(codes(short), ['paginationShort']);
});

test('a repeated cursor, an out-of-order part and an unfinished row are violations', () => {
  const part = (n, of) => ({ index: 0, rowPart: { part: n, of }, data: { files: [{ path: `p${n}`, matches: [] }] } });
  const loop = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({ [key(FIRST)]: { results: [part(1, 3)], next: next('a') }, [key({ cursor: 'a' })]: { results: [part(2, 3)], next: next('a') } }).run });
  assert.deepEqual(codes(loop), ['repeatedCursor', 'unfinishedRow']);

  const skipped = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({ [key(FIRST)]: { results: [part(1, 3)], next: next('a') }, [key({ cursor: 'a' })]: { results: [part(3, 3)] } }).run });
  assert.deepEqual(codes(skipped), ['partOrder']);
});

test('the same page twice, repeated evidence and a changed repeated field are violations', () => {
  const same = { results: [{ index: 0, data: { files: [] } }] };
  const twice = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({ [key(FIRST)]: { ...same, next: next('a') }, [key({ cursor: 'a' })]: same }).run });
  assert.deepEqual(codes(twice), ['repeatedPage']);

  const repeated = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({ [key(FIRST)]: { results: [{ index: 0, data: { files: [{ path: 'a', matches: [{ line: 1 }, { line: 1 }] }] } }] } }).run });
  assert.deepEqual(codes(repeated), ['repeatedEvidence']);

  const changed = walk({
    tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({
      [key(FIRST)]: { results: [{ index: 0, rowPart: { part: 1, of: 2 }, data: { filesScanned: 3, files: [] } }], next: next('a') },
      [key({ cursor: 'a' })]: { results: [{ index: 0, rowPart: { part: 2, of: 2 }, data: { filesScanned: 4, files: [] } }] },
    }).run,
  });
  assert.deepEqual(codes(changed), ['contextDisagrees']);
});

test('every query index needs a row; partial and failed rows are disclosed, not violations', () => {
  const two = { queries: [{ matchString: ['x'] }, { matchString: ['y'] }] };
  const missing = walk({ tool: 'localSearch', input: two, schema: SCHEMA, run: server({ [key(two)]: { results: [{ index: 0, data: { isPartial: true, partialReasons: ['scanBudget'], files: [] } }] } }).run });
  assert.deepEqual(codes(missing), ['missingIndex']);
  assert.deepEqual(missing.partial, [{ index: 0, slice: 0, isPartial: true, partialReasons: ['scanBudget'], terminalLimit: undefined }]);

  const failed = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run: server({ [key(FIRST)]: { results: [{ index: 0, data: { errorCode: 'pathNotFound', error: 'no such path' } }] } }).run });
  assert.equal(failed.ok, true);
  assert.deepEqual(failed.rowErrors, [{ index: 0, slice: 0, errorCode: 'pathNotFound', error: 'no such path' }]);
});

test('--max-pages stops a walk that has not ended and names the pending call', () => {
  const { run } = server({ [key(FIRST)]: { results: [{ index: 0, data: { files: [] } }], next: next('a') } });
  const result = walk({ tool: 'localSearch', input: FIRST, schema: SCHEMA, run, maxPages: 1 });
  assert.deepEqual(codes(result), ['notTerminated']);
  assert.match(result.violations[0].message, /"cursor":"a"/);
});

test('the command exits 2 on a refused first call or bad arguments and 1 on a violation', { skip: process.platform === 'win32' }, t => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-walk-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const bin = path.join(dir, 'octocode');
  // Refuses any request whose matchString is "bad"; otherwise loops on one cursor.
  fs.writeFileSync(bin, `#!/usr/bin/env node
const [tool, input] = process.argv.slice(2);
if (tool === 'schema') { console.log(JSON.stringify(${JSON.stringify(SCHEMA)})); process.exit(0); }
if (input.includes('"bad"')) { console.log(JSON.stringify({ error: 'unknown field', errorCode: 'invalidInput', errorClass: 'correctInput' })); process.exit(2); }
console.log(JSON.stringify({ results: [{ index: 0, data: { files: [{ path: input, matches: [] }] } }], next: { tool: 'localSearch', input: { cursor: 'same' } } }));
`, { mode: 0o755 });
  const call = (...args) => spawnSync(process.execPath, [SCRIPT, ...args], { encoding: 'utf8' });
  const refused = call('localSearch', '{"queries":[{"matchString":["bad"]}]}', '--bin', bin);
  assert.equal(refused.status, 2);
  assert.equal(JSON.parse(refused.stdout).error.code, 'invalidInput');
  assert.equal(call('localSearch', 'not json').status, 2);
  assert.equal(call('localSearch', '{}', '--max-pages', '0').status, 2);
  const looped = call('localSearch', '{"queries":[{"matchString":["x"]}]}', '--bin', bin);
  assert.equal(looped.status, 1);
  assert.deepEqual(JSON.parse(looped.stdout).violations.map(item => item.code), ['repeatedCursor']);
});
