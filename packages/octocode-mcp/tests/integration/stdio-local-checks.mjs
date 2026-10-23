// Local read, search, discovery and AST checks for stdio.acceptance.mjs, run
// against the isolated source fixture.
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import {
  call,
  fixture,
  invoke,
  listedPaths,
  nextCall,
  receipt,
  state,
} from './stdio-harness.mjs';

async function numberedLocalRead() {
  const file = path.join(fixture, 'math.ts');
  const data = await call('localFetch', {
    path: file,
  });
  // `<line>\t<text>` per source line (docs/TOOL_DATA_CONTRACT.md).
  const source = await readFile(file, 'utf8');
  const numbered = source
    .split('\n')
    .map((line, index, all) =>
      index === all.length - 1 && line === '' ? '' : `${index + 1}\t${line}`
    )
    .join('\n');
  assert.equal(data.content, numbered);
  const { response } = receipt.calls.at(-1);
  assert.equal(
    JSON.parse(response.content[0].text).results[0].data.content,
    numbered
  );
}

async function orderedBulkResults() {
  const response = await invoke('localFetch', {
    queries: [
      {
        reasoning: 'Read the first bulk fixture through native MCP.',
        path: path.join(fixture, 'math.ts'),
        ranges: ['1-1'],
      },
      {
        reasoning: 'Read the second bulk fixture through native MCP.',
        path: path.join(fixture, 'entry.ts'),
        ranges: ['1-1'],
      },
    ],
  });
  assert.equal(response.isError, false);
  const rows = response.structuredContent?.results;
  assert.equal(rows?.length, 2);
  assert.deepEqual(
    rows.map(row => row.index),
    [0, 1]
  );
  assert.ok(rows[0].data.content.includes('Arithmetic fixture'));
  assert.ok(rows[1].data.content.includes('import'));
}

/** Follows every continueWindow of one localFetch chunk view. */
async function readChunkView(file, source, unit, matched) {
  // Byte accounting (sourceBytes/returnedBytes) is opt-in `stats`;
  // continuations carry extras forward.
  let page = await call('localFetch', {
    path: file,
    unit,
    length: unit === 'lines' ? 1 : 3,
    extras: ['stats'],
    ...(matched
      ? { matchString: 'needle', contextLines: 0, minify: 'standard' }
      : {}),
  });
  let content = '';
  let count = 0;
  const numbers = [];
  for (;;) {
    assert.ok(++count < 100);
    assert.equal(page.totalLines, 5);
    assert.equal(page.sourceBytes, Buffer.byteLength(source));
    // Line views carry a `<line>\t` gutter (TOOL_DATA_CONTRACT numbered
    // content); returnedBytes counts only the source bytes under it.
    // Byte windows stay verbatim.
    const text =
      unit === 'lines'
        ? page.content.replace(/^(\d+)\t/gm, (_, line) => {
            numbers.push(Number(line));
            return '';
          })
        : page.content;
    assert.equal(page.returnedBytes, Buffer.byteLength(text));
    // matchString reads are verbatim; no fallback notice says so per row.
    assert.equal(page.minifyFallback, undefined);
    content += text;
    if (!page.next?.continueWindow) break;
    assert.equal(page.next.continueWindow.tool, 'localFetch');
    page = await nextCall(page.next.continueWindow);
  }
  return { content, numbers };
}

async function chunkedLocalFetch() {
  const directory = await mkdtemp(path.join(fixture, 'fetch-chunks-'));
  const file = path.join(directory, 'source.txt');
  const source = 'skip\r\nneedle 🌍\r\n\r\nneedle café\nlast\n';
  try {
    await writeFile(file, source);
    for (const unit of ['lines', 'bytes']) {
      for (const matched of [false, true]) {
        const { content, numbers } = await readChunkView(
          file,
          source,
          unit,
          matched
        );
        if (unit === 'lines')
          assert.deepEqual(numbers, matched ? [2, 4] : [1, 2, 3, 4, 5]);
        assert.equal(
          content,
          matched
            ? 'needle 🌍\r\n... [line 3 not requested] ...\nneedle café\n'
            : source
        );
      }
    }
    const invalid = await invoke('localFetch', {
      queries: [
        {
          reasoning: 'Verify an unknown localFetch window field is rejected.',
          path: file,
          window: 3,
        },
      ],
    });
    assert.equal(invalid.isError, true);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

const anchorSearch = (regex, matchString) => [
  `local text search returns the observed anchor (${regex})`,
  async () => {
    const data = await call('localSearch', {
      path: path.join(fixture, 'math.ts'),
      matchString,
      regex,
      wholeWord: true,
      resultView: 'paginated',
    });
    // Minimal output drops single-page stats: the rows are the count.
    assert.equal(data.files.length, 1);
    assert.equal(data.files[0].matches.length, 1);
    assert.notEqual(data.isPartial, true);
    assert.equal(data.capped, undefined);
    assert.equal(data.files[0].matches[0].line, 2);
    assert.ok(data.files[0].matches[0].value.includes('function add'));
  },
];

async function emptySearchHint() {
  const query = {
    path: fixture,
    matchString: 'octocode-definitely-absent-token',
    regex: 'literal',
    resultView: 'files',
  };
  const data = await call('localSearch', query);
  assert.equal(data.matchCount, 0);
  // An empty row states how many files it read once, in filesScanned;
  // its one hint is a recovery, never the count again.
  assert.ok(data.filesScanned > 0);
  assert.equal(data.hints?.length, 1);
  assert.ok(data.hints[0].length <= 120);
  assert.doesNotMatch(data.hints[0], /No matches in|searched\./);
  const description = state.list.tools.find(
    tool => tool.name === 'localSearch'
  ).description;
  assert.match(description, /If empty: [^.]*(shorter|regex|includeIgnored)/i);
}

const briefSearch = brief =>
  call('localSearch', {
    path: fixture,
    matchString: 'add',
    pageSize: 1,
    ...brief,
  });

async function pagesCarryNoBrief() {
  // The brief is request-level (docs/TOOL_DATA_CONTRACT.md): a page
  // carries the scope and cursor, never mainGoal or reasoning.
  for (const brief of [
    {},
    { mainGoal: 'Trace the add helper.', reasoning: 'Find every caller.' },
  ]) {
    const data = await briefSearch(brief);
    assert.ok(data.next?.nextPage, 'no nextPage on a multi-file search');
    for (const field of ['mainGoal', 'reasoning'])
      assert.equal(
        data.next.nextPage.query.queries[0][field],
        undefined,
        field
      );
  }
}

async function fileDiscovery() {
  const data = await call('structureSearch', {
    operation: 'files',
    path: fixture,
    extensions: ['ts'],
    pageSize: 50,
  });
  assert.ok(
    listedPaths(data.files, data.path).some(file => file.endsWith('math.ts'))
  );
}

async function astSymbols() {
  const data = await call('astSearch', {
    operation: 'symbols',
    path: path.join(fixture, 'math.ts'),
    symbolName: 'add',
    kinds: ['function'],
  });
  // Minimal output drops the `operation` request echo.
  assert.equal(data.operation, undefined);
  // One declaration object; `docStartLine` marks the comment block above it.
  assert.deepEqual(data.symbols, [
    {
      symbolName: 'add',
      kind: 'function',
      line: 2,
      exported: true,
      docStartLine: 1,
    },
  ]);
  assert.equal(data.pagination?.totalItems ?? data.symbols.length, 1);
}

/** The local checks, in run order. */
export const localChecks = () => [
  [
    'local file read carries numbered source lines in structured content and its JSON text',
    numberedLocalRead,
  ],
  ['native MCP preserves ordered bulk query results', orderedBulkResults],
  [
    'localFetch line and byte chunks preserve selected views through real MCP',
    chunkedLocalFetch,
  ],
  anchorSearch('literal', 'add'),
  anchorSearch('rust', '\\badd\\b'),
  anchorSearch('pcre2', '(?<!\\w)add(?!\\w)'),
  ['empty local search returns one concise recovery hint', emptySearchHint],
  ['a page never carries the brief, sent or not', pagesCarryNoBrief],
  ['local file discovery positive', fileDiscovery],
  ['AST symbols identify the exported arithmetic declaration', astSymbols],
];
