import { describe, expect, it, vi } from 'vitest';
import { fetchGitHubFileContentAPI } from '../../src/github/fileContent.js';
import { readFileEntry } from '../../src/tools/github_fetch_content/finalizer/entryParsers.js';
import { FileContentBulkQueryLocalSchema } from '../../src/tools/github_fetch_content/scheme.js';
import type { FileContentExecutionQuery } from '../../src/tools/github_fetch_content/types.js';

const fixture = vi.hoisted(() => ({ source: '' }));
vi.mock('../../src/github/fileContentRaw/cache.js', () => ({
  fetchCachedRawGitHubFileContent: vi.fn(async () => ({
    auth: 'fixture',
    rawResult: { data: { rawContent: fixture.source, branch: 'main' } },
  })),
}));
const base = {
  owner: 'octo',
  repo: 'fixture',
  path: 'source.txt',
  branch: 'main',
};
async function run(query: Record<string, unknown>) {
  expect(
    FileContentBulkQueryLocalSchema.safeParse({ queries: [query] }).success
  ).toBe(true);
  const result = await fetchGitHubFileContentAPI({
    ...query,
    noTimestamp: true,
  } as FileContentExecutionQuery);
  if (!('data' in result) || !result.data)
    throw new Error(JSON.stringify(result));
  return readFileEntry(result.data as Record<string, unknown>, query);
}
describe('GitHub selected-view secret scanner limits', () => {
  it.each(['none', 'standard', 'symbols'] as const)(
    '%s exposes a typed limit and executable source-line recovery',
    async minify => {
      fixture.source = 'safe first line\n'.padEnd(10_000_001, 'x');
      const result = await run({ ...base, minify, charLength: 7 });
      expect(result).toMatchObject({
        errorCode: 'contentSecurityLimit',
        terminalLimit: true,
        isPartial: true,
        partialReasons: ['security-selected-view-size-limit'],
        sourceChars: fixture.source.length,
        totalLines: 2,
      });
      expect(result.content).toBe('');
      expect(result.contentView).toBeUndefined();
      const next = result.next?.readBoundedLines;
      expect(next).toBeDefined();
      expect(next?.query).not.toHaveProperty('charOffset');
      const bounded = await run(next!.query);
      expect(bounded.content).toBe('safe first line');
      expect(bounded.sourceChars).toBe(fixture.source.length);
    }
  );
  it('does not emit an impossible continuation for an oversized single source line', async () => {
    fixture.source = 'x'.repeat(10_000_001);
    const result = await run({ ...base, minify: 'none', charLength: 7 });
    expect(result).toMatchObject({
      terminalLimit: true,
      errorCode: 'contentSecurityLimit',
    });
    expect(result.next?.readBoundedLines).toBeUndefined();
    expect(result.next?.continueChars).toBeUndefined();
  });
});
