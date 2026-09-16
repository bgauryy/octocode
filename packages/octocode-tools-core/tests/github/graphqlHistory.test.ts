import { describe, expect, it } from 'vitest';
import { graphqlCompleteCollectionEligible } from '../../src/github/graphqlHistory.js';

describe('graphqlCompleteCollectionEligible', () => {
  it('requires a PR number and at least two content flags', () => {
    expect(
      graphqlCompleteCollectionEligible({
        owner: 'a',
        repo: 'b',
      })
    ).toBe(false);
    expect(
      graphqlCompleteCollectionEligible({
        owner: 'a',
        repo: 'b',
        prNumber: 1,
        content: { body: true, changedFiles: true },
      })
    ).toBe(true);
  });

  it('skips GraphQL when collection continuation, patches, or commit files are requested', () => {
    expect(
      graphqlCompleteCollectionEligible({
        owner: 'a',
        repo: 'b',
        prNumber: 1,
        content: { body: true, comments: { discussion: true } },
        collectionPages: { discussion: 2 },
      })
    ).toBe(false);
    expect(
      graphqlCompleteCollectionEligible({
        owner: 'a',
        repo: 'b',
        prNumber: 1,
        content: {
          body: true,
          changedFiles: true,
          patches: { mode: 'all' },
        },
      })
    ).toBe(false);
    expect(
      graphqlCompleteCollectionEligible({
        owner: 'a',
        repo: 'b',
        prNumber: 1,
        content: {
          body: true,
          commits: { includeFiles: true },
        },
      })
    ).toBe(false);
  });
});
