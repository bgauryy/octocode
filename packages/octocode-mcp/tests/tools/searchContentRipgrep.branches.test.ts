import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { RipgrepQuery } from '../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../../octocode-tools-core/src/errors/localToolErrors.js';

const mocks = vi.hoisted(() => ({
  checkCommandAvailability: vi.fn(),
  executeRipgrepSearchInternal: vi.fn(),
  executeGrepFallbackSearch: vi.fn(),
}));

vi.mock(
  '../../../octocode-tools-core/src/utils/exec/commandAvailability.js',
  () => ({
    checkCommandAvailability: mocks.checkCommandAvailability,
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_ripgrep/ripgrepExecutor.js',
  () => ({
    executeRipgrepSearchInternal: mocks.executeRipgrepSearchInternal,
  })
);

vi.mock(
  '../../../octocode-tools-core/src/tools/local_ripgrep/grepFallbackExecutor.js',
  () => ({
    executeGrepFallbackSearch: mocks.executeGrepFallbackSearch,
  })
);

const { searchContentRipgrep } =
  await import('../../../octocode-tools-core/src/tools/local_ripgrep/searchContentRipgrep.js');

function makeRipgrepQuery(overrides: Partial<RipgrepQuery> = {}): RipgrepQuery {
  return {
    id: 'q-test',
    researchGoal: 'unit-test',
    reasoning: 'cover branches',
    keywords: 'foo',
    path: '/tmp',
    matchContentLength: 200,
    itemsPerPage: 10,
    page: 1,
    maxMatchesPerFile: 10,
    sort: 'path',
    ...overrides,
  };
}

describe('searchContentRipgrep — error handling', () => {
  beforeEach(() => {
    mocks.checkCommandAvailability.mockReset();
    mocks.executeRipgrepSearchInternal.mockReset();
    mocks.executeGrepFallbackSearch.mockReset();
  });

  it('falls back to grep when the ripgrep binary is missing', async () => {
    mocks.checkCommandAvailability.mockResolvedValueOnce({
      available: false,
      error: 'bundled rg missing',
    });
    mocks.executeGrepFallbackSearch.mockResolvedValueOnce({
      searchEngine: 'grep',
      warnings: ['Using grep fallback (ripgrep unavailable)'],
      files: [],
    });

    const query = makeRipgrepQuery();
    const result = await searchContentRipgrep(query);

    expect(result.searchEngine).toBe('grep');
    expect(mocks.executeRipgrepSearchInternal).not.toHaveBeenCalled();
    expect(mocks.executeGrepFallbackSearch).toHaveBeenCalledWith(
      expect.objectContaining({ keywords: query.keywords, path: query.path }),
      'bundled rg missing'
    );
  });

  it('maps "Output size limit exceeded" exceptions to an OUTPUT_TOO_LARGE result with workflow hints', async () => {
    mocks.checkCommandAvailability.mockResolvedValueOnce({ available: true });
    mocks.executeRipgrepSearchInternal.mockRejectedValueOnce(
      new Error('Output size limit exceeded (10485760 bytes)')
    );

    const result = await searchContentRipgrep(makeRipgrepQuery());

    expect(result.status).toBe('error');
    if (result.status === 'error') {
      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE);
      expect(result.error).toMatch(/Output size limit exceeded/);
      expect(result.hints?.join('\n')).toMatch(/filesOnly|Strategy/);
    }
  });

  it('falls through to the generic error path for unrelated exceptions (covers line 60 catchall)', async () => {
    mocks.checkCommandAvailability.mockResolvedValueOnce({ available: true });
    mocks.executeRipgrepSearchInternal.mockRejectedValueOnce(
      new Error('something else entirely went wrong')
    );

    const result = await searchContentRipgrep(makeRipgrepQuery());

    expect(result.status).toBe('error');
    if (result.status === 'error') {
      expect(result.errorCode).not.toBe(
        LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE
      );
      expect(result.error).toMatch(/something else entirely went wrong/);
    }
  });

  it('uses ripgrep and never calls grep fallback when ripgrep is available', async () => {
    mocks.checkCommandAvailability.mockResolvedValueOnce({ available: true });
    mocks.executeRipgrepSearchInternal.mockResolvedValueOnce({
      searchEngine: 'rg',
      files: [
        {
          path: '/tmp/a.ts',
          matchCount: 1,
          matches: [{ line: 2, column: 0, value: 'const foo = true;' }],
        },
      ],
    });

    const result = await searchContentRipgrep(makeRipgrepQuery());

    expect(result.searchEngine).toBe('rg');
    expect(mocks.executeRipgrepSearchInternal).toHaveBeenCalledOnce();
    expect(mocks.executeGrepFallbackSearch).not.toHaveBeenCalled();
    expect(result.files?.[0]?.matches![0]).toMatchObject({
      line: 2,
      value: 'const foo = true;',
    });
  });
});
