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
  const actual = await importOriginal<
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

const { searchContentStructural } = await import(
  '../../../src/tools/local_ripgrep/structuralSearch.js'
);

function makeQuery(overrides: Record<string, unknown> = {}) {
  return {
    id: 'structural-test',
    researchGoal: 'unit-test',
    reasoning: 'validate structural search behavior',
    path: '/repo',
    mode: 'structural' as const,
    pattern: 'target($X)',
    maxFiles: 10,
    ...overrides,
  };
}

describe('searchContentStructural', () => {
  beforeEach(() => {
    vi.clearAllMocks();
      mocks.validateToolPath.mockReturnValue({
        isValid: true,
        sanitizedPath: '/repo',
      });
      mocks.stat.mockRejectedValue(new Error('not found in unit test'));
      mocks.readFile.mockResolvedValue('');
      mocks.structuralSearch.mockReturnValue([]);
      mocks.structuralSearchFiles.mockReturnValue({
        files: [],
        totalMatches: 0,
      parsedFiles: 0,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
    });
  });

  it('delegates filesystem traversal, reads, and AST matching to native Rust', async () => {
    mocks.structuralSearchFiles.mockReturnValue({
      files: [
        {
          path: '/repo/a.ts',
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
        },
      ],
      totalMatches: 1,
      parsedFiles: 1,
      skippedByPreFilter: 2,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: ['Pre-filter skipped parsing 2 file(s); parsed 1.'],
    });

    const result = await searchContentStructural(makeQuery());

    expect(mocks.structuralSearchFiles).toHaveBeenCalledWith({
      path: '/repo',
      pattern: 'target($X)',
      rule: undefined,
      maxFiles: 10,
      maxFileBytes: 1_000_000,
    });
    const nativeOptions = mocks.structuralSearchFiles.mock.calls[0]?.[0] ?? {};
    // No directories are excluded by default — structural search must not
    // silently skip node_modules/build/dist (see DEFAULT_STRUCTURAL_EXCLUDE_DIRS).
    expect(nativeOptions).not.toHaveProperty('excludeDir');
    expect(nativeOptions).not.toHaveProperty('include');
    expect(result.searchEngine).toBe('structural');
    expect(result.files).toHaveLength(1);
    expect(result.warnings?.join('\n')).toContain('Pre-filter skipped');
    expect(result.files[0]?.matches?.[0]).toMatchObject({
      endLine: 1,
      endColumn: 14,
      metavars: { X: ['value'] },
    });
    // Successful structural searches carry evidence in structured fields; no
    // next-step hint boilerplate is emitted on success.
      expect(result.hints).toBeUndefined();
    });

    it('uses the single-file native matcher for structural file paths', async () => {
      mocks.validateToolPath.mockReturnValue({
        isValid: true,
        sanitizedPath: '/repo/a.ts',
      });
      mocks.stat.mockResolvedValue({
        isFile: () => true,
      });
      mocks.readFile.mockResolvedValue('target(value);\n');
      mocks.structuralSearch.mockReturnValue([
        {
          startLine: 1,
          endLine: 1,
          startCol: 1,
          endCol: 14,
          text: 'target(value)',
          metavars: { X: ['value'] },
        },
      ]);

      const result = await searchContentStructural(makeQuery({ path: '/repo/a.ts' }));

      expect(mocks.structuralSearchFiles).not.toHaveBeenCalled();
      expect(mocks.structuralSearch).toHaveBeenCalledWith(
        'target(value);\n',
        '/repo/a.ts',
        'target($X)',
        undefined
      );
      expect(result.searchEngine).toBe('structural');
      expect(result.files[0]?.matches?.[0]).toMatchObject({
        line: 1,
        endLine: 1,
        column: 1,
        endColumn: 14,
        metavars: { X: ['value'] },
      });
    });

    it('passes caller include and excludeDir options to native Rust', async () => {
    await searchContentStructural(
      makeQuery({ include: ['*.tsx'], excludeDir: ['vendor'], maxFiles: 3 })
    );

    expect(mocks.structuralSearchFiles).toHaveBeenCalledWith(
      expect.objectContaining({
        include: ['*.tsx'],
        excludeDir: ['vendor'],
        maxFiles: 3,
      })
    );
  });

  it('appends zero-match ergonomics guidance via the typed warnings channel', async () => {
    // Default beforeEach mock returns no files / totalMatches 0.
    const result = await searchContentStructural(makeQuery());

    expect(result.files).toHaveLength(0);
    expect(result.warnings?.join('\n')).toContain('0 structural matches');
    expect(result.warnings?.join('\n')).toContain('$$$BODY');
    expect(result.warnings?.join('\n')).toContain('YAML `rule`');
    // Guidance is a typed warning, never a hint.
    expect(result.hints).toBeUndefined();
  });

  it('combines native warnings with the zero-match guidance', async () => {
    mocks.structuralSearchFiles.mockReturnValue({
      files: [],
      totalMatches: 0,
      parsedFiles: 1,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: ['Pre-filter skipped parsing 3 file(s); parsed 1.'],
    });

    const result = await searchContentStructural(makeQuery());

    const text = result.warnings?.join('\n') ?? '';
    expect(text).toContain('Pre-filter skipped');
    expect(text).toContain('0 structural matches');
  });

  it('surfaces native structural errors with pattern remediation guidance', async () => {
    mocks.structuralSearchFiles.mockImplementation(() => {
      throw new Error('invalid structural pattern: bad');
    });

    const result = await searchContentStructural(makeQuery({ langType: 'py' }));

    expect(result.status).toBe('error');
    expect(result.error).toContain('Invalid structural pattern');
    // Remediation is appended to the error message so a parse error tells the
    // agent how to fix it.
    expect(result.error).toContain('match a complete node');
    expect(result.error).toContain('$$$BODY');
    expect(result.error).toContain('valid py');
    expect(result.error).toContain('tools localSearchCode --scheme');
    expect(result.hints).toBeUndefined();
  });
});
