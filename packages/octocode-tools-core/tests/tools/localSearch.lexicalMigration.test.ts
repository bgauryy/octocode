import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { findDirectToolDefinition } from '@octocodeai/octocode-core/schema';
import { executeDirectTool } from '../helpers/executeDirectTool.js';
import { LocalSearchQuerySchema } from '@octocodeai/octocode-core/schema';

describe('localSearch lexical contract', () => {
  let root = '';
  let file = '';

  beforeAll(async () => {
    await mkdir(join(process.cwd(), '.octocode', 'tmp'), { recursive: true });
    root = await mkdtemp(
      join(process.cwd(), '.octocode', 'tmp', 'local-lexical-')
    );
    file = join(root, 'sample.ts');
    await writeFile(file, 'const needle = 1;\nconst needleish = 2;\n');
    await writeFile(join(root, 'second.ts'), 'const needle = 3;\n');
  });

  afterAll(async () => {
    await rm(root, { recursive: true, force: true });
  });

  it('accepts lexical queries without operation and rejects AST/legacy fields', () => {
    const reasoning = 'Validate the lexical localSearch contract.';
    const schema = findDirectToolDefinition('localSearch')?.inputSchema;
    expect(
      schema?.safeParse({ queries: [{ reasoning, path: root, searchText: 'needle' }] })
        .success
    ).toBe(true);
    expect(
      schema?.safeParse({
        queries: [
          { reasoning, path: root, searchText: 'needle', operation: 'text' },
        ],
      }).success
    ).toBe(false);
    expect(
      schema?.safeParse({
        queries: [{ reasoning, path: root, pattern: 'const $X = $Y' }],
      })
        .success
    ).toBe(false);
  });

  it.each([
    ['literal', 'needle'],
    ['rust', 'needle'],
    ['pcre2', 'needle(?:ish)?'],
  ] as const)(
    'maps regex:%s to the native lexical matcher',
    async (regex, searchText) => {
      const result = await executeDirectTool('localSearch', {
        queries: [
          { path: root, searchText, regex, resultView: 'countMatches' },
        ],
      });
      expect(result.isError).not.toBe(true);
      expect(
        (
          result.structuredContent as {
            results: Array<{ data: { stats?: { totalOccurrences?: number } } }>;
          }
        ).results[0]?.data.stats?.totalOccurrences
      ).toBeGreaterThan(0);
    }
  );

  it('normalizes lexical continuations to schema-valid operation-free queries', async () => {
    const result = await executeDirectTool('localSearch', {
      queries: [
        {
          path: root,
          searchText: 'needle',
          resultView: 'paginated',
          pageSize: 1,
        },
      ],
    });
    const row = (
      result.structuredContent as {
        results: Array<{ data: Record<string, unknown> }>;
      }
    ).results[0];
    const next = (
      row?.data.next as
        Record<string, { query: Record<string, unknown> }> | undefined
    )?.nextPage?.query;
    if (next) {
      expect(next.operation).toBeUndefined();
      const parsed = LocalSearchQuerySchema.safeParse(next);
      expect(
        parsed?.success,
        JSON.stringify({
          next,
          issues: parsed && !parsed.success ? parsed.error.issues : [],
        })
      ).toBe(true);
    }
  });
});
