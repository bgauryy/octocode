import { describe, expect, it } from 'vitest';
import { prepareDirectToolInput } from '../helpers/prepareDirectToolInput.js';
import { getDirectToolSchemaVariants } from '@octocodeai/octocode-core/schema';
import { ArtifactSearchQueryLocalSchema } from '@octocodeai/octocode-core/schema';

describe('research skill public contract alignment', () => {
  it.each([1, 2])(
    'rejects page %i for exact npm lookup at the public boundary',
    page => {
      expect(() =>
        prepareDirectToolInput(
          'artifactSearch',
          { type: 'npm', packageName: 'octokit', page },
          { rejectUnknownFields: true }
        )
      ).toThrow();
    }
  );

  it('accepts an exact lookup without inserting inapplicable pagination', () => {
    const prepared = prepareDirectToolInput(
      'artifactSearch',
      { type: 'npm', packageName: 'octokit' },
      { rejectUnknownFields: true }
    );
    expect(prepared.queries[0]).not.toHaveProperty('page');
    expect(
      ArtifactSearchQueryLocalSchema.safeParse(prepared.queries[0]).success
    ).toBe(true);
  });

  it('preserves keyword pagination', () => {
    const prepared = prepareDirectToolInput(
      'artifactSearch',
      { type: 'npm', keywords: ['octokit'], cursor: 'opaque', pageSize: 2 },
      { rejectUnknownFields: true }
    );
    expect(prepared.queries[0]).toMatchObject({
      keywords: ['octokit'],
      cursor: 'opaque',
      pageSize: 2,
    });
  });

  it('advertises optional reachability roots consistently with inferred-root support', () => {
    const prepared = prepareDirectToolInput(
      'astSearch',
      { operation: 'topology', analysis: 'reachability', path: '/repo' },
      { rejectUnknownFields: true }
    );
    expect(prepared.queries[0]).toMatchObject({
      operation: 'topology',
      analysis: 'reachability',
      path: '/repo',
    });
    const variant = getDirectToolSchemaVariants('astSearch').find(
      item => item.name === 'topology:reachability'
    );
    expect(variant).toBeDefined();
    expect(variant?.requires).not.toContain('entrypoints');
  });
});
