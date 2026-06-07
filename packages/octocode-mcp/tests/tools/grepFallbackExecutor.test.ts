import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi.fn(),
}));

vi.mock('octocode-security-utils/pathValidator', () => ({
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

vi.mock('../../src/hints/index.js', () => ({
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

import { safeExec } from '../../src/utils/exec/safe.js';
import { executeGrepFallbackSearch } from '../../src/tools/local_ripgrep/grepFallbackExecutor.js';

const mockSafeExec = vi.mocked(safeExec);

const expectedSimpleMatchShape = {
  path: '/workspace/a.ts',
  matchCount: 1,
  matches: [{ line: 3, value: 'const needle = true;' }],
};

const baseQuery = {
  id: 'grep-fallback-test',
  researchGoal: 'unit-test',
  reasoning: 'cover grep fallback',
  pattern: 'needle',
  path: '/workspace',
  fixedString: true,
  matchContentLength: 200,
  itemsPerPage: 10,
  page: 1,
  matchesPerFile: 10,
  binaryFiles: 'without-match',
  smartCase: true,
};

describe('executeGrepFallbackSearch', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '/workspace/a.ts:3:const needle = true;\n',
      stderr: '',
    });
  });

  it('runs grep and returns localSearchCode-shaped matches', async () => {
    const result = await executeGrepFallbackSearch(
      baseQuery as never,
      'bundled rg missing'
    );

    expect(mockSafeExec).toHaveBeenCalledWith(
      'grep',
      expect.arrayContaining(['-R', '-H', '-F', '-i', '-I', '-n'])
    );
    expect(result.searchEngine).toBe('grep');
    expect(result.warnings?.join('\n')).toContain('Using grep fallback');
    expect(result.files?.[0]).toMatchObject(expectedSimpleMatchShape);
  });

  it('matches ripgrep basic shape for a simple path:line:value hit', async () => {
    const result = await executeGrepFallbackSearch(baseQuery as never);

    expect(result).toMatchObject({
      searchEngine: 'grep',
      files: [expectedSimpleMatchShape],
    });
  });

  it('returns empty with fallback hints when grep exits 1', async () => {
    mockSafeExec.mockResolvedValueOnce({
      success: false,
      code: 1,
      stdout: '',
      stderr: '',
    });

    const result = await executeGrepFallbackSearch(baseQuery as never);

    expect(result.status).toBe('empty');
    expect(result.searchEngine).toBe('grep');
    expect(result.hints?.join('\n')).toContain('ripgrep');
  });
});
