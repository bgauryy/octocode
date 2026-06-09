import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../src/tools/utils.js', () => ({
  handleCatchError: vi
    .fn()
    .mockReturnValue({ status: 'error', error: 'guarded failure' }),
}));

import { handleCatchError } from '../../src/tools/utils.js';
import { executeWithToolBoundary } from '../../src/tools/executionGuard.js';

describe('executeWithToolBoundary', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns successful execution result unchanged', async () => {
    const result = await executeWithToolBoundary({
      toolName: 'localGetFileContent',
      query: { researchGoal: 'test', reasoning: 'test' },
      execute: async () => ({ data: { content: 'ok' } }),
    });

    expect(result).toEqual({ data: { content: 'ok' } });
    expect(handleCatchError).not.toHaveBeenCalled();
  });

  it('converts thrown errors via handleCatchError', async () => {
    const query = { researchGoal: 'test', reasoning: 'test' };

    const result = await executeWithToolBoundary({
      toolName: 'githubCloneRepo',
      query,
      contextMessage: 'Clone failed for owner/repo',
      execute: async () => {
        throw new Error('boom');
      },
    });

    expect(handleCatchError).toHaveBeenCalledWith(
      expect.any(Error),
      query,
      'Clone failed for owner/repo',
      'githubCloneRepo'
    );
    expect(result).toEqual({ status: 'error', error: 'guarded failure' });
  });
});
