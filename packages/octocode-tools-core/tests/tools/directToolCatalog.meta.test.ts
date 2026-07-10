import { describe, expect, it } from 'vitest';

import {
  DirectToolInputError,
  prepareDirectToolInput,
} from '../../src/tools/directToolCatalog.meta.js';

describe('prepareDirectToolInput', () => {
  it('rejects unknown query fields when strict mode is enabled', () => {
    expect(() =>
      prepareDirectToolInput(
        'localSearchCode',
        { path: '.', keywords: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow(DirectToolInputError);

    expect(() =>
      prepareDirectToolInput(
        'localSearchCode',
        { path: '.', keywords: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow('Unknown field(s): typo');
  });

  it('suggests the closest valid field for the measured first-contact misses', () => {
    const cases: Array<{ tool: string; query: object; expected: string }> = [
      {
        tool: 'ghSearchCode',
        query: { keywordsToSearch: ['x'], owner: 'o', repo: 'r' },
        expected: "'keywordsToSearch' → did you mean 'keywords'?",
      },
      {
        tool: 'ghViewRepoStructure',
        query: { owner: 'o', repo: 'r', path: '', depth: 1 },
        expected: "'depth' → did you mean 'maxDepth'?",
      },
      {
        tool: 'npmSearch',
        query: { name: 'zod' },
        expected: "'name' → did you mean 'packageName'?",
      },
    ];
    for (const { tool, query, expected } of cases) {
      try {
        prepareDirectToolInput(tool, query, { rejectUnknownFields: true });
        expect.unreachable(`expected ${tool} to reject unknown fields`);
      } catch (error) {
        expect(error).toBeInstanceOf(DirectToolInputError);
        const details = (error as DirectToolInputError & { details?: string[] })
          .details;
        expect(details, `${tool} details`).toContain(expected);
      }
    }
  });
});
