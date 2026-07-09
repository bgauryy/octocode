import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
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
