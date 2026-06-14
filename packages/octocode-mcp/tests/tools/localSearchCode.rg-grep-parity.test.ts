import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../../octocode-tools-core/src/utils/exec/safe.js', () => ({
  safeExec: vi.fn(),
}));

vi.mock('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js', () => ({
  resolveRipgrepBinary: vi.fn().mockReturnValue('rg'),
}));

vi.mock('octocode-security/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn().mockReturnValue({
      isValid: true,
      sanitizedPath: '/workspace',
    }),
  },
}));

vi.mock('fs', () => ({
  promises: {
    stat: vi
      .fn()
      .mockResolvedValue({ mtime: new Date('2024-01-01T00:00:00Z') }),
  },
}));

vi.mock('../../../octocode-tools-core/src/hints/index.js', () => ({
  getHints: vi.fn().mockReturnValue(['empty hint']),
}));

vi.mock('@octocodeai/octocode-core/schemas/runtime', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('@octocodeai/octocode-core/schemas/runtime')
    >();
  return {
    ...actual,
    validateRipgrepQuery: vi.fn().mockReturnValue({
      isValid: true,
      errors: [],
      warnings: [],
    }),
  };
});

import { safeExec } from '../../../octocode-tools-core/src/utils/exec/safe.js';
import { executeRipgrepSearchInternal } from '../../../octocode-tools-core/src/tools/local_ripgrep/ripgrepExecutor.js';
import { executeGrepFallbackSearch } from '../../../octocode-tools-core/src/tools/local_ripgrep/grepFallbackExecutor.js';

const mockSafeExec = vi.mocked(safeExec);

const query = {
  id: 'rg-grep-parity',
  researchGoal: 'unit-test',
  reasoning: 'compare ripgrep and grep fallback output shape',
  keywords: 'needle',
  path: '/workspace',
  fixedString: true,
  matchContentLength: 200,
  itemsPerPage: 10,
  page: 1,
  maxMatchesPerFile: 10,
};

describe('localSearchCode ripgrep vs grep fallback parity', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns the same basic file, line, and value shape for a simple match', async () => {
    mockSafeExec
      .mockResolvedValueOnce({
        success: true,
        code: 0,
        stdout:
          JSON.stringify({
            type: 'match',
            data: {
              path: { text: '/workspace/a.ts' },
              lines: { text: 'const needle = true;\n' },
              line_number: 3,
              absolute_offset: 0,
              submatches: [{ match: { text: 'needle' }, start: 6, end: 12 }],
            },
          }) + '\n',
        stderr: '',
      })
      .mockResolvedValueOnce({
        success: true,
        code: 0,
        stdout: '/workspace/a.ts:3:const needle = true;\n',
        stderr: '',
      });

    const rgResult = await executeRipgrepSearchInternal(query as never);
    const grepResult = await executeGrepFallbackSearch(query as never);

    expect(rgResult.files?.[0]).toMatchObject({
      path: '/workspace/a.ts',
      matchCount: 1,
      matches: [{ line: 3, value: 'const needle = true;' }],
    });
    expect(grepResult.files?.[0]).toMatchObject({
      path: '/workspace/a.ts',
      matchCount: 1,
      matches: [{ line: 3, value: 'const needle = true;' }],
    });
  });
});
