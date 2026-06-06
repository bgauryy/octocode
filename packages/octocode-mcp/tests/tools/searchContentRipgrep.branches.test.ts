import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { RipgrepQuery } from '@octocodeai/octocode-core';
import { LOCAL_TOOL_ERROR_CODES } from '../../src/errors/localToolErrors.js';

const mocks = vi.hoisted(() => ({
  checkCommandAvailability: vi.fn(),
  executeRipgrepSearchInternal: vi.fn(),
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: mocks.checkCommandAvailability,
  getMissingCommandError: () => 'ripgrep not installed',
}));

vi.mock('../../src/tools/local_ripgrep/ripgrepExecutor.js', () => ({
  executeRipgrepSearchInternal: mocks.executeRipgrepSearchInternal,
}));

const { searchContentRipgrep } =
  await import('../../src/tools/local_ripgrep/searchContentRipgrep.js');

function makeRipgrepQuery(overrides: Partial<RipgrepQuery> = {}): RipgrepQuery {
  return {
    id: 'q-test',
    researchGoal: 'unit-test',
    reasoning: 'cover branches',
    pattern: 'foo',
    path: '/tmp',
    smartCase: true,
    matchContentLength: 200,
    filesPerPage: 10,
    filePageNumber: 1,
    matchesPerPage: 10,
    binaryFiles: 'without-match',
    includeStats: true,
    sort: 'path',
    showFileLastModified: false,
    ...overrides,
  };
}

describe('searchContentRipgrep — error handling', () => {
  beforeEach(() => {
    mocks.checkCommandAvailability.mockReset();
    mocks.executeRipgrepSearchInternal.mockReset();
  });

  it('returns a command-not-available error when ripgrep binary is missing', async () => {
    mocks.checkCommandAvailability.mockResolvedValueOnce({ available: false });

    const result = await searchContentRipgrep(makeRipgrepQuery());

    expect(result.status).toBe('error');
    expect(mocks.executeRipgrepSearchInternal).not.toHaveBeenCalled();
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

  it('returns the executor result unchanged when no exception is thrown', async () => {
    mocks.checkCommandAvailability.mockResolvedValueOnce({ available: true });
    mocks.executeRipgrepSearchInternal.mockResolvedValueOnce({
      status: 'empty',
      searchEngine: 'rg',
      hints: [],
    });

    const result = await searchContentRipgrep(makeRipgrepQuery());

    expect(result.status).toBe('empty');
  });
});
