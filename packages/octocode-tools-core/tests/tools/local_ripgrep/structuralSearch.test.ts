import { describe, expect, it, vi, beforeEach } from 'vitest';
import { cleanJsonObject } from '../../../src/responses.js';

const mocks = vi.hoisted(() => ({
  readFile: vi.fn(),
  stat: vi.fn(),
  validateToolPath: vi.fn(),
  structuralSearch: vi.fn(),
  structuralSearchFiles: vi.fn(),
  structuralSearchFilesDetailed: vi.fn(),
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
    structuralSearchFilesDetailed: mocks.structuralSearchFilesDetailed,
  },
}));

const { searchContentStructural } =
  await import('../../../src/tools/local_ripgrep/structuralSearch.js');

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
    mocks.structuralSearchFilesDetailed.mockReturnValue({
      files: [],
      diagnostics: [],
    });
  });

  it('does not suggest TypeScript return syntax for a C++ function pattern', async () => {
    const result = await searchContentStructural(
      makeQuery({ pattern: 'int $NAME($$$ARGS) { $$$BODY }', langType: 'cpp' })
    );
    const output = cleanJsonObject(result);
    expect(output).toHaveProperty('diagnostics');
    expect(JSON.stringify(output)).not.toContain(': $R');
  });

  it.each([
    ['target($X)', 'target($X);'],
    [
      'function $NAME($$$ARGS) { $$$BODY }',
      'function $NAME($$$ARGS): $R { $$$BODY }',
    ],
  ])(
    'discloses the executed fallback pattern and continues it: %s',
    async (pattern, effectivePattern) => {
      const empty = {
        status: 'ok',
        diagnostics: [],
        files: [],
        totalMatches: 0,
        parsedFiles: 1,
        skippedByPreFilter: 0,
        skippedUnreadable: 0,
        skippedLarge: 0,
        warnings: [],
      };
      mocks.structuralSearchFiles
        .mockResolvedValueOnce(empty)
        .mockResolvedValueOnce({
          ...empty,
          totalMatches: 2,
          files: [
            {
              path: '/repo/a.ts',
              matches: [1, 2].map(line => ({
                startLine: line,
                endLine: line,
                startCol: 0,
                endCol: 9,
                text: 'target(x)',
                metavars: {},
              })),
            },
          ],
        });
      const result = await searchContentStructural(
        makeQuery({ pattern, maxMatchesPerFile: 1 })
      );
      const output = cleanJsonObject(result);
      expect(output).toMatchObject({
        diagnostics: [
          expect.objectContaining({
            code: 'structural.query.rewritten',
            message: expect.stringContaining(JSON.stringify(effectivePattern)),
          }),
        ],
        next: { nextMatchPage: { query: { pattern: effectivePattern } } },
      });
      expect(output).not.toHaveProperty('warnings');
    }
  );

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
    // silently skip node_modules/build/dist.
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

  it('marks a structural match value when its display text is compacted', async () => {
    const longMatch = 'x'.repeat(350);
    mocks.structuralSearchFiles.mockReturnValue({
      files: [
        {
          path: '/repo/long.ts',
          matches: [
            {
              startLine: 1,
              endLine: 20,
              startCol: 1,
              endCol: 2,
              text: longMatch,
              metavars: {},
            },
          ],
        },
      ],
      totalMatches: 1,
      parsedFiles: 1,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
    });

    const result = await searchContentStructural(makeQuery());
    const value = result.files[0]?.matches?.[0]?.value ?? '';

    expect(value).toHaveLength(300);
    expect(value.endsWith('…')).toBe(true);
  });

  it('reports how many zero-match engine diagnostics were omitted', async () => {
    mocks.structuralSearchFiles.mockReturnValue({
      files: [],
      totalMatches: 0,
      parsedFiles: 1,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
      query: {
        kind: 'pattern',
        preFilter: 'anchor',
        diagnostics: Array.from({ length: 5 }, (_, index) => ({
          severity: 'warning',
          stage: 'parse',
          code: `D${index}`,
          message: `diagnostic ${index}`,
        })),
      },
      diagnostics: [],
    });

    const result = await searchContentStructural(makeQuery());

    expect(result.warnings?.join('\n')).toContain(
      '2 additional engine diagnostic(s) omitted'
    );
    expect(mocks.structuralSearchFilesDetailed).not.toHaveBeenCalled();
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

    const result = await searchContentStructural(
      makeQuery({ path: '/repo/a.ts' })
    );

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
    expect(cleanJsonObject(result)).toMatchObject({
      diagnostics: [
        expect.objectContaining({
          code: 'structural.query.noMatches',
          message: expect.stringContaining('0 structural matches'),
        }),
      ],
    });
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

  it.each(['sh', 'bash', 'zsh', 'vue', 'svelte', 'astro', 'dart'])(
    'reports unsupported .%s files without invalid-pattern advice',
    async extension => {
      mocks.stat.mockResolvedValue({ isFile: () => true });
      mocks.structuralSearch.mockRejectedValue(
        new Error(
          `[structural.language.unsupported] structural search does not support .${extension} files`
        )
      );
      const result = await searchContentStructural(makeQuery());
      expect(result.status).toBe('error');
      expect(result.errorCode).toBe('structural.language.unsupported');
      expect(result.error).toContain(`.${extension}`);
      expect(result.error).toContain('operation:"text"');
      expect(result.error).not.toContain('Invalid structural');
      expect(result.error).not.toContain('$$$BODY');
      expect(mocks.structuralSearch).toHaveBeenCalledTimes(1);
    }
  );

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
    expect(result.error).toContain('tools localSearch --scheme');
    expect(result.error).not.toContain('local.text');
    expect(result.hints).toBeUndefined();
  });

  it.each([
    'structural.query.compileFailed',
    'structural.query.invalid',
    'structural.parse.failed',
  ])('preserves native %s through the public error payload', async code => {
    mocks.structuralSearchFiles.mockRejectedValue(
      new Error(`[${code}] Invalid native query detail.`)
    );
    const result = await searchContentStructural(makeQuery());
    expect(result).toMatchObject({ status: 'error', errorCode: code });
    const output = cleanJsonObject(result);
    expect(output).toMatchObject({ errorCode: code });
    expect(JSON.stringify(output)).not.toContain('toolExecutionFailed');
  });

  it('preserves completed files and execution diagnostics when native matching is incomplete', async () => {
    const diagnostic = {
      code: 'structural.match.backtrackingLimit',
      severity: 'warning',
      stage: 'match',
      message: 'Structural matching exhausted its attempt budget.',
      path: '/repo/wide.ts',
    };
    mocks.structuralSearchFiles.mockResolvedValue({
      status: 'truncated',
      diagnostics: [diagnostic],
      files: [
        {
          path: '/repo/a.ts',
          matches: [
            {
              startLine: 1,
              endLine: 1,
              startCol: 0,
              endCol: 9,
              text: 'target(x)',
              metavars: {},
            },
          ],
        },
      ],
      totalMatches: 1,
      parsedFiles: 2,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
    });

    const result = await searchContentStructural(makeQuery());

    expect(result).toMatchObject({
      searchEngine: 'structural',
      truncated: true,
      terminalLimit: true,
      partialReasons: ['structuralLimit'],
      diagnostics: [diagnostic],
    });
    expect(result.files).toHaveLength(1);
    expect(result.files[0]?.path).toBe('/repo/a.ts');
  });

  it('does not retry or advise pattern repair after an incomplete empty search', async () => {
    mocks.structuralSearchFiles.mockResolvedValue({
      status: 'truncated',
      diagnostics: [
        {
          code: 'structural.parse.interrupted',
          severity: 'warning',
          stage: 'parse',
          message: 'Parser deadline reached.',
        },
      ],
      files: [],
      totalMatches: 0,
      parsedFiles: 1,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
    });

    const result = await searchContentStructural(makeQuery());

    expect(result).toMatchObject({ truncated: true, terminalLimit: true });
    expect(mocks.structuralSearchFiles).toHaveBeenCalledTimes(1);
    expect(mocks.structuralSearchFilesDetailed).not.toHaveBeenCalled();
    expect(result.warnings?.join('\n') ?? '').not.toContain(
      '0 structural matches'
    );
  });

  it('reports single-file execution exhaustion as a typed terminal limit', async () => {
    mocks.stat.mockResolvedValue({ isFile: () => true });
    mocks.structuralSearch.mockRejectedValue(
      new Error('[structural.match.depthLimit] Matching depth exhausted.')
    );

    const result = await searchContentStructural(makeQuery());

    expect(result).toMatchObject({
      searchEngine: 'structural',
      truncated: true,
      terminalLimit: true,
      partialReasons: ['structuralLimit'],
      diagnostics: [
        expect.objectContaining({ code: 'structural.match.depthLimit' }),
      ],
    });
    expect(result.error).toBeUndefined();
    expect(result.warnings?.join('\n') ?? '').not.toContain(
      'Invalid structural'
    );
  });

  it('preserves the native content bound as a typed terminal limit', async () => {
    mocks.stat.mockResolvedValue({ isFile: () => true });
    mocks.structuralSearch.mockRejectedValue(
      new Error('[structural.content.tooLarge] Content exceeds the byte limit.')
    );
    const result = await searchContentStructural(makeQuery());
    expect(result).toMatchObject({
      truncated: true,
      terminalLimit: true,
      diagnostics: [
        expect.objectContaining({
          code: 'structural.content.tooLarge',
          stage: 'parse',
        }),
      ],
    });
    expect(result.error).toBeUndefined();
  });

  it('keeps an incomplete empty retry instead of falling back to completed absence', async () => {
    mocks.structuralSearchFiles
      .mockResolvedValueOnce({
        status: 'ok',
        diagnostics: [],
        files: [],
        totalMatches: 0,
        parsedFiles: 1,
        skippedByPreFilter: 0,
        skippedUnreadable: 0,
        skippedLarge: 0,
        warnings: [],
      })
      .mockResolvedValueOnce({
        status: 'truncated',
        diagnostics: [
          {
            code: 'structural.match.backtrackingLimit',
            severity: 'warning',
            stage: 'match',
            message: 'Attempt budget exhausted.',
          },
        ],
        files: [],
        totalMatches: 0,
        parsedFiles: 1,
        skippedByPreFilter: 0,
        skippedUnreadable: 0,
        skippedLarge: 0,
        warnings: [],
      });
    const result = await searchContentStructural(makeQuery());
    expect(result).toMatchObject({
      truncated: true,
      terminalLimit: true,
      diagnostics: [
        expect.objectContaining({ code: 'structural.match.backtrackingLimit' }),
        expect.objectContaining({
          code: 'structural.query.rewritten',
          message: expect.stringContaining('"target($X);"'),
        }),
      ],
    });
    expect(mocks.structuralSearchFiles).toHaveBeenCalledTimes(2);
    expect(result.warnings?.join('\n') ?? '').not.toContain('Matched after');
    expect(mocks.structuralSearchFilesDetailed).not.toHaveBeenCalled();
  });

  it('propagates an execution limit thrown by a single-file retry', async () => {
    mocks.stat.mockResolvedValue({ isFile: () => true });
    mocks.structuralSearch
      .mockResolvedValueOnce([])
      .mockRejectedValueOnce(
        new Error('[structural.parse.interrupted] Deadline reached.')
      );
    const result = await searchContentStructural(makeQuery());
    expect(result).toMatchObject({
      truncated: true,
      terminalLimit: true,
      diagnostics: [
        expect.objectContaining({ code: 'structural.parse.interrupted' }),
        expect.objectContaining({
          code: 'structural.query.rewritten',
          message: expect.stringContaining('"target($X);"'),
        }),
      ],
    });
    expect(mocks.structuralSearch).toHaveBeenCalledTimes(2);
  });

  it('does not treat a completed single-file search with maxFiles 1 as scan truncation', async () => {
    mocks.stat.mockResolvedValue({ isFile: () => true });
    mocks.structuralSearch.mockResolvedValue([
      {
        startLine: 1,
        endLine: 1,
        startCol: 0,
        endCol: 9,
        text: 'target(x)',
        metavars: {},
      },
    ]);

    const result = await searchContentStructural(makeQuery({ maxFiles: 1 }));

    expect(result.files).toHaveLength(1);
    expect(result).not.toHaveProperty('truncated', true);
    expect(result).not.toHaveProperty('partialReasons');
    expect(result).not.toHaveProperty('next.expandScan');
    expect(mocks.structuralSearchFiles).not.toHaveBeenCalled();
  });

  it('uses the native scanTruncated signal to distinguish an exact file-bound completion', async () => {
    mocks.structuralSearchFiles.mockResolvedValue({
      status: 'ok',
      scanTruncated: false,
      diagnostics: [],
      files: [],
      totalMatches: 0,
      parsedFiles: 10,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
    });

    const result = await searchContentStructural(
      makeQuery({
        pattern: undefined,
        rule: 'kind: call_expression',
        maxFiles: 10,
      })
    );

    expect(result).not.toHaveProperty('truncated', true);
    expect(result).not.toHaveProperty('next.expandScan');
  });

  it('keeps a native scan limit resumable and skips zero-match repair retries', async () => {
    mocks.structuralSearchFiles.mockResolvedValue({
      status: 'ok',
      scanTruncated: true,
      diagnostics: [],
      files: [],
      totalMatches: 0,
      parsedFiles: 10,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
    });

    const result = await searchContentStructural(makeQuery());

    expect(result).toMatchObject({
      truncated: true,
      partialReasons: ['maxFiles'],
      next: {
        expandScan: {
          tool: 'local.text',
          query: { mode: 'structural', maxFiles: 20, page: 1 },
        },
      },
    });
    expect(result).not.toHaveProperty('terminalLimit', true);
    expect(mocks.structuralSearchFiles).toHaveBeenCalledTimes(1);
    expect(mocks.structuralSearchFilesDetailed).not.toHaveBeenCalled();
    expect(result.warnings?.join('\n') ?? '').not.toContain(
      '0 structural matches'
    );
  });

  it('expands beyond the actual structural default scan bound', async () => {
    mocks.structuralSearchFiles.mockResolvedValue({
      status: 'ok',
      scanTruncated: true,
      diagnostics: [],
      files: [],
      totalMatches: 0,
      parsedFiles: 2000,
      skippedByPreFilter: 0,
      skippedUnreadable: 0,
      skippedLarge: 0,
      warnings: [],
    });

    const result = await searchContentStructural(
      makeQuery({ maxFiles: undefined })
    );

    expect(mocks.structuralSearchFiles).toHaveBeenCalledWith(
      expect.objectContaining({ maxFiles: 2000 })
    );
    expect(result).toHaveProperty('next.expandScan.query.maxFiles', 4000);
    expect(result).not.toHaveProperty('terminalLimit', true);
  });
});
