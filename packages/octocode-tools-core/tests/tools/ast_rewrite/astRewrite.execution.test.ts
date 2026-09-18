import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  validateToolPath: vi.fn(),
  runAstRewrite: vi.fn(),
}));

vi.mock('../../../src/utils/file/toolHelpers.js', () => ({
  validateToolPath: mocks.validateToolPath,
}));

vi.mock('../../../src/tools/ast_rewrite/index.js', () => ({
  runAstRewrite: mocks.runAstRewrite,
}));

import { executeAstRewrite } from '../../../src/tools/ast_rewrite/execution.js';

describe('executeAstRewrite path policy', () => {
  beforeEach(() => {
    mocks.validateToolPath.mockReset();
    mocks.runAstRewrite.mockReset();
  });

  it('returns the shared path-policy error without invoking ast-grep', async () => {
    mocks.validateToolPath.mockReturnValue({
      isValid: false,
      errorResult: {
        status: 'error',
        errorCode: 'path.validation.failed',
        error: 'denied by allowed roots',
      },
    });

    const response = await executeAstRewrite({
      queries: [
        {
          path: '/private/secret',
          langType: 'ts',
          ruleKind: 'pattern',
          pattern: 'oldCall($A)',
          rewrite: 'newCall($A)',
        },
      ],
    });
    const row = (response.structuredContent as { results: unknown[] })
      .results[0];

    expect(row).toMatchObject({
      status: 'error',
      data: {
        errorCode: 'path.validation.failed',
      },
    });
    expect(mocks.validateToolPath).toHaveBeenCalledWith(
      expect.objectContaining({ path: '/private/secret' }),
      'astRewrite'
    );
    expect(mocks.runAstRewrite).not.toHaveBeenCalled();
  });

  it('passes only the sanitized authorized path to the rewrite runtime', async () => {
    mocks.validateToolPath.mockReturnValue({
      isValid: true,
      sanitizedPath: '/workspace/real-source',
    });
    mocks.runAstRewrite.mockResolvedValue({
      status: 'empty',
      operation: 'rewrite',
      mode: 'preview',
      root: '/workspace/real-source',
      executable: { path: '/usr/bin/ast-grep', version: '0.40.1' },
      totalMatches: 0,
      affectedFiles: 0,
      matches: [],
      files: [],
      complete: true,
      isPartial: false,
    });

    await executeAstRewrite({
      queries: [
        {
          path: '/workspace/link',
          langType: 'ts',
          ruleKind: 'pattern',
          pattern: 'oldCall($A)',
          rewrite: 'newCall($A)',
        },
      ],
    });

    expect(mocks.runAstRewrite).toHaveBeenCalledWith(
      expect.objectContaining({ path: '/workspace/real-source' }),
      expect.objectContaining({ allowApply: expect.any(Boolean) })
    );
  });
});
