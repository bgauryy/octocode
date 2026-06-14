import { describe, it, expect } from 'vitest';
import { parseRipgrepJson } from '../../../../octocode-tools-core/src/utils/parsers/ripgrep.js';
import {
  parseCountOutput,
  parseFilesOnlyOutput,
  parseRipgrepOutput,
} from '../../../../octocode-tools-core/src/tools/local_ripgrep/ripgrepParser.js';
import type { RipgrepQuery } from '../../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js';

const baseQuery = {
  keywords: 'test',
  path: '/test/path',
} as RipgrepQuery;

describe('parseRipgrepJson', () => {
  it('should parse basic match output', () => {
    const jsonOutput = JSON.stringify({
      type: 'match',
      data: {
        path: { text: '/test/file.ts' },
        lines: { text: 'const test = 1;' },
        line_number: 10,
        absolute_offset: 100,
        submatches: [{ match: { text: 'test' }, start: 6, end: 10 }],
      },
    });

    const { files, stats } = parseRipgrepJson(jsonOutput, baseQuery);

    expect(files).toHaveLength(1);
    expect(files[0]!.path).toBe('/test/file.ts');
    expect(files[0]!.matchCount).toBe(1);
    expect(files[0]!.matches![0]!.line).toBe(10);
    expect(files[0]!.matches![0]!.column).toBe(6);
    expect(stats).toEqual({});
  });

  it('should parse multiple matches in the same file', () => {
    const jsonOutput = [
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test line 1' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test line 2' },
          line_number: 20,
          absolute_offset: 200,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
    ].join('\n');

    const { files } = parseRipgrepJson(jsonOutput, baseQuery);

    expect(files).toHaveLength(1);
    expect(files[0]!.matchCount).toBe(2);
    expect(files[0]!.matches).toHaveLength(2);
  });

  it('should parse matches across multiple files', () => {
    const jsonOutput = [
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file1.ts' },
          lines: { text: 'test in file 1' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file2.ts' },
          lines: { text: 'test in file 2' },
          line_number: 5,
          absolute_offset: 50,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
    ].join('\n');

    const { files } = parseRipgrepJson(jsonOutput, baseQuery);

    expect(files).toHaveLength(2);
  });

  it('should parse context lines', () => {
    const jsonOutput = [
      JSON.stringify({
        type: 'context',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'context before' },
          line_number: 9,
          absolute_offset: 80,
        },
      }),
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
      JSON.stringify({
        type: 'context',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'context after' },
          line_number: 11,
          absolute_offset: 120,
        },
      }),
    ].join('\n');

    const { files } = parseRipgrepJson(jsonOutput, {
      ...baseQuery,
      contextLines: 1,
    });

    expect(files).toHaveLength(1);
    expect(files[0]!.matches![0]!.value).toContain('context before');
    expect(files[0]!.matches![0]!.value).toContain('test match');
    expect(files[0]!.matches![0]!.value).toContain('context after');
  });

  it('should parse summary statistics', () => {
    const jsonOutput = [
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test' },
          line_number: 1,
          absolute_offset: 0,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
      JSON.stringify({
        type: 'summary',
        data: {
          elapsed_total: { human: '0.001s' },
          stats: {
            elapsed: { human: '0.001s' },
            searches: 10,
            searches_with_match: 3,
            bytes_searched: 1000,
            bytes_printed: 50,
            matched_lines: 5,
            matches: 7,
          },
        },
      }),
    ].join('\n');

    const { stats } = parseRipgrepJson(jsonOutput, baseQuery);

    expect(stats.matchCount).toBe(7);
    expect(stats.matchedLines).toBe(5);
    expect(stats.filesMatched).toBe(3);
    expect(stats.filesSearched).toBe(10);
    expect(stats.bytesSearched).toBe(1000);
    expect(stats.searchTime).toBe('0.001s');
  });

  it('should handle empty submatches array', () => {
    const jsonOutput = JSON.stringify({
      type: 'match',
      data: {
        path: { text: '/test/file.ts' },
        lines: { text: 'test line' },
        line_number: 10,
        absolute_offset: 100,
        submatches: [],
      },
    });

    const { files } = parseRipgrepJson(jsonOutput, baseQuery);

    expect(files).toHaveLength(1);
    expect(files[0]!.matches![0]!.column).toBe(0);
  });

  it('should truncate long match values', () => {
    const longContent = 'x'.repeat(500);
    const jsonOutput = JSON.stringify({
      type: 'match',
      data: {
        path: { text: '/test/file.ts' },
        lines: { text: longContent },
        line_number: 10,
        absolute_offset: 100,
        submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
      },
    });

    const { files } = parseRipgrepJson(jsonOutput, {
      ...baseQuery,
      matchContentLength: 100,
    });

    expect(files[0]!.matches![0]!.value!.length).toBeLessThanOrEqual(100);
    expect(files[0]!.matches![0]!.value).toMatch(/\.\.\.$/);
  });

  it('should skip malformed JSON lines', () => {
    const jsonOutput = [
      'this is not json',
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test' },
          line_number: 1,
          absolute_offset: 0,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
      'also not json {broken',
    ].join('\n');

    const { files } = parseRipgrepJson(jsonOutput, baseQuery);

    expect(files).toHaveLength(1);
  });

  it('should skip non-JSON lines (like output headers)', () => {
    const jsonOutput = [
      'Searching...',
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test' },
          line_number: 1,
          absolute_offset: 0,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
    ].join('\n');

    const { files } = parseRipgrepJson(jsonOutput, baseQuery);

    expect(files).toHaveLength(1);
  });

  it('should handle empty output', () => {
    const { files, stats } = parseRipgrepJson('', baseQuery);

    expect(files).toHaveLength(0);
    expect(stats).toEqual({});
  });

  it('should skip valid JSON that fails schema validation (line 46 branch)', () => {
    const jsonOutput = [
      JSON.stringify({ type: 'unknown_ripgrep_event', data: { path: {} } }),
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test' },
          line_number: 1,
          absolute_offset: 0,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
    ].join('\n');

    const { files } = parseRipgrepJson(jsonOutput, baseQuery);

    expect(files).toHaveLength(1);
  });

  it('should use contextLines when specific before/after not set', () => {
    const jsonOutput = [
      JSON.stringify({
        type: 'context',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'context before' },
          line_number: 8,
          absolute_offset: 60,
        },
      }),
      JSON.stringify({
        type: 'context',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'context before 2' },
          line_number: 9,
          absolute_offset: 80,
        },
      }),
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test match' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
    ].join('\n');

    const { files } = parseRipgrepJson(jsonOutput, {
      ...baseQuery,
      contextLines: 2,
    });

    expect(files[0]!.matches![0]!.value).toContain('context before');
    expect(files[0]!.matches![0]!.value).toContain('context before 2');
  });

  it('should not double-space snippets when ripgrep JSON lines include trailing newlines', () => {
    const jsonOutput = [
      JSON.stringify({
        type: 'context',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'context before\n' },
          line_number: 9,
          absolute_offset: 80,
        },
      }),
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test match\n' },
          line_number: 10,
          absolute_offset: 100,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
      JSON.stringify({
        type: 'context',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'context after\n' },
          line_number: 11,
          absolute_offset: 120,
        },
      }),
    ].join('\n');

    const { files } = parseRipgrepJson(jsonOutput, {
      ...baseQuery,
      contextLines: 1,
    });

    expect(files[0]!.matches![0]!.value).toBe(
      ['context before', 'test match', 'context after'].join('\n')
    );
  });

  it('should handle begin/end messages (ignored)', () => {
    const jsonOutput = [
      JSON.stringify({
        type: 'begin',
        data: { path: { text: '/test/file.ts' } },
      }),
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/test/file.ts' },
          lines: { text: 'test' },
          line_number: 1,
          absolute_offset: 0,
          submatches: [{ match: { text: 'test' }, start: 0, end: 4 }],
        },
      }),
      JSON.stringify({
        type: 'end',
        data: { path: { text: '/test/file.ts' } },
      }),
    ].join('\n');

    const { files } = parseRipgrepJson(jsonOutput, baseQuery);

    expect(files).toHaveLength(1);
  });
});

describe('parseCountOutput', () => {
  it('should parse basic path:count format', () => {
    const stdout = '/src/file1.ts:5\n/src/file2.ts:12\n';

    const files = parseCountOutput(stdout);

    expect(files).toHaveLength(2);
    expect(files[0]!.path).toBe('/src/file1.ts');
    expect(files[0]!.matchCount).toBe(5);
    expect(files[0]!.matches).toEqual([]);
    expect(files[1]!.path).toBe('/src/file2.ts');
    expect(files[1]!.matchCount).toBe(12);
  });

  it('should handle single file output', () => {
    const stdout = '/src/main.ts:1\n';

    const files = parseCountOutput(stdout);

    expect(files).toHaveLength(1);
    expect(files[0]!.path).toBe('/src/main.ts');
    expect(files[0]!.matchCount).toBe(1);
  });

  it('should handle paths with colons (Windows-like or special chars)', () => {
    const stdout = 'C:/Users/project/file.ts:3\n';

    const files = parseCountOutput(stdout);

    expect(files).toHaveLength(1);
    expect(files[0]!.path).toBe('C:/Users/project/file.ts');
    expect(files[0]!.matchCount).toBe(3);
  });

  it('should handle empty output', () => {
    const files = parseCountOutput('');
    expect(files).toHaveLength(0);
  });

  it('should handle whitespace-only output', () => {
    const files = parseCountOutput('   \n  \n');
    expect(files).toHaveLength(0);
  });

  it('should handle large counts', () => {
    const stdout = '/src/bigfile.ts:999\n';

    const files = parseCountOutput(stdout);

    expect(files).toHaveLength(1);
    expect(files[0]!.matchCount).toBe(999);
  });

  it('should handle zero count (no matches in file)', () => {
    const stdout = '/src/empty.ts:0\n';

    const files = parseCountOutput(stdout);

    expect(files).toHaveLength(1);
    expect(files[0]!.matchCount).toBe(0);
  });

  it('should filter out ripgrep stats lines', () => {
    const stdout =
      '/src/file.ts:5\n3 files contained matches\n10 files searched\n';

    const files = parseCountOutput(stdout);

    expect(files).toHaveLength(1);
    expect(files[0]!.path).toBe('/src/file.ts');
    expect(files[0]!.matchCount).toBe(5);
  });

  it('should fallback to matchCount=1 for malformed lines without colon', () => {
    const stdout = '/src/weirdfile.ts\n';

    const files = parseCountOutput(stdout);

    expect(files).toHaveLength(1);
    expect(files[0]!.path).toBe('/src/weirdfile.ts');
    expect(files[0]!.matchCount).toBe(1);
  });

  it('should fallback to matchCount=1 for non-numeric count', () => {
    const stdout = '/src/file.ts:abc\n';

    const files = parseCountOutput(stdout);

    expect(files).toHaveLength(1);
    expect(files[0]!.path).toBe('/src/file.ts');
    expect(files[0]!.matchCount).toBe(1);
  });
});

describe('parseFilesOnlyOutput', () => {
  it('should parse plain filename-per-line output', () => {
    const stdout = '/src/a.ts\n/src/b.ts\n';

    const files = parseFilesOnlyOutput(stdout);

    expect(files).toHaveLength(2);
    expect(files[0]!.path).toBe('/src/a.ts');
    expect(files[0]!.matchCount).toBe(1);
    expect(files[1]!.path).toBe('/src/b.ts');
    expect(files[1]!.matchCount).toBe(1);
  });

  it('should return empty array for empty output', () => {
    expect(parseFilesOnlyOutput('')).toHaveLength(0);
  });
});

describe('parseRipgrepOutput routing', () => {
  it('should route count queries to parseCountOutput', () => {
    const stdout = '/src/file.ts:7\n/src/other.ts:3\n';
    const query = { ...baseQuery, countLinesPerFile: true } as RipgrepQuery;

    const { files, stats } = parseRipgrepOutput(stdout, query);

    expect(files).toHaveLength(2);
    expect(files[0]!.matchCount).toBe(7);
    expect(files[1]!.matchCount).toBe(3);
    expect(stats.matchCount).toBe(10);
  });

  it('should route countMatchesPerFile queries to parseCountOutput', () => {
    const stdout = '/src/file.ts:15\n';
    const query = {
      ...baseQuery,
      countMatchesPerFile: true,
    } as RipgrepQuery;

    const { files, stats } = parseRipgrepOutput(stdout, query);

    expect(files).toHaveLength(1);
    expect(files[0]!.matchCount).toBe(15);
    expect(stats.matchCount).toBe(15);
  });

  it('should route filesOnly queries to parseFilesOnlyOutput', () => {
    const stdout = '/src/file.ts\n/src/other.ts\n';
    const query = { ...baseQuery, filesOnly: true } as RipgrepQuery;

    const { files, stats } = parseRipgrepOutput(stdout, query);

    expect(files).toHaveLength(2);
    expect(files[0]!.matchCount).toBe(1);
    expect(files[1]!.matchCount).toBe(1);
    expect(stats).toEqual({});
  });
});
