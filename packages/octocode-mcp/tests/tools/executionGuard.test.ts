import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../../octocode-tools-core/src/tools/utils.js', () => ({
  handleCatchError: vi
    .fn()
    .mockReturnValue({ status: 'error', error: 'guarded failure' }),
}));

import { handleCatchError } from '../../../octocode-tools-core/src/tools/utils.js';
import { executeWithToolBoundary } from '../../../octocode-tools-core/src/tools/executionGuard.js';

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
      toolName: 'ghCloneRepo',
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
      'ghCloneRepo'
    );
    expect(result).toEqual({ status: 'error', error: 'guarded failure' });
  });
});
