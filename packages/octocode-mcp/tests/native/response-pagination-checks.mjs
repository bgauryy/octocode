// Cross-surface page walks for response-pagination.mjs: every surface (native
// CLI, npm CLI, MCP) must reach the same source evidence through
// its continuations, exactly once, and reject a stale snapshot.
import assert from 'node:assert/strict';
import path from 'node:path';

async function walk(call, firstInput, nextAt, label) {
  let input = firstInput;
  const pages = [];
  const seen = new Set();
  for (let index = 0; index < 100; index += 1) {
    const result = await call(input);
    assert.notEqual(
      result.isError,
      true,
      `${label}: ${JSON.stringify(result.structuredContent)}`
    );
    const data = result.structuredContent;
    assert.ok(data, `${label}: missing structured content`);
    pages.push(data);
    const { hasMore, next } = nextAt(data);
    if (!hasMore) {
      assert.equal(
        next,
        undefined,
        `${label}: terminal page has a continuation`
      );
      return pages;
    }
    assert.equal(next?.tool, 'localFetch', `${label}: continuation tool`);
    assert.ok(
      next.query && typeof next.query === 'object',
      `${label}: missing continuation query`
    );
    if (next.query.responseSnapshot !== undefined) {
      assert.deepEqual(
        Object.keys(next.query),
        ['responseSnapshot'],
        `${label}: cursor-only continuation`
      );
    }
    const key = JSON.stringify(next.query);
    assert.ok(!seen.has(key), `${label}: repeated continuation`);
    seen.add(key);
    input = next.query;
  }
  return assert.fail(`${label}: continuation did not terminate`);
}

export async function checkToolPages(surfaceCalls, source, sourceText) {
  const firstInput = {
    queries: [{ path: source, offset: 0, length: 5, unit: 'lines' }],
  };
  const expected = sourceText
    .split('\n')
    .slice(0, -1)
    .map((line, index) => `${index + 1}\t${line}\n`)
    .join('');
  const walks = [];
  for (const [name, call] of surfaceCalls) {
    const pages = await walk(
      call,
      firstInput,
      data => {
        const row = data.results?.[0];
        assert.equal(row?.index, 0, `${name}: row index`);
        assert.equal(
          row?.data?.path,
          path.basename(source),
          `${name}: source identity`
        );
        const pagination = row.data.pagination;
        return {
          hasMore: pagination?.hasMore,
          next: row.data.next?.continueWindow,
        };
      },
      `${name} tool page`
    );
    assert.ok(pages.length > 1, `${name}: expected multiple tool pages`);
    const content = pages.map(page => page.results[0].data.content).join('');
    assert.equal(
      content,
      expected,
      `${name}: skipped, reordered or duplicated source lines`
    );
    const continuation = pages[0].results[0].data.next.continueWindow;
    const stale = await call({
      ...continuation.query,
      queries: continuation.query.queries.map(query => ({
        ...query,
        snapshot: '0'.repeat(64),
      })),
    });
    const failed = stale.structuredContent?.results?.[0];
    assert.equal(
      failed?.data?.errorCode,
      'staleSnapshot',
      `${name}: stale source snapshot`
    );
    assert.equal(failed?.status, 'error', `${name}: stale source status`);
    assert.equal(
      failed.data.next?.restart?.tool,
      'localFetch',
      `${name}: restart lead`
    );
    walks.push({ name, content, count: pages.length });
  }
  assert.equal(
    new Set(walks.map(entry => entry.count)).size,
    1,
    'tool page counts differ'
  );
  return walks[0].count;
}

export async function checkResponsePages(surfaceCalls, source) {
  const queries = [1, 2, 1].map(line => ({
    path: source,
    ranges: [`${line}-${line}`],
  }));
  const firstInput = { queries, responseLength: 300, responseScope: 'rows' };
  const expected = queries.map(({ ranges }) => {
    const line = Number(ranges[0].split('-')[0]);
    return `${line}\talpha 😀 beta\n`;
  });
  const walks = [];
  for (const [name, call] of surfaceCalls) {
    const pages = await walk(
      call,
      firstInput,
      data => {
        const pagination = data.responsePagination;
        assert.equal(pagination?.scope, 'rows', `${name}: response page scope`);
        return { hasMore: pagination.hasMore, next: pagination.next };
      },
      `${name} response page`
    );
    assert.ok(pages.length > 1, `${name}: expected multiple response pages`);
    const rows = pages.flatMap(page => page.results ?? []);
    assert.deepEqual(
      rows.map(row => row.index),
      [0, 1, 2],
      `${name}: row order and duplicates`
    );
    assert.deepEqual(
      rows.map(row => row.data.content),
      expected,
      `${name}: source evidence`
    );
    const continuation = pages[0].responsePagination.next;
    const stale = await call({
      ...continuation.query,
      responseSnapshot: 'response-v1:stale',
    });
    assert.equal(
      stale.structuredContent?.errorCode,
      'responseSnapshotExpired',
      `${name}: stale response snapshot`
    );
    walks.push({ name, rows, count: pages.length });
  }
  assert.equal(
    new Set(walks.map(entry => entry.count)).size,
    1,
    'response page counts differ'
  );
  assert.deepEqual(
    walks.slice(1).map(entry => entry.rows),
    walks.slice(1).map(() => walks[0].rows)
  );
  return walks[0].count;
}
