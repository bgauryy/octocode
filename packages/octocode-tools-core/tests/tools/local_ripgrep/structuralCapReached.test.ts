import { describe, expect, it, vi, beforeEach } from 'vitest';

const mocks = vi.hoisted(() => ({
  readFile: vi.fn(),
  stat: vi.fn(),
  validateToolPath: vi.fn(),
  structuralSearch: vi.fn(),
  structuralSearchFiles: vi.fn(),
}));

vi.mock('node:fs/promises', () => ({
  readFile: mocks.readFile,
  stat: mocks.stat,
}));

vi.mock('../../../src/utils/file/toolHelpers.js', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('../../../src/utils/file/toolHelpers.js')
    >();
  return {
    ...actual,
    validateToolPath: mocks.validateToolPath,
  };
});

vi.mock('../../../src/utils/contextUtils.js', () => ({
  contextUtils: {
    structuralSearch: mocks.structuralSearch,
    structuralSearchFiles: mocks.structuralSearchFiles,
  },
}));

const { searchContentStructural } =
  await import('../../../src/tools/local_ripgrep/structuralSearch.js');

function makeQuery(overrides: Record<string, unknown> = {}) {
  return {
    id: 'cap-test',
    researchGoal: 'unit-test',
    reasoning: 'validate cap truncation honesty',
    path: '/repo',
    mode: 'structural' as const,
    pattern: 'target($X)',
    maxFiles: 2,
    ...overrides,
  };
}

function fileResult(path: string) {
  return {
    path,
    matches: [
      {
        startLine: 1,
        endLine: 1,
        startCol: 1,
        endCol: 14,
        text: 'target(value)',
        metavars: { X: ['value'] },
      },
    ],
  };
}

describe('structural maxFiles cap honesty (capReached)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.validateToolPath.mockReturnValue({
      isValid: true,
      sanitizedPath: '/repo',
    });
    mocks.stat.mockRejectedValue(new Error('not found in unit test'));
    mocks.readFile.mockResolvedValue('');
  });

  it('sets stats.capReached when native collection found a candidate beyond maxFiles', async () => {
    // The native extra-candidate probe, rather than the returned file count,
    // establishes whether the scan is incomplete.
    mocks.structuralSearchFiles.mockReturnValue({
      status: 'ok',
      diagnostics: [],
      scanTruncated: true,
      files: [fileResult('/repo/a.ts'), fileResult('/repo/b.ts')],
      totalMatches: 2,
      parsedFiles: 2,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
    });

    const result = await searchContentStructural(makeQuery({ maxFiles: 2 }));
    expect(result.stats?.capReached).toBe(true);
  });

  it('preserves native cap evidence when a scanned candidate was unreadable', async () => {
    mocks.structuralSearchFiles.mockReturnValue({
      status: 'partial',
      diagnostics: [],
      scanTruncated: true,
      files: [fileResult('/repo/a.ts')],
      totalMatches: 1,
      parsedFiles: 1,
      skippedByPreFilter: 0,
      skippedUnreadable: 1,
      skippedLarge: 0,
      warnings: [],
    });

    const result = await searchContentStructural(makeQuery({ maxFiles: 2 }));
    expect(result.stats?.capReached).toBe(true);
  });

  it('omits capReached when the scan finished under the cap', async () => {
    mocks.structuralSearchFiles.mockReturnValue({
      status: 'ok',
      diagnostics: [],
      scanTruncated: false,
      files: [fileResult('/repo/a.ts')],
      totalMatches: 1,
      parsedFiles: 1,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
    });

    const result = await searchContentStructural(makeQuery({ maxFiles: 10 }));
    expect(result.stats?.capReached).toBeUndefined();
  });
});
