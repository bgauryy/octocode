// Row, structured, inventory and whole-response pagination checks for
// stdio.acceptance.mjs: every continuation reaches each line, file or
// character exactly once.
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import {
  byCodeUnit,
  call,
  fixture,
  invoke,
  listedPaths,
  pages,
} from './stdio-harness.mjs';

// A numbered `localFetch` content line is `<line>\t<text>`.
const withoutGutter = content => content.replace(/^\d+\t/gmu, '');

async function rowWindows() {
  const source = path.join(fixture, 'math.ts');
  const args = {
    queries: [
      {
        reasoning:
          'Exercise localFetch row windows through built stdio acceptance.',
        path: source,
      },
    ],
  };
  const full = await invoke('localFetch', args);
  const fullRow = full.structuredContent.results[0].data;
  assert.equal(fullRow.next, undefined);
  const sourceText = await readFile(source, 'utf8');
  assert.equal(withoutGutter(fullRow.content), sourceText);
  let current = await invoke('localFetch', {
    ...args,
    responseLength: 150,
  });
  let content = '';
  const lines = [];
  let count = 0;
  while (true) {
    assert.ok(count++ < 30);
    // The row is complete on every page: its own window carries the
    // continuation; no text window empties `results`.
    assert.equal(current.structuredContent.results.length, 1);
    assert.equal(current.structuredContent.responseWindow, undefined);
    const row = current.structuredContent.results[0].data;
    assert.equal(row.totalLines, fullRow.totalLines);
    // A window cut to the response share is verbatim source whose
    // sourceLineRanges state its lines; a numbered window states them in
    // its gutter (TOOL_DATA_CONTRACT.md "Numbered source content").
    const returned = row.sourceLineRanges
      ? row.sourceLineRanges.flatMap(({ line, endLine = line }) =>
          Array.from({ length: endLine - line + 1 }, (_, at) => line + at)
        )
      : [...row.content.matchAll(/^(\d+)\t/gmu)].map(([, line]) =>
          Number(line)
        );
    assert.ok(returned.length > 0, 'a window returned no source line');
    lines.push(...returned);
    content += row.sourceLineRanges ? row.content : withoutGutter(row.content);
    const next = row.next?.continueWindow;
    assert.equal(row.pagination.hasMore, next !== undefined);
    if (!next) break;
    assert.equal(next.tool, 'localFetch');
    // The continuation resumes where this page ends, in its own unit: the
    // page's nextOffset for a byte window, else the 0-based line after the
    // last line this page returned.
    const [query] = next.query.queries;
    assert.equal(
      query.offset,
      query.unit === 'bytes' ? row.pagination.nextOffset : returned.at(-1)
    );
    current = await invoke(next.tool, next.query);
  }
  assert.ok(count > 1, 'responseLength:150 did not window the read');
  // Every source line arrives once, in order, with its exact text; only a
  // byte window that splits a line names that line on both sides.
  assert.deepEqual(
    lines.filter((line, index) => line !== lines[index - 1]),
    Array.from({ length: fullRow.totalLines }, (_, index) => index + 1)
  );
  assert.equal(content, sourceText);
}

async function textWindows() {
  const args = { operation: 'files', path: fixture, extensions: ['ts'] };
  const full = await invoke('structureSearch', { queries: [args] });
  let current = await invoke('structureSearch', {
    queries: [args],
    responseLength: 150,
    responseScope: 'structured',
  });
  let text = '';
  let count = 0;
  assert.ok(full.structuredContent.results.length > 0);
  while (true) {
    assert.ok(count++ < 30);
    assert.deepEqual(current.structuredContent.results, []);
    // The window slices the serialized structuredContent of the whole
    // response: fragments carry no page header and concatenate as is.
    text += current.structuredContent.responseWindow;
    const next = current.structuredContent.responsePagination?.next;
    if (!next) break;
    current = await invoke(next.tool, next.query);
  }
  assert.ok(count > 1);
  assert.deepEqual(JSON.parse(text), full.structuredContent);
}

async function fileInventoryPages() {
  const query = { operation: 'files', path: fixture, extensions: ['ts'] };
  const full = await call('structureSearch', { ...query, pageSize: 50 });
  const first = await call('structureSearch', { ...query, pageSize: 1 });
  const paged = await pages(first, 'nextPage', data =>
    listedPaths(data.files, data.path)
  );
  assert.ok(paged.count > 1);
  assert.deepEqual(
    paged.rows.toSorted(byCodeUnit),
    listedPaths(full.files, full.path).toSorted(byCodeUnit)
  );
}

// Read every page of one saved response by its cursor alone.
async function drain(first) {
  let current = first;
  let rendered = '';
  let count = 0;
  for (;;) {
    assert.ok(++count < 100);
    // The last page has nothing left to page: it carries no responsePagination.
    const pagination = current.structuredContent.responsePagination;
    if (pagination) {
      assert.notEqual(pagination.restart, true);
      if (pagination.next)
        assert.deepEqual(Object.keys(pagination.next.query), [
          'responseSnapshot',
        ]);
    } else assert.ok(count > 1, 'the first page lost its responsePagination');
    // A structured window carries its slice; a row page is its JSON (the
    // MCP text block, asserted by invoke to equal structuredContent).
    rendered +=
      current.structuredContent.responseWindow ?? current.content[0].text;
    if (!pagination?.next) break;
    current = await invoke(pagination.next.tool, pagination.next.query);
  }
  assert.ok(count > 1);
  return rendered;
}

const once = (rendered, index, value = index) =>
  assert.equal(
    rendered.split(`export const value = ${value};`).length - 1,
    1,
    `value ${value} of source ${index} must arrive once across the pages`
  );

/** Captures, replays after an edit, and recaptures one paged response. */
async function replayAndRecapture(files, page1) {
  const first = await page1();
  const before = first.structuredContent.responsePagination;
  assert.ok(before.next);
  const original = await drain(first);
  for (const index of [1, 2, 3]) once(original, index);

  // A response token names one captured response: after an edit its
  // pages replay the captured evidence, never re-reading the files.
  const captured = await page1();
  assert.equal(
    captured.structuredContent.responsePagination.next.query.responseSnapshot,
    before.next.query.responseSnapshot
  );
  await writeFile(files[0], 'export const value = 100;\n');
  await writeFile(files[2], 'export const value = 300;\n');
  const replayed = await drain(captured);
  for (const index of [1, 2, 3]) once(replayed, index);
  assert.doesNotMatch(replayed, /value = (100|300);/);

  // A new query without response fields reads the current source and
  // captures a new response.
  const fresh = await call('localFetch', { path: files[2] });
  assert.equal(fresh.content, '1\texport const value = 300;\n');
  const recaptured = await page1();
  const recapturedPagination = recaptured.structuredContent.responsePagination;
  assert.notEqual(
    recapturedPagination.next.query.responseSnapshot,
    before.next.query.responseSnapshot
  );
  const current = await drain(recaptured);
  once(current, 1, 100);
  once(current, 2);
  once(current, 3, 300);
}

const snapshotReplay = scope => [
  `whole-response ${scope} pages replay one captured response; a new query sees an edit`,
  async () => {
    const parent = fixture;
    await mkdir(parent, { recursive: true });
    const directory = await mkdtemp(path.join(parent, 'mcp-snapshot-'));
    // Three small reads: no row is split, so the response pages between them.
    const files = [1, 2, 3].map(index =>
      path.join(directory, `source-${index}.ts`)
    );
    try {
      for (const [index, file] of files.entries())
        await writeFile(file, `export const value = ${index + 1};\n`);
      const page1 = () =>
        invoke('localFetch', {
          queries: files.map(file => ({
            reasoning:
              'Exercise localFetch response pagination through built stdio acceptance.',
            path: file,
          })),
          responseLength: 300,
          responseScope: scope,
        });
      await replayAndRecapture(files, page1);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  },
];

/** The pagination checks, in run order (skipped by --quick). */
export const paginationChecks = () => [
  [
    'localFetch rows page by the response window and continueWindow reaches every line once',
    rowWindows,
  ],
  [
    'structured response pagination windows the structuredContent JSON and reconstructs every character',
    textWindows,
  ],
  [
    'file pagination executes continuations and preserves full inventory',
    fileInventoryPages,
  ],
  snapshotReplay('rows'),
  snapshotReplay('structured'),
];
