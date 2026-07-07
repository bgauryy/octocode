import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  buildMemoryRenderCall,
  buildMemoryRenderResult,
  buildOctocodeRenderCall,
  buildOctocodeRenderResult,
  buildResultStats,
  buildToolCallSummary,
  makeRenderer,
  singleLineRenderer,
  truncateToWidth,
  visibleWidth,
  wrapText,
} from '../src/tools/render-helpers.js';
import type { PiTheme, ToolCallResult } from '../src/types.js';

const theme: PiTheme = {
  bold: (text: string) => `<b>${text}</b>`,
  fg: (color: string, text: string) => `<${color}>${text}</${color}>`,
};

function textResult(text: string, details: unknown = {}, isError = false): ToolCallResult {
  return {
    isError,
    content: [{ type: 'text', text }],
    details,
  };
}

test('ANSI-aware rendering helpers keep visible width stable', () => {
  assert.equal(visibleWidth('\x1b[31mred\x1b[0m plain'), 9);
  assert.equal(truncateToWidth('abcdef', 4), 'abc…\x1b[0m');
  assert.equal(truncateToWidth('abcdef', 0), '');
  assert.equal(truncateToWidth('abcdef', 1), '…');
  assert.equal(truncateToWidth('\x1b[31mabcdef\x1b[0m', 5), '\x1b[31mabcd…\x1b[0m');

  assert.deepEqual(wrapText('alpha beta gamma', 10), ['alpha beta', 'gamma']);
  assert.deepEqual(wrapText('superlongword tiny', 5), ['super', 'tiny']);
  assert.deepEqual(wrapText('', 10), ['']);
  assert.deepEqual(wrapText('abc', 0), []);

  const renderer = makeRenderer(() => ['x'.repeat(20)]);
  assert.equal(visibleWidth(renderer.render(6)[0]!), 6);
  assert.equal(singleLineRenderer('single long line').render(8)[0], 'single …\x1b[0m');
});

test('buildToolCallSummary formats each Octocode direct-tool family', () => {
  const cases: Array<[string, unknown, RegExp]> = [
    ['ghSearchCode', { queries: [{ owner: 'octo', repo: 'repo', keywords: ['foo', 'bar'], language: 'ts', filename: 'a.ts' }, { keywords: ['more'] }] }, /"foo bar".*file:a\.ts.*lang:ts.*in octo\/repo.*\+1/],
    ['ghSearchRepos', { queries: [{ keywords: ['agent'], language: 'Rust' }] }, /"agent".*lang:Rust/],
    ['ghGetFileContent', { queries: [{ owner: 'octo', repo: 'repo', path: 'src/a.ts', matchString: 'needle in haystack' }] }, /octo\/repo:src\/a\.ts \/needle in haystack\//],
    ['ghGetFileContent', { queries: [{ owner: 'octo', repo: 'repo', path: 'src/a.ts', startLine: 3, endLine: 8 }] }, /:src\/a\.ts:3-8/],
    ['ghViewRepoStructure', { queries: [{ owner: 'octo', repo: 'repo', path: 'packages/pi' }] }, /octo\/repo\/packages\/pi/],
    ['ghHistoryResearch', { queries: [{ owner: 'octo', repo: 'repo', type: 'commits', prNumber: 17 }] }, /octo\/repo commits#17/],
    ['ghCloneRepo', { queries: [{ owner: 'octo', repo: 'repo', sparsePath: 'src' }] }, /octo\/repo\/src/],
    ['ghUnknown', { queries: [{ owner: 'octo', repo: 'repo' }] }, /octo\/repo/],
    ['localSearchCode', { queries: [{ keywords: 'class Foo', path: '/very/long/path/to/project/src', mode: 'ast' }, { keywords: 'next' }] }, /\[ast\] "class Foo".*project\/src.*\+1/],
    ['localGetFileContent', { queries: [{ path: '/tmp/src/file.ts', startLine: 10, endLine: 12 }] }, /file\.ts:10-12/],
    ['localGetFileContent', { queries: [{ path: '/tmp/src/file.ts', matchString: 'export function longName' }] }, /file\.ts \/export function long/],
    ['localViewStructure', { queries: [{ path: '/tmp/workspace', maxDepth: 4 }] }, /workspace depth:4/],
    ['localFindFiles', { queries: [{ path: '/tmp/workspace', names: ['a.ts', 'b.ts'], pathPattern: 'src/**' }] }, /workspace \[a\.ts, b\.ts\] src\/\*\*/],
    ['localBinaryInspect', { queries: [{ path: '/tmp/archive.zip', mode: 'list' }] }, /archive\.zip \(list\)/],
    ['lspGetSemantics', { queries: [{ type: 'references', symbolName: 'run', uri: 'file:///tmp/src/main.ts?x=1', lineHint: 42 }] }, /references "run" in main\.ts:42/],
    ['npmSearch', { queries: [{ packageName: 'vitest' }] }, /vitest/],
    ['customTool', { queries: [{ id: 'skip', reasoning: 'skip', alpha: 'one', beta: 'two', gamma: 'three', delta: 'four' }] }, /one two three/],
  ];

  for (const [toolName, args, pattern] of cases) {
    assert.match(buildToolCallSummary(toolName, args), pattern, toolName);
  }

  assert.equal(buildToolCallSummary('ghSearchCode', {}), '');
  assert.equal(buildToolCallSummary('localGetFileContent', { queries: [{ path: 'short.ts' }] }), 'short.ts');
});

test('buildResultStats extracts meaningful per-tool result summaries', () => {
  const result = (data: Record<string, unknown>) => ({ data });

  assert.deepEqual(buildResultStats('ghSearchCode', { results: [result({ totalCount: 7 }), result({ items: [{}, {}] })] }), {
    queryCount: 2,
    summary: '9 results',
    paths: undefined,
  });
  assert.deepEqual(buildResultStats('ghSearchRepos', { results: [result({ items: [{ fullName: 'a/repo' }, { name: 'fallback' }] })] }), {
    queryCount: 1,
    summary: '2 results',
    paths: ['a/repo', 'fallback'],
  });
  assert.deepEqual(buildResultStats('ghGetFileContent', { results: [result({ path: 'src/a.ts' }), result({ filePath: 'src/b.ts' })] }), {
    queryCount: 2,
    paths: ['a.ts', 'b.ts'],
  });
  assert.deepEqual(buildResultStats('ghViewRepoStructure', { results: [result({ totalEntries: 5 }), result({ files: ['a', 'b'] })] }), {
    queryCount: 2,
    summary: '7 entries',
  });
  assert.deepEqual(buildResultStats('ghCloneRepo', { results: [result({ localPath: '/tmp/repo' }), result({ path: '/tmp/other' })] }), {
    queryCount: 2,
    paths: ['/tmp/repo', '/tmp/other'],
  });
  assert.deepEqual(buildResultStats('localSearchCode', { results: [result({ totalMatches: 3, totalFiles: 2 }), result({ matches: [{}, {}] })] }), {
    queryCount: 2,
    summary: '5 matches, 2 files',
  });
  assert.deepEqual(buildResultStats('localGetFileContent', { results: [result({ resolvedPath: '/tmp/a.ts', totalLines: 9 })] }), {
    queryCount: 1,
    paths: ['a.ts'],
    summary: '9 lines',
  });
  assert.deepEqual(buildResultStats('localViewStructure', { results: [result({ files: ['a'] })] }), {
    queryCount: 1,
    summary: '1 entries',
  });
  assert.deepEqual(buildResultStats('localFindFiles', { results: [result({ entries: ['a', 'b'] }), result({ totalEntries: 3 })] }), {
    queryCount: 2,
    summary: '5 files',
  });
  assert.deepEqual(buildResultStats('lspGetSemantics', { results: [result({ location: { uri: 'file:///tmp/a.ts', line: 12 }, references: [{}, {}] }), result({ symbols: [{}] })] }), {
    queryCount: 2,
    paths: ['a.ts:12'],
    summary: '3 refs',
  });
  assert.deepEqual(buildResultStats('npmSearch', { results: [result({ name: 'pkg', version: '1.2.3' }), result({ packageName: 'other' })] }), {
    queryCount: 2,
    paths: ['pkg@1.2.3', 'other'],
  });
  assert.deepEqual(buildResultStats('ghHistoryResearch', { results: [result({ items: [{}, {}] }), result({ prs: [{}] }), result({ commits: [{}, {}, {}] })] }), {
    queryCount: 3,
    summary: '6 items',
  });
  assert.deepEqual(buildResultStats('unknown', { results: [result({})] }), { queryCount: 1 });
  assert.deepEqual(buildResultStats('unknown', null), {});
});

test('Octocode renderers cover partial, collapsed, expanded, stats, and error states', () => {
  const call = buildOctocodeRenderCall('ghSearchCode', { queries: [{ owner: 'o', repo: 'r', keywords: ['x'] }] }, theme).render(120)[0]!;
  assert.match(call, /<toolTitle><b>ghSearchCode<\/b><\/toolTitle>/);
  assert.match(call, /<dim>"x" in o\/r<\/dim>/);

  assert.equal(
    buildOctocodeRenderResult('localSearchCode', textResult('still running'), { isPartial: true }, theme).render(120)[0],
    '<warning>localSearchCode running…</warning>',
  );

  const collapsed = buildOctocodeRenderResult(
    'localSearchCode',
    textResult('ok', { results: [{ data: { totalMatches: 4, totalFiles: 2 } }] }),
    { expanded: false },
    theme,
  ).render(180)[0]!;
  assert.match(collapsed, /<success>✓<\/success>/);
  assert.match(collapsed, /4 matches, 2 files/);
  assert.match(collapsed, /expand for full output/);

  const expanded = buildOctocodeRenderResult(
    'ghGetFileContent',
    textResult(Array.from({ length: 30 }, (_, i) => `line ${i + 1}`).join('\n'), { results: [{ data: { path: 'src/a.ts' } }] }),
    { expanded: true },
    theme,
  ).render(80);
  assert.equal(expanded.length, 27);
  assert.match(expanded.at(-1)!, /5 more lines hidden/);

  const error = buildOctocodeRenderResult('npmSearch', textResult('bad', {}, true), { expanded: false }, theme).render(120)[0]!;
  assert.match(error, /<error>✗<\/error>/);
});

test('memory renderCall formats every memory support surface', () => {
  const cases: Array<[string, unknown, RegExp]> = [
    ['memory_recall', { query: 'find the thing', label: 'BUG' }, /find the thing \[BUG\]/],
    ['memory_record', { label: 'TEST', importance: 8, task_context: 'record this lesson' }, /\[TEST·8\] record this lesson/],
    ['memory_reflect', { task: 'ship tests', outcome: 'worked' }, /ship tests \(worked\)/],
    ['memory_verify', { allPending: true, status: 'SUCCESS' }, /allPending → SUCCESS/],
    ['memory_verify', { task_ids: ['a', 'b'] }, /2 tasks/],
    ['memory_verify', { task_id: 'task_abcdefghijklmnopqrstuvwxyz' }, /task_abcdefghijklmno/],
    ['memory_forget', { tags: ['old', 'bad'], max_importance: 3, before: '2026-01-02T00:00:00Z', dry_run: true }, /tags:\[old, bad\] ≤3 before:2026-01-02T… dry_run/],
    ['memory_digest', { dry_run: true, export_doc: true }, /dry_run export_doc/],
    ['memory_notify', { kind: 'handoff', subject: 'coverage branch done' }, /\[handoff\] coverage branch done/],
    ['memory_refine_get', { state: 'open' }, /state:open/],
    ['memory_workspace_status', {}, /memory_workspace_status/],
  ];

  for (const [toolName, args, pattern] of cases) {
    const line = buildMemoryRenderCall(toolName, args, theme).render(160)[0]!;
    assert.match(line, pattern, toolName);
  }
});

test('memory renderResult parses JSON stats and expanded output', () => {
  const cases: Array<[string, string, RegExp]> = [
    ['memory_recall', JSON.stringify({ memories: [{}, {}] }), /2 memories/],
    ['memory_record', JSON.stringify({ label: 'BUG' }), /recorded \[BUG\]/],
    ['memory_record', JSON.stringify({ skipped: true }), /skipped \(similar exists\)/],
    ['memory_reflect', JSON.stringify({ outcome: 'worked' }), /reflected \(worked\)/],
    ['file_lock', JSON.stringify({ type: 'lock', files: ['a'], expiresAt: 'tomorrow' }), /locked 1 file until tomorrow/],
    ['memory_file_lock', JSON.stringify({ type: 'status', locks: [{}, {}] }), /2 locks/],
    ['file_lock', JSON.stringify({ type: 'renew', locks_renewed: 3 }), /3 renewed/],
    ['file_lock', JSON.stringify({ type: 'release', locks_released: 4 }), /4 released/],
    ['memory_workspace_status', JSON.stringify({ locks: [{}], agents: [{}, {}], pending_tasks: 3 }), /1 lock, 2 agents, 3 pending/],
    ['memory_workspace_status', JSON.stringify({ locks: [], agents: [], pending_tasks: 0 }), /no activity/],
    ['memory_refine_get', JSON.stringify({ refinements: [{}] }), /1 refinement/],
    ['memory_audit_unverified', JSON.stringify({ pending: [{}, {}] }), /2 pending tasks/],
    ['memory_verify', JSON.stringify({ results: [{}, {}] }), /2 verified/],
    ['memory_digest', JSON.stringify({ archived: 2, pruned: 3 }), /5 cleaned \(2 archived, 3 pruned\)/],
    ['memory_digest', JSON.stringify({ archived: 0, pruned: 0 }), /nothing to clean/],
    ['memory_forget', JSON.stringify({ dry_run: true, previewed: 9 }), /preview: 9 would delete/],
    ['memory_forget', JSON.stringify({ deleted: 2 }), /2 deleted/],
    ['memory_notify', JSON.stringify({ ok: true }), /posted/],
  ];

  for (const [toolName, json, pattern] of cases) {
    const line = buildMemoryRenderResult(toolName, textResult(json), { expanded: false }, theme).render(200)[0]!;
    assert.match(line, pattern, toolName);
  }

  assert.equal(
    buildMemoryRenderResult('memory_recall', textResult('{}'), { isPartial: true }, theme).render(80)[0],
    '<warning>memory_recall…</warning>',
  );
  assert.match(
    buildMemoryRenderResult('memory_recall', textResult('not-json'), { expanded: false }, theme).render(80)[0]!,
    /memory_recall/,
  );
  assert.match(
    buildMemoryRenderResult('memory_recall', textResult('bad', {}, true), { expanded: false }, theme).render(80)[0]!,
    /<error>✗<\/error>/,
  );

  const expanded = buildMemoryRenderResult(
    'memory_recall',
    textResult(JSON.stringify({ count: 1, memories: [{ observation: 'kept' }] })),
    { expanded: true },
    theme,
  ).render(120);
  assert.ok(expanded.some((line) => line.includes('"observation": "kept"')));
});
