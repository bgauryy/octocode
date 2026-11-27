import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { node, withDir } from './helpers.mjs';

const session = (dir, sources, failures = '# Failures\n\nNo fetch failure recorded.\n') => {
  for (const folder of ['text', 'reports', 'cdp', 'snippets']) mkdirSync(join(dir, folder), { recursive: true });
  writeFileSync(join(dir, 'AGENT_INDEX.json'), JSON.stringify({ sessionId: 's', warnings: [] }));
  writeFileSync(join(dir, 'sources.jsonl'), sources.map((row) => JSON.stringify(row)).join('\n'));
  writeFileSync(join(dir, 'reports/failures.md'), failures);
};

test('coverage-md prints one table row per source and per refused or robots-skipped URL', () => withDir((dir) => {
  session(dir, [
    { pageId: 'page-001', url: 'https://ex.test/a', status: 200, ok: true, textParts: ['text/page-001.clean.part-001.md', 'text/page-001.clean.part-002.md'] },
    { pageId: 'page-002', url: 'https://ex.test/b?token=abcdef123456', status: 404, ok: false, textParts: [], raw: 'raw/page-002.html', targetLikelyError: 'not found | gone' },
    { pageId: 'page-003', url: 'https://ex.test/c', status: 200, ok: true, textParts: ['text/page-003.clean.part-001.md'], browserRecommended: true, browserReason: 'application shell', textTruncated: true },
  ], '# Failures\n\n- refused http://10.0.0.1/x: private host\n- robots.txt skipped https://ex.test/private: disallowed by robots.txt\n- page-002 status 404: not found\n');
  const res = node('corpus-inspect.mjs', ['--session-dir', dir, '--format', 'coverage-md']);
  assert.equal(res.status, 0, res.stdout + res.stderr);
  const lines = res.stdout.trim().split('\n');
  assert.equal(lines[0], '3 fetched (2 ok, 1 failed); 2 not fetched (refused or robots).');
  assert.equal(lines[2], '| URL | Status | Session file | Notes |');
  const rows = lines.slice(4);
  assert.equal(rows.length, 5);
  assert.equal(rows[0], '| https://ex.test/a | 200 | text/page-001.clean.part-001.md, text/page-001.clean.part-002.md |  |');
  assert.ok(rows[1].includes('404 failed') && rows[1].includes('raw/page-002.html') && rows[1].includes('not found \\| gone'), rows[1]);
  assert.ok(!res.stdout.includes('abcdef123456'), 'URL credentials are redacted');
  assert.ok(rows[2].includes('browser recommended: application shell') && rows[2].includes('text truncated'), rows[2]);
  assert.ok(rows[3].startsWith('| http://10.0.0.1/x | refused |'), rows[3]);
  assert.ok(rows[4].startsWith('| https://ex.test/private | robots skipped |'), rows[4]);
  // Every row has the four columns once escaped pipes are set aside.
  for (const row of rows) assert.equal(row.replaceAll('\\|', '').split('|').length, 6, row);
  const bad = node('corpus-inspect.mjs', ['--session-dir', dir, '--format', 'html']);
  assert.equal(bad.status, 2);
  assert.equal(JSON.parse(bad.stdout).error.code, 'invalidArguments');
}));

test('the captures view marks empty and duplicate captures and pages every row', () => withDir((dir) => {
  session(dir, [{ pageId: 'page-001', url: 'https://ex.test/a', status: 200, ok: true, textParts: [] }]);
  writeFileSync(join(dir, 'text/page-001.clean.part-001.md'), 'Price: 12 EUR');
  writeFileSync(join(dir, 'text/page-002.clean.part-001.md'), 'Price: 12 EUR');
  writeFileSync(join(dir, 'text/page-003.clean.part-001.md'), '  \n');
  writeFileSync(join(dir, 'text/page-001.md'), 'text index, not a capture');
  writeFileSync(join(dir, 'cdp/cdp-bodies-1-body-001.txt'), '{"items":[1]}');
  const rows = [];
  let res = JSON.parse(node('corpus-inspect.mjs', ['--session-dir', dir, '--view', 'captures', '--limit', '1']).stdout);
  assert.deepEqual(res.captureSummary, { files: 4, empty: 1, duplicates: 1 });
  for (;;) {
    rows.push(...res.captures);
    const next = res.next?.captures;
    if (!next) break;
    res = JSON.parse(node('corpus-inspect.mjs', next.args.slice(1)).stdout);
  }
  assert.deepEqual(rows.map((row) => row.file), ['cdp/cdp-bodies-1-body-001.txt', 'text/page-001.clean.part-001.md', 'text/page-002.clean.part-001.md', 'text/page-003.clean.part-001.md']);
  assert.equal(rows.find((row) => row.file === 'text/page-002.clean.part-001.md').duplicateOf, 'text/page-001.clean.part-001.md');
  assert.equal(rows.find((row) => row.file === 'text/page-003.clean.part-001.md').empty, true);
  assert.equal(rows.filter((row) => row.duplicateOf || row.empty).length, 2);
  const plain = JSON.parse(node('corpus-inspect.mjs', ['--session-dir', dir]).stdout);
  assert.equal(plain.captures, undefined, 'captures are listed only on request');
  assert.equal(readFileSync(join(dir, 'text/page-001.md'), 'utf8'), 'text index, not a capture');
}));

test('a corpus-run extractor writes rows through ctx.write, inside the session only', () => withDir((dir) => {
  session(dir, [{ pageId: 'page-001', url: 'https://ex.test/a', status: 200, ok: true, textParts: ['text/page-001.clean.part-001.md'] }]);
  writeFileSync(join(dir, 'text/page-001.clean.part-001.md'), 'Basic: 10 EUR\nPro: 25 EUR\n');
  const extractor = join(dir, 'prices.mjs');
  writeFileSync(extractor, `export async function run(ctx) {
  const file = 'text/page-001.clean.part-001.md';
  const rows = (await ctx.read(file)).split('\\n').map((line, i) => ({ line, at: i + 1 })).filter((row) => /EUR/.test(row.line))
    .map(({ line, at }) => ({ plan: line.split(':')[0], price: line.split(':')[1].trim(), file, line: at }));
  await ctx.write('extracts/prices/plans.jsonl', rows.map((row) => JSON.stringify(row)).join('\\n') + '\\n');
  return { ok: true, rows: rows.length };
}`);
  const res = node('corpus-run.mjs', ['--session-dir', dir, '--script', extractor]);
  assert.equal(res.status, 0, res.stdout + res.stderr);
  assert.equal(JSON.parse(res.stdout).script.result.rows, 2);
  const written = readFileSync(join(dir, 'extracts/prices/plans.jsonl'), 'utf8').trim().split('\n').map((line) => JSON.parse(line));
  assert.deepEqual(written[1], { plan: 'Pro', price: '25 EUR', file: 'text/page-001.clean.part-001.md', line: 2 });
  writeFileSync(extractor, 'export async function run(ctx) { await ctx.write("../escape.txt", "x"); return { ok: true }; }');
  const escaped = node('corpus-run.mjs', ['--session-dir', dir, '--script', extractor]);
  assert.notEqual(escaped.status, 0);
  assert.equal(JSON.parse(escaped.stdout).error.code, 'scriptFailed');
  assert.match(JSON.parse(escaped.stdout).error.message, /must stay inside/);
}));
