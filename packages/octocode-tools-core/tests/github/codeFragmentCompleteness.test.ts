import { expect, it } from 'vitest';
import { mapCodeSearchProviderResult } from '../../src/tools/providerMappers/codeSearch.js';
import { buildGitHubSearchFinalizer } from '../../src/tools/github_search/finalizer.js';

it('preserves the full provider fragment and a match beyond the old 500-character cut', () => {
  const context = 'x'.repeat(600) + 'needle';
  const data = mapCodeSearchProviderResult(
    {
      items: [
        {
          path: 'a.ts',
          repository: { name: 'o/r', id: 'o/r', url: 'https://github.com/o/r' },
          url: '',
          matches: [{ context, positions: [[600, 606]] }],
        },
      ],
      totalCount: 1,
    } as never,
    { keywords: ['needle'] } as never
  );
  const result = buildGitHubSearchFinalizer()({
    queries: [{ operation: 'code', keywords: ['needle'] }],
    results: [{ index: 0, data }],
    config: { toolName: 'ghSearch' },
  } as never);
  const output = result.structuredContent as {
    results: Array<{
      data: {
        files: Array<{
          matches: Array<{
            value: string;
            matchIndices: Array<{ start: number; end: number }>;
          }>;
        }>;
      };
    }>;
  };
  const match = output.results[0]!.data.files[0]!.matches[0]!;
  expect(match.value).toBe(context);
  expect(match.matchIndices[0]).toMatchObject({ start: 600, end: 606 });
});
