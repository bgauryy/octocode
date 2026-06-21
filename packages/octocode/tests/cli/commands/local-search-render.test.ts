import { describe, expect, it, vi } from 'vitest';

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_color: string, s: string) => s,
  bold: (s: string) => s,
  dim: (s: string) => s,
}));

import { renderLocalResults } from '../../../src/cli/commands/local-search-render.js';

describe('local search renderer', () => {
  it('renders only-matching histograms as value rows', () => {
    const output = renderLocalResults(
      {
        results: [
          {
            data: {
              files: [
                {
                  path: 'src/example.ts',
                  totalMatchRows: 2,
                  returnedMatchRows: 2,
                  matches: [
                    { line: 1, value: 'TOKEN', count: 3 },
                    { line: 2, value: 'OTHER', count: 1 },
                  ],
                },
              ],
            },
          },
        ],
      },
      10,
      0,
      { valuesOnly: true }
    );

    expect(output).toContain('3x  TOKEN');
    expect(output).toContain('1x  OTHER');
    expect(output).not.toContain('src/example.ts');
    expect(output).not.toContain('L1:');
  });

  it('renders context lines with physical line gutters and metavars', () => {
    const output = renderLocalResults(
      {
        results: [
          {
            data: {
              files: [
                {
                  path: 'src/example.ts',
                  matches: [
                    {
                      line: 10,
                      value: 'before\nneedle()\nafter',
                      metavars: { X: ['needle()'] },
                    },
                  ],
                },
              ],
            },
          },
        ],
      },
      10,
      1
    );

    expect(output).toContain('src/example.ts');
    expect(output).toContain('L9');
    expect(output).toContain('L10:');
    expect(output).toContain('$X=needle()');
  });

  it('uses shared match counts and pagination hints', () => {
    const output = renderLocalResults(
      {
        results: [
          {
            data: {
              shared: { totalMatchRows: 7 },
              pagination: { totalFiles: 3, page: 2, totalPages: 4 },
              files: [{ path: 'src/a.ts' }],
            },
          },
        ],
      },
      1
    );

    expect(output).toContain('(7 matches)');
    expect(output).toContain('… 2 more files');
    expect(output).toContain('Page 2/4');
  });

  it('omits fake zero counts for files-only results', () => {
    const output = renderLocalResults(
      {
        results: [
          {
            data: {
              files: [{ path: 'src/a.ts' }],
            },
          },
        ],
      },
      1
    );

    expect(output).toContain('src/a.ts');
    expect(output).not.toContain('(0 matches)');
  });

  it('renders no-match output for empty results', () => {
    expect(
      renderLocalResults({ results: [{ data: { files: [] } }] }, 10)
    ).toContain('No matches found.');
  });
});
