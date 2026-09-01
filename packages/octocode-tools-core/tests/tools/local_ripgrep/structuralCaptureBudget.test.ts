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

const LONG_STATEMENT = `const x = ${'y'.repeat(300)};`;

function nativeResultWithBodyCapture() {
  return {
    files: [
      {
        path: '/repo/a.ts',
        matches: [
          {
            startLine: 1,
            endLine: 9,
            startCol: 0,
            endCol: 1,
            text: 'export function f() { ... }',
            metavars: {
              NAME: ['f'],
              BODY: ['// a comment line', LONG_STATEMENT, 'return 1;'],
            },
            metavarRanges: {
              NAME: [
                { text: 'f', line: 1, column: 16, endLine: 1, endColumn: 17 },
              ],
              BODY: [
                {
                  text: '// a comment line',
                  line: 2,
                  column: 2,
                  endLine: 2,
                  endColumn: 19,
                },
                {
                  text: LONG_STATEMENT,
                  line: 3,
                  column: 2,
                  endLine: 3,
                  endColumn: 300,
                },
                {
                  text: 'return 1;',
                  line: 4,
                  column: 2,
                  endLine: 4,
                  endColumn: 11,
                },
              ],
            },
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
  };
}

function makeQuery(overrides: Record<string, unknown> = {}) {
  return {
    id: 'capture-budget-test',
    researchGoal: 'unit-test',
    reasoning: 'validate structural capture budget',
    path: '/repo',
    mode: 'structural' as const,
    pattern: 'export function $NAME() { $$$BODY }',
    maxFiles: 10,
    ...overrides,
  };
}

describe('structural capture budget', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.validateToolPath.mockReturnValue({
      isValid: true,
      sanitizedPath: '/repo',
    });
    mocks.stat.mockRejectedValue(new Error('not found in unit test'));
    mocks.structuralSearchFiles.mockReturnValue(nativeResultWithBodyCapture());
  });

  it('by default: keeps single captures, drops list-capture text from metavars, prunes comments and truncates ranges', async () => {
    const result = await searchContentStructural(makeQuery());
    const match = result.files[0]?.matches?.[0] as Record<string, unknown>;

    // Single-node capture text survives (cheap, high value as LSP anchor).
    expect(match.metavars).toMatchObject({ NAME: ['f'] });
    // List capture ($$$BODY) full text is a token bomb — omitted by default.
    expect((match.metavars as Record<string, string[]>).BODY).toBeUndefined();

    const ranges = match.metavarRanges as Record<
      string,
      Array<{ text: string; line: number }>
    >;
    // Ranges survive as line anchors...
    expect(ranges.NAME?.[0]).toMatchObject({ text: 'f', line: 1 });
    // ...comment-only entries are pruned from list captures...
    expect(ranges.BODY?.some(r => r.text.startsWith('//'))).toBe(false);
    // ...and long capture texts are truncated.
    const longEntry = ranges.BODY?.find(r => r.line === 3);
    expect(longEntry).toBeDefined();
    expect(longEntry!.text.length).toBeLessThanOrEqual(121);
    expect(longEntry!.text.endsWith('…')).toBe(true);
    // Real statements keep their anchors.
    expect(ranges.BODY?.some(r => r.text === 'return 1;')).toBe(true);
    expect(match.capturesTruncated).toBe(true);

    const expandCaptures = (
      result as unknown as {
        next?: Record<
          string,
          { tool?: string; query?: Record<string, unknown> }
        >;
      }
    ).next?.expandCaptures;
    expect(expandCaptures).toMatchObject({
      tool: 'local.text',
      query: {
        mode: 'structural',
        captureText: true,
      },
    });
  });

  it('captureText:true restores full capture text (verbatim passthrough)', async () => {
    const result = await searchContentStructural(
      makeQuery({ captureText: true })
    );
    const match = result.files[0]?.matches?.[0] as Record<string, unknown>;
    const metavars = match.metavars as Record<string, string[]>;

    expect(metavars.BODY).toHaveLength(3);
    expect(metavars.BODY?.[1]).toBe(LONG_STATEMENT);
    const ranges = match.metavarRanges as Record<
      string,
      Array<{ text: string }>
    >;
    expect(ranges.BODY?.[1]?.text).toBe(LONG_STATEMENT);
    expect(ranges.BODY).toHaveLength(3);
    expect(match.capturesTruncated).toBeUndefined();
    expect(
      (result as unknown as { next?: Record<string, unknown> }).next
        ?.expandCaptures
    ).toBeUndefined();
  });
});
