import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { contextUtils } from '../../src/utils/contextUtils.js';
import { executeDirectTool } from '../helpers/executeDirectTool.js';

describe('localSearch native completeness', () => {
  let root: string;
  beforeAll(async () => {
    await mkdir(join(process.cwd(), '.octocode', 'tmp'), { recursive: true });
    root = await mkdtemp(
      join(process.cwd(), '.octocode', 'tmp', 'native-coverage-')
    );
    await writeFile(join(root, 'sample.txt'), 'needle');
  });
  afterAll(async () => {
    await rm(root, { recursive: true, force: true });
  });

  it.each([true, false])(
    'preserves native file errors with matches=%s',
    async withMatches => {
      const spy = vi.spyOn(contextUtils, 'searchRipgrep').mockResolvedValue({
        files: withMatches
          ? [
              {
                path: join(root, 'sample.txt'),
                matchCount: 1,
                matches: [{ line: 1, column: 0, value: 'needle' }],
              },
            ]
          : [],
        stats: {
          matchCount: withMatches ? 1 : 0,
          matchedLines: withMatches ? 1 : 0,
          filesSearched: 1,
          capped: false,
          errorCount: 1,
          firstError: 'denied.txt: Permission denied',
        },
      } as never);
      try {
        const result = await executeDirectTool('localSearch', {
          queries: [{ path: root, searchText: 'needle', regex: 'literal' }],
        });
        const row = (
          result.structuredContent as {
            results: Array<{ data: Record<string, unknown> }>;
          }
        ).results[0]!;
        expect(row.data.stats).toMatchObject({
          errorCount: 1,
          firstError: 'denied.txt: Permission denied',
        });
        expect(row.data).toMatchObject({
          terminalLimit: true,
          truncated: true,
          partialReasons: ['nativeSearchError'],
        });
        if (withMatches) expect(row.data.files).toHaveLength(1);
      } finally {
        spy.mockRestore();
      }
    }
  );

  it('makes an unpageable native span limit explicit through the public tool', async () => {
    const path = join(root, 'many.txt');
    await writeFile(path, 'needle '.repeat(1001));
    const result = await executeDirectTool('localSearch', {
      queries: [
        {
          path,
          searchText: 'needle',
          regex: 'literal',
          resultView: 'matchOnly',
          maxMatchesPerFile: 2000,
        },
      ],
    });
    const row = (
      result.structuredContent as {
        results: Array<{ data: Record<string, unknown> }>;
      }
    ).results[0]!;
    expect(row.data.stats).toMatchObject({
      totalOccurrences: 1001,
      capped: true,
      capReason: 'maxOnlyMatchingPerLine',
    });
    expect(row.data).toMatchObject({
      terminalLimit: true,
      truncated: true,
      partialReasons: ['nativeResultCap'],
    });
  });
});
