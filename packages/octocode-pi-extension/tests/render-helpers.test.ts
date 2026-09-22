import { truncateToWidth, visibleWidth } from '../src/tui/width.js';
import { sanitizeLine } from '../src/tui/palette.js';
import assert from 'node:assert/strict';
import { visibleWidth as piVisibleWidth } from '@earendil-works/pi-tui';
import { test } from 'vitest';
import { buildOctocodeRenderCall, buildOctocodeRenderResult, buildResultStats, buildToolCallSummary, makeCachedRenderer, makeComponentRenderer, singleLineRenderer, wrapText } from '../src/tools/render-helpers.js';
import { CLI_GLYPH, cliSpinnerFrame } from '../src/tui/cli-design.js';
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

test('CLI design contract centralizes glyphs and spinner frames', () => {
  assert.equal(CLI_GLYPH.tool, '◇');
  assert.equal(cliSpinnerFrame(0), '⠋');
  assert.equal(cliSpinnerFrame(120), '⠙');
});

test('ANSI-aware rendering helpers keep visible width stable', () => {
  assert.equal(visibleWidth('\x1b[31mred\x1b[0m plain'), 9);
  assert.equal(truncateToWidth('abcdef', 4), 'abc\x1b[0m…\x1b[0m');
  assert.equal(truncateToWidth('abcdef', 0), '');
  assert.equal(truncateToWidth('abcdef', 1), '\x1b[0m…\x1b[0m');
  assert.equal(truncateToWidth('\x1b[31mabcdef\x1b[0m', 5), '\x1b[31mabcd\x1b[0m…\x1b[0m');

  // Regression: tab in agent-ledger preview crashed pi TUI (width undercount)
  assert.equal(visibleWidth('a\tb'), 5); // tab expands to 3 spaces, pi-tui parity
  assert.equal(truncateToWidth('27:\tkeypress', 20), '27:   keypress');
  assert.ok(!truncateToWidth('x\ty\tz', 5).includes('\t'));
  assert.equal(visibleWidth('\u{1F600}'), 2); // emoji counts 2 cols
  assert.equal(visibleWidth('⧗'), 1); // ledger icon stays narrow, pi-tui parity
  const wide = truncateToWidth('\u{1F600}\u{1F600}\u{1F600}', 4);
  assert.ok(visibleWidth(wide) <= 4);
  assert.equal(visibleWidth('a\rb\x00c'), 5); // control chars become spaces

  // Regression: truncated output must never exceed maxWidth as measured by
  // pi-tui itself (the renderer crashes on `piVisibleWidth(line) > width`).
  // These char classes undercounted in the hand-rolled width model: EAW-wide
  // singletons (⌚ ⭐ ⬛), VS16 emoji (©️ ‼️), keycaps (1️⃣), flags, ZWJ families.
  const nasty = [
    '⌚⏳⭐⬛◽ watch',
    '©️™️‼️↩️ vs16',
    '1️⃣2️⃣#️⃣ keycaps',
    '🇺🇸🇯🇵 flags',
    '👨‍👩‍👧‍👦 family',
    '\x1b[31m⭐ tab\there\x1b[0m',
    'あいうえお漢字',
  ];
  for (const s of nasty) {
    for (const w of [3, 5, 8, 12]) {
      assert.ok(
        piVisibleWidth(truncateToWidth(s, w)) <= w,
        `pi-tui width of truncate(${JSON.stringify(s)}, ${w})`,
      );
    }
    assert.equal(visibleWidth(s), piVisibleWidth(sanitizeLine(s)));
  }

  assert.deepEqual(wrapText('alpha beta gamma', 10), ['alpha beta', 'gamma']);
  assert.deepEqual(wrapText('superlongword tiny', 5), ['super', 'tiny']);
  assert.deepEqual(wrapText('', 10), ['']);
  assert.deepEqual(wrapText('abc', 0), []);

  const renderer = makeComponentRenderer((_props, _context) => ['x'.repeat(20)], undefined);
  assert.equal(visibleWidth(renderer.render(6)[0]!), 6);
  assert.equal(singleLineRenderer('single long line').render(8)[0], 'single \x1b[0m…\x1b[0m');
});

test('component renderer resolves live props and enforces the terminal width contract', () => {
  let label = 'initial';
  const renderer = makeComponentRenderer(
    (props: { label: string }, context) => [`${props.label} @ ${context.width}`],
    () => ({ label }),
  );
  assert.deepEqual(renderer.render(20), ['initial @ 20']);
  label = 'updated state that is deliberately long';
  assert.ok(visibleWidth(renderer.render(12)[0]!) <= 12);
  assert.match(renderer.render(40)[0]!, /updated state/);
});

test('buildToolCallSummary formats each Octocode direct-tool family', () => {
  const cases: Array<[string, unknown, RegExp]> = [
    ['ghSearch', { queries: [{ operation: 'code', owner: 'octo', repo: 'repo', keywords: ['foo', 'bar'], language: 'ts', filename: 'a.ts' }, { operation: 'code', keywords: ['more'] }] }, /"foo bar".*file:a\.ts.*lang:ts.*in octo\/repo/],
    ['ghSearch', { queries: [{ operation: 'repositories', keywords: ['agent'], language: 'Rust' }] }, /"agent".*lang:Rust/],
    ['ghGetFileContent', { queries: [{ owner: 'octo', repo: 'repo', path: 'src/a.ts', matchString: 'needle in haystack' }] }, /octo\/repo:src\/a\.ts \/needle in haystack\//],
    ['ghGetFileContent', { queries: [{ owner: 'octo', repo: 'repo', path: 'src/a.ts', startLine: 3, endLine: 8 }] }, /:src\/a\.ts:3-8/],
    ['ghSearch', { queries: [{ operation: 'tree', owner: 'octo', repo: 'repo', path: 'packages/pi' }] }, /octo\/repo\/packages\/pi/],
    ['ghGetHistoryItem', { queries: [{ operation: 'pullRequest', owner: 'octo', repo: 'repo', prNumber: 17 }] }, /\[pullRequest\] octo\/repo #17/],
    ['ghSearchHistory', { queries: [{ operation: 'issues', owner: 'octo', repo: 'repo', keywords: ['memory', 'leak'] }] }, /\[issues\] octo\/repo "memory leak"/],
    ['ghSearchHistory', { queries: [{ operation: 'commits', owner: 'octo', repo: 'repo', path: 'src', base: 'main', head: 'next' }] }, /\[commits\] octo\/repo main\.\.next path:src/],
    ['ghCloneRepo', { queries: [{ owner: 'octo', repo: 'repo', sparsePath: 'src' }] }, /octo\/repo\/src/],
    ['ghUnknown', { queries: [{ owner: 'octo', repo: 'repo' }] }, /octo\/repo/],
    ['astSearch', { queries: [{ operation: 'match', pattern: 'class $A', path: '/very/long/path/to/project/src' }, { operation: 'text', searchText: 'next', path: '/tmp' }] }, /\[match\] "class \$A".*project\/src/],
    ['localFetch', { queries: [{ path: '/tmp/src/file.ts', startLine: 10, endLine: 12 }] }, /file\.ts:10-12/],
    ['localFetch', { queries: [{ path: '/tmp/src/file.ts', matchString: 'export function longName' }] }, /file\.ts \/export function long/],
    ['astSearch', { queries: [{ operation: 'tree', path: '/tmp/workspace', maxDepth: 4 }] }, /workspace depth:4/],
    ['astSearch', { queries: [{ operation: 'files', path: '/tmp/workspace', names: ['a.ts', 'b.ts'], pathPattern: 'src/**' }] }, /workspace \[a\.ts, b\.ts\] src\/\*\*/],
    ['astTopology', { queries: [{ operation: 'topology', analysis: 'deadCode', path: '/tmp/workspace', entrypoints: ['src/index.ts'] }] }, /workspace entries:\[src\/index\.ts\]/],
    ['lspSearch', { queries: [{ operation: 'references', symbolName: 'run', uri: 'file:///tmp/src/main.ts?x=1', lineHint: 42 }] }, /references "run" in main\.ts:42/],
    ['artifactSearch', { queries: [{ type: 'npm', packageName: 'vitest' }] }, /npm: vitest/],
    ['artifactSearch', { queries: [{ type: 'crates', keywords: ['async', 'runtime'] }] }, /crates: async runtime/],
    ['customTool', { queries: [{ id: 'skip', reasoning: 'skip', alpha: 'one', beta: 'two', gamma: 'three', delta: 'four' }] }, /one two three/],
  ];

  for (const [toolName, args, pattern] of cases) {
    assert.match(buildToolCallSummary(toolName, args), pattern, toolName);
  }

  assert.equal(buildToolCallSummary('ghSearch', {}), '');
  assert.equal(buildToolCallSummary('localFetch', { queries: [{ path: 'short.ts' }] }), 'short.ts');
});

test('buildResultStats extracts meaningful per-tool result summaries', () => {
  const result = (data: Record<string, unknown>) => ({ data });

  assert.deepEqual(buildResultStats('ghSearch', { results: [result({ operation: 'code', files: [{ path: 'src/a.ts' }] }), result({ operation: 'code', files: [{ repository: { fullName: 'octo/repo' } }] })] }), {
    queryCount: 2,
    summary: '2 results',
    paths: undefined,
    previews: ['src/a.ts', 'octo/repo'],
  });
  assert.deepEqual(buildResultStats('ghSearch', { results: [result({ operation: 'repositories', repositories: [{ fullName: 'a/repo' }, { name: 'fallback' }] })] }), {
    queryCount: 1,
    summary: '2 results',
    paths: ['a/repo', 'fallback'],
    previews: ['a/repo', 'fallback'],
  });
  assert.deepEqual(buildResultStats('ghGetFileContent', { results: [result({ path: 'src/a.ts', content: 'export const a = 1;' }), result({ filePath: 'src/b.ts' })] }), {
    queryCount: 2,
    paths: ['a.ts', 'b.ts'],
    previews: ['export const a = 1;'],
  });
  assert.deepEqual(buildResultStats('ghSearch', { results: [result({ operation: 'tree', structure: [{ path: 'a' }, { path: 'b' }] })] }), {
    queryCount: 1,
    summary: '2 results',
    paths: undefined,
    previews: ['a', 'b'],
  });
  assert.deepEqual(buildResultStats('ghCloneRepo', { results: [result({ localPath: '/tmp/repo' }), result({ path: '/tmp/other' })] }), {
    queryCount: 2,
    paths: ['/tmp/repo', '/tmp/other'],
  });
  assert.deepEqual(buildResultStats('localSearch', { results: [result({ stats: { totalOccurrences: 3, filesMatched: 2 } }), result({ stats: { totalOccurrences: 2 } })] }), {
    queryCount: 2,
    summary: '5 matches, 2 files',
  });
  assert.deepEqual(buildResultStats('localFetch', { results: [result({ resolvedPath: '/tmp/a.ts', totalLines: 9, content: 'function run() {}' })] }), {
    queryCount: 1,
    paths: ['a.ts'],
    summary: '9 lines',
    previews: ['function run() {}'],
  });
  assert.deepEqual(buildResultStats('localSearch', { results: [result({ files: ['a'] })] }), {
    queryCount: 1,
    summary: '1 entries',
  });
  assert.deepEqual(buildResultStats('localSearch', { results: [result({ files: ['a', 'b'], folders: ['c', 'd', 'e'] })] }), {
    queryCount: 1,
    summary: '5 entries',
  });
  assert.deepEqual(buildResultStats('lspSearch', { results: [result({ location: { uri: 'file:///tmp/a.ts', line: 12 }, references: [{}, {}] }), result({ symbols: [{}] })] }), {
    queryCount: 2,
    paths: ['a.ts:12'],
    summary: '3 refs',
  });
  assert.deepEqual(buildResultStats('artifactSearch', { results: [result({ artifacts: [{ name: 'pkg', version: '1.2.3' }] }), result({ artifacts: [{ name: 'other' }] })] }), {
    queryCount: 2,
    paths: ['pkg@1.2.3', 'other'],
  });
  assert.deepEqual(buildResultStats('ghSearchHistory', { results: [result({ items: [{}, {}] }), result({ pullRequests: [{}] })] }), {
    queryCount: 2,
    summary: '3 items',
  });
  assert.deepEqual(buildResultStats('ghSearchHistory', { results: [result({ issues: [{}, {}] })] }), {
    queryCount: 1,
    summary: '2 items',
  });
  assert.deepEqual(buildResultStats('ghGetHistoryItem', { results: [result({ commits: [{}, {}, {}] })] }), {
    queryCount: 1,
    summary: '3 items',
  });
  assert.deepEqual(buildResultStats('unknown', { results: [result({})] }), { queryCount: 1 });
  assert.deepEqual(buildResultStats('unknown', null), {});
});

test('Octocode renderers cover partial, collapsed, expanded, stats, and error states', () => {
  const call = buildOctocodeRenderCall('ghSearch', { queries: [{ operation: 'code', owner: 'o', repo: 'r', keywords: ['x'] }] }, theme).render(120)[0]!;
  assert.match(call, /<accent>◇<\/accent>/);
  assert.match(call, /<toolTitle><b>ghSearch<\/b><\/toolTitle>/);
  assert.match(call, /<dim> · <\/dim><dim>"x" in o\/r<\/dim>/);
  const callLines = buildOctocodeRenderCall('ghSearch', { queries: [{ operation: 'code', owner: 'o', repo: 'r', keywords: ['x'], reasoning: 'find x' }] }, theme).render(120);
  assert.equal(callLines.length, 2);
  assert.match(callLines[1]!, /find x/);
  assert.doesNotMatch(callLines.join('\n'), /request:|reasoning:/);

  const parallelCall = buildOctocodeRenderCall('ghSearch', {
    queryRunType: 'parallel',
    queries: [
      { owner: 'o', repo: 'r', keywords: ['x'], reasoning: 'find x' },
      { owner: 'o', repo: 'r', keywords: ['y'], reasoning: 'find y' },
    ],
  }, theme).render(160);
  assert.match(parallelCall[0]!, /2 queries.*parallel/);

  const running = buildOctocodeRenderResult('localSearch', textResult('still running'), { isPartial: true }, theme).render(120)[0]!;
  assert.match(running, /<accent>⠋|<accent>⠙|<accent>⠹|<accent>⠸|<accent>⠼|<accent>⠴|<accent>⠦|<accent>⠧|<accent>⠇|<accent>⠏/);
  assert.match(running, /<toolTitle>localSearch<\/toolTitle>/);
  assert.match(running, /<accent>running…<\/accent>/);

  const collapsed = buildOctocodeRenderResult(
    'localSearch',
    textResult('ok', { results: [{ data: { stats: { totalOccurrences: 4, filesMatched: 2 } } }] }),
    { expanded: false },
    theme,
  ).render(180)[0]!;
  assert.match(collapsed, /<success>✓<\/success>/);
  assert.match(collapsed, /4 matches, 2 files/);
  assert.match(collapsed, /→ ok/, 'collapsed rows carry the first line of the result');

  const bare = buildOctocodeRenderResult('artifactSearch', textResult('found 3 packages\nsecond line'), { expanded: false }, theme).render(180)[0]!;
  assert.match(bare, /<dim>→ found 3 packages<\/dim>/, 'no stats → the response text is the result');
  assert.doesNotMatch(bare, /second line/);
  assert.match(bare, /Ctrl\+O details/, 'multi-line results advertise their disclosure key inline');
  assert.doesNotMatch(collapsed, /Ctrl\+O details/, 'complete one-line results do not advertise empty detail');

  const withPreview = buildOctocodeRenderResult(
    'localFetch',
    textResult('ok', { results: [{ data: { resolvedPath: '/tmp/a.ts', totalLines: 2, content: 'const answer = 42;' } }] }),
    { expanded: false },
    theme,
  ).render(180)[0]!;
  assert.match(withPreview, /a\.ts/);
  assert.match(withPreview, /“const answer = 42;”/);
  assert.doesNotMatch(withPreview, /→ ok/, 'a structured preview replaces the raw-text fallback');

  const providerRows = buildOctocodeRenderResult(
    'localFetch',
    textResult('batch complete', {
      results: [
        { data: { resolvedPath: '/tmp/a.ts', totalLines: 2 } },
        { status: 'error', error: 'permission denied' },
      ],
    }),
    { expanded: false },
    theme,
  ).render(180);
  assert.equal(providerRows.length, 2);
  assert.match(providerRows[0]!, /✓.*\[0\].*a\.ts/);
  assert.match(providerRows[1]!, /✗.*\[1\].*permission denied/);
  assert.doesNotMatch(providerRows.join('\n'), /2 queries/);

  const canonicalRows = buildOctocodeRenderResult(
    'readMedia',
    textResult('2 queries succeeded · parallel.', {
      queryRunType: 'parallel',
      results: [
        { index: 0, status: 'success', summary: 'image a loaded' },
        { index: 1, status: 'success', summary: 'image b loaded' },
      ],
    }),
    { expanded: false },
    theme,
  ).render(180);
  assert.match(canonicalRows[0]!, /2 queries.*parallel/);
  assert.match(canonicalRows[1]!, /\[0\].*image a loaded/);
  assert.match(canonicalRows[2]!, /\[1\].*image b loaded/);

  const expandedBatch = buildOctocodeRenderResult(
    'readMedia',
    textResult('2 queries succeeded · parallel.\nfull batch evidence', {
      queryRunType: 'parallel',
      results: [
        { index: 0, status: 'success', summary: 'image a loaded' },
        { index: 1, status: 'success', summary: 'image b loaded' },
      ],
    }),
    { expanded: true },
    theme,
  ).render(180).join('\n');
  assert.match(expandedBatch, /response:/);
  assert.match(expandedBatch, /full batch evidence/, 'expanded batches reveal their full content instead of staying collapsed');

  const expanded = buildOctocodeRenderResult(
    'ghGetFileContent',
    textResult(Array.from({ length: 30 }, (_, i) => `line ${i + 1}`).join('\n'), { results: [{ data: { path: 'src/a.ts' } }] }),
    { expanded: true },
    theme,
  ).render(80);
  assert.equal(expanded.length, 28);
  assert.match(expanded.join('\n'), /response:/);
  assert.match(expanded.at(-1)!, /5 more lines hidden/);

  const error = buildOctocodeRenderResult('artifactSearch', textResult('bad', {}, true), { expanded: false }, theme).render(120)[0]!;
  assert.match(error, /<error>✗<\/error>/);
});

test('error result rows surface the failure text and honor system-level context.isError', () => {
  // result.isError path now shows the message text, not just the glyph.
  const r1 = buildOctocodeRenderResult('artifactSearch', textResult('boom: it failed', {}, true), { expanded: false }, theme).render(200)[0]!;
  assert.match(r1, /<error>✗<\/error>/);
  assert.match(r1, /<error>boom: it failed<\/error>/);

  // context.isError (system-level) marks the row as an error even when the
  // returned result.isError is false — Pi ignores the returned flag.
  const r2 = buildOctocodeRenderResult(
    'localFetch',
    textResult('arguments: must be object', {}, false),
    { expanded: false },
    theme,
    { isError: true, invalidate() {} },
  ).render(200)[0]!;
  assert.match(r2, /<error>✗<\/error>/);
  assert.match(r2, /arguments: must be object/);

  // A success result with no error stays a success row (no regression).
  const okRow = buildOctocodeRenderResult(
    'localFetch',
    textResult('ok', { results: [{ data: { path: 'a.ts', totalLines: 1 } }] }, false),
    { expanded: false },
    theme,
    { isError: false, invalidate() {} },
  ).render(200)[0]!;
  assert.match(okRow, /<success>✓<\/success>/);

  // Expanded error dumps the message body under the header.
  const r3 = buildOctocodeRenderResult('MCPTool', textResult('line A\nline B', {}, true), { expanded: true }, theme).render(200);
  assert.match(r3[0]!, /<error>✗<\/error>/);
  assert.ok(r3.some((l) => /line B/.test(l)));
});

test('single Octocode query errors show the actionable cause instead of a structural key or error code', () => {
  const text = [
    'results:',
    '- index: 0',
    '  status: error',
    '  data:',
    '    error: File not found: missing-root. Verify the path with astSearch operation:"files".',
    '    errorCode: fileAccessFailed',
  ].join('\n');
  const row = buildOctocodeRenderResult(
    'localSearch',
    textResult(text, {}, true),
    { expanded: false },
    theme,
  ).render(240)[0]!;

  assert.match(row, /File not found: missing-root/);
  assert.doesNotMatch(row, /· results:|errorCode: fileAccessFailed/);
});

test('makeCachedRenderer memoizes lines per width and clears on invalidate', () => {
  let calls = 0;
  const r = makeCachedRenderer((w) => {
    calls += 1;
    return [`w=${w}`];
  });
  assert.deepEqual(r.render(80), ['w=80']);
  assert.deepEqual(r.render(80), ['w=80']);
  assert.equal(calls, 1, 'same width is served from cache');
  assert.deepEqual(r.render(40), ['w=40']);
  assert.equal(calls, 2, 'a new width recomputes');
  r.invalidate();
  assert.deepEqual(r.render(80), ['w=80']);
  assert.equal(calls, 3, 'invalidate() clears the cache');
});
