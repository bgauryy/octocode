import { describe, expect, it } from 'vitest';

import { applyContentPagination } from '../../src/github/fileContentProcess.js';
import { buildGithubFetchContentFinalizer } from '../../src/tools/github_fetch_content/finalizer.js';
import type { GitHubFileContentApiResult } from '../../src/tools/github_fetch_content/types.js';
import type { FlatQueryResult } from '../../src/types/toolResults.js';

// A plain text file with no semantic boundaries so the char-limit path drives
// pagination deterministically. 3 chunks of 1000 chars over 3000 total.
const FULL = 'x'.repeat(3000);

function base(): GitHubFileContentApiResult {
  return {
    owner: 'octo',
    repo: 'engine',
    path: 'data.txt',
    content: FULL,
    branch: 'main',
    totalLines: 1,
    sourceChars: FULL.length,
    sourceBytes: FULL.length,
  } as GitHubFileContentApiResult;
}

describe('ghGetFileContent applyContentPagination — nextCharOffset is present', () => {
  it('a non-final chunk carries nextCharOffset === charOffset + charLength and hasMore:true', () => {
    const out = applyContentPagination(base(), 0, 1000);
    const pg = out.pagination!;
    expect(pg.hasMore).toBe(true);
    expect(pg.charOffset).toBe(0);
    expect(pg.charLength).toBe(1000);
    expect(pg.nextCharOffset).toBe(pg.charOffset! + pg.charLength!);
    expect(pg.totalChars).toBe(3000);
  });

  it('the final chunk has no nextCharOffset and hasMore:false', () => {
    const out = applyContentPagination(base(), 2000, 1000);
    const pg = out.pagination!;
    expect(pg.hasMore).toBe(false);
    expect(pg.nextCharOffset).toBeUndefined();
  });

  it('walking nextCharOffset reassembles the full file losslessly', () => {
    let offset = 0;
    let assembled = '';
    let guard = 0;
    for (;;) {
      const out = applyContentPagination(base(), offset, 1000);
      assembled += out.content ?? '';
      const pg = out.pagination!;
      if (!pg.hasMore || pg.nextCharOffset === undefined) break;
      offset = pg.nextCharOffset;
      if (++guard > 100) throw new Error('pagination did not terminate');
    }
    expect(assembled).toBe(FULL);
  });
});

describe('ghGetFileContent finalizer — next.continueChars fires from nextCharOffset', () => {
  it('emits a ready continuation carrying the materialized nextCharOffset', () => {
    const paginated = applyContentPagination(base(), 0, 1000);
    const query = {
      id: 'q1',
      owner: 'octo',
      repo: 'engine',
      branch: 'main',
      path: 'data.txt',
      minify: 'none' as const,
    };
    const result: FlatQueryResult = {
      id: 'q1',
      status: 'success',
      data: paginated as unknown as Record<string, unknown>,
    };
    const finalize = buildGithubFetchContentFinalizer<typeof query>();
    const out = finalize({
      queries: [query],
      results: [result],
    } as never);

    const file = (
      out.structuredContent.results as Array<{ files?: unknown[] }>
    )[0]?.files?.[0] as {
      next?: { continueChars?: { query: Record<string, unknown> } };
    };

    expect(file.next?.continueChars).toBeDefined();
    expect(file.next?.continueChars?.query.charOffset).toBe(
      paginated.pagination!.nextCharOffset
    );
    expect(file.next?.continueChars?.query.path).toBe('data.txt');
  });
});
