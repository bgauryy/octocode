import { describe, expect, it } from 'vitest';
import { prepareDirectToolInput } from '../../src/tools/directToolCatalog/toolInputPreparation.js';
import { getDirectToolSchemaVariants } from '../../src/tools/directToolCatalog/toolSchemaRelations.js';
import { NpmSearchQueryLocalSchema } from '../../src/tools/package_search/scheme.js';

describe('research skill public contract alignment', () => {
  it.each([1, 2])(
    'rejects page %i for exact npm lookup at the public boundary',
    page => {
      expect(() =>
        prepareDirectToolInput(
          'npmSearch',
          { packageName: 'octokit', page },
          { rejectUnknownFields: true }
        )
      ).toThrow();
    }
  );

  it('accepts an exact lookup without inserting inapplicable pagination', () => {
    const prepared = prepareDirectToolInput(
      'npmSearch',
      { packageName: 'octokit' },
      { rejectUnknownFields: true }
    );
    expect(prepared.queries[0]).not.toHaveProperty('page');
    expect(
      NpmSearchQueryLocalSchema.safeParse(prepared.queries[0]).success
    ).toBe(true);
  });

  it('preserves keyword pagination', () => {
    const prepared = prepareDirectToolInput(
      'npmSearch',
      { keywords: ['octokit'], page: 2, pageSize: 2 },
      { rejectUnknownFields: true }
    );
    expect(prepared.queries[0]).toMatchObject({
      keywords: ['octokit'],
      page: 2,
      pageSize: 2,
    });
  });

  it('advertises optional reachability roots consistently with inferred-root support', () => {
    const prepared = prepareDirectToolInput(
      'localAnalyzeGraph',
      { operation: 'reachability', path: '/repo' },
      { rejectUnknownFields: true }
    );
    expect(prepared.queries[0]).toMatchObject({
      operation: 'reachability',
      path: '/repo',
    });
    const variant = getDirectToolSchemaVariants('localAnalyzeGraph').find(
      item => item.name === 'reachability'
    );
    expect(variant).toBeDefined();
    expect(variant?.requires).not.toContain('entrypoints');
  });
});
