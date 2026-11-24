import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { callersUnion } from '../callers-union.mjs';

const SCRIPT = fileURLToPath(new URL('../callers-union.mjs', import.meta.url));
const TARGET = { id: 'src/lib/fail.mjs#fail', kind: 'function', symbolName: 'fail', line: 6, endLine: 9, path: 'src/lib/fail.mjs' };

// Shapes follow real replies: graphQuery callers rows (symbol rows carry path/line, file rows only id) and
// lspSearch callers files[].matches[] (module callers have kind module, call sites listed per caller).
const graphRow = (path, symbolName, line, endLine, sites) => ({ id: `${path}#${symbolName}`, kind: 'function', depth: 1, edge: 'calls', sites, confidence: 'high', edgeLine: line + 1, via: 'import', endLine, symbolName, path, line });
const graphFile = (path, sites) => ({ id: path, kind: 'file', depth: 1, edge: 'calls', via: 'import', edgeLine: 3, confidence: 'high', sites });
const lspMatch = (symbolName, line, endLine, siteLines) => ({ symbolName, kind: 'function', line, endLine, sites: siteLines.map((site) => ({ line: site, column: 5 })) });
const lspModule = (siteLines) => ({ kind: 'module', line: 1, endLine: 40, sites: siteLines.map((site) => ({ line: site, column: 1 })) });

const graphReply = (rows, extra = {}) => ({ results: [{ index: 0, data: { graph: { id: 'g1' }, target: TARGET, coverage: { basis: 'syntax', callExtraction: 'supported', language: 'javascript' }, rows, ...extra } }] });
const lspReply = (files, extra = {}, next) => ({ results: [{ index: 0, data: { workspaceRoot: 'src', files, ...extra } }], ...(next ? { next: { tool: 'lspSearch', input: { cursor: next } } } : {}) });

const GRAPH = graphReply([
  graphRow('src/a.mjs', 'main', 10, 20, 2),
  graphRow('src/b.mjs', 'run', 5, 15, 1),
  graphFile('src/c.mjs', 1),
  graphRow('src/d.mjs', 'onlyGraph', 30, 31, 1),
]);
const LSP_FILES = [
  { path: 'src/a.mjs', matches: [lspMatch('main', 10, 20, [11, 14])] },
  { path: 'src/b.mjs', matches: [lspMatch('run', 6, 15, [7, 9])] },
  { path: 'src/c.mjs', matches: [lspModule([3])] },
  { path: 'src/e.mjs', matches: [lspMatch('onlyLsp', 2, 4, [3])] },
];

function run(t, graph, lsp, { raw = false } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'callers-union-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const files = [graph, lsp].map((value, index) => {
    const file = join(dir, `${index}.json`);
    writeFileSync(file, raw ? value : JSON.stringify(value));
    return file;
  });
  const result = spawnSync(process.execPath, [SCRIPT, ...files], { encoding: 'utf8' });
  return { status: result.status, output: JSON.parse(result.stdout) };
}

test('union counts every caller once, tags its sources and lists the rows only one tool found', () => {
  const report = callersUnion([GRAPH], [lspReply(LSP_FILES)]);
  const keys = report.callers.map((caller) => caller.key);
  assert.equal(new Set(keys).size, keys.length, 'each caller appears once');
  assert.equal(report.counts.union, report.callers.length);
  assert.equal(report.counts.both + report.counts.graphOnly + report.counts.lspOnly, report.counts.union);
  assert.deepEqual(report.graphOnly, ['src/d.mjs:30']);
  assert.deepEqual(report.lspOnly, ['src/e.mjs:2']);
  assert.deepEqual(report.callers.find((caller) => caller.path === 'src/c.mjs').sources, ['graph', 'lsp'], 'module callers match as path:module');
  const shifted = report.callers.find((caller) => caller.path === 'src/b.mjs');
  assert.equal(shifted.matchedBy, 'overlap', 'a name line one tool places differently still pairs by path, name and range');
  assert.equal(shifted.lspKey, 'src/b.mjs:6');
  assert.equal(report.target.id, TARGET.id);
  assert.equal(report.complete, true);
  assert.deepEqual(report.gaps, []);
});

test('different call-site counts for one caller are flagged', () => {
  const report = callersUnion([GRAPH], [lspReply(LSP_FILES)]);
  const flagged = report.callers.filter((caller) => caller.sitesDiffer).map((caller) => caller.key);
  assert.deepEqual(flagged, ['src/b.mjs:5'], 'graph counts 1 site in run, lsp lists 2');
});

test('an unfinished page, partial reasons and an unverified lsp level keep the union a lower bound', () => {
  const graph = graphReply(GRAPH.results[0].data.rows, { partialReasons: ['callSitesUnresolved'] });
  const lsp = lspReply(LSP_FILES, { lsp: { level: 'syntax' } }, 'cursor2');
  const report = callersUnion([graph], [lsp]);
  assert.equal(report.complete, false);
  assert.deepEqual(report.gaps, [
    { tool: 'graph', reason: 'callSitesUnresolved' },
    { tool: 'lsp', reason: 'level syntax' },
    { tool: 'lsp', reason: 'unfinishedPages' },
  ]);
  assert.equal(report.coverage.lsp.unfinishedPages, true);
});

test('pages given one reply per line are joined, and the last page decides completeness', (t) => {
  const first = lspReply(LSP_FILES.slice(0, 2), {}, 'cursor2');
  const last = lspReply(LSP_FILES.slice(2));
  const lines = `${JSON.stringify(first)}\n${JSON.stringify(last)}\n`;
  const { status, output } = run(t, JSON.stringify(GRAPH), lines, { raw: true });
  assert.equal(status, 0);
  assert.equal(output.counts.union, callersUnion([GRAPH], [lspReply(LSP_FILES)]).counts.union);
  assert.equal(output.complete, true);
});

test('swapped files, error replies and several queries in one file are invalid input (exit 2)', (t) => {
  const swapped = run(t, lspReply(LSP_FILES), GRAPH);
  assert.equal(swapped.status, 2);
  assert.equal(swapped.output.error.code, 'wrongOrder');

  const error = { results: [{ index: 0, status: 'error', data: { errorCode: 'invalidInput', error: '2 nodes match "fail"' } }] };
  const failed = run(t, error, lspReply(LSP_FILES));
  assert.equal(failed.status, 2);
  assert.equal(failed.output.error.code, 'errorReply');

  const several = { results: [...lspReply(LSP_FILES).results, { ...lspReply(LSP_FILES).results[0], index: 1 }] };
  const mixed = run(t, GRAPH, several);
  assert.equal(mixed.status, 2);
  assert.equal(mixed.output.error.code, 'severalQueries');
});

test('help names the inputs, output and exit codes', () => {
  const help = spawnSync(process.execPath, [SCRIPT, '--help'], { encoding: 'utf8' });
  assert.equal(help.status, 0);
  for (const word of ['graphQuery', 'lspSearch', 'graphOnly', 'lspOnly', 'gaps', 'complete', 'Exit codes']) assert.match(help.stdout, new RegExp(word));
});
