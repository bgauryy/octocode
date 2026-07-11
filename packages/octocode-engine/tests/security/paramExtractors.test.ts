import { describe, expect, it } from 'vitest';

import {
  extractResearchFields,
  extractRepoOwnerFromParams,
} from '../../src/security/paramExtractors.js';

describe('extractResearchFields', () => {
  it('extracts fields from a flat (non-bulk) params object', () => {
    const r = extractResearchFields({
      mainResearchGoal: 'find auth bug',
      researchGoal: 'trace login flow',
      reasoning: 'user reports 500',
    });
    expect(r).toEqual({
      mainResearchGoal: 'find auth bug',
      researchGoal: 'trace login flow',
      reasoning: 'user reports 500',
    });
  });

  it('ignores non-string and empty-string fields on a flat object', () => {
    const r = extractResearchFields({
      mainResearchGoal: '',
      researchGoal: 42,
      reasoning: null,
    });
    expect(r).toEqual({});
  });

  it('aggregates and de-duplicates fields across a queries array', () => {
    const r = extractResearchFields({
      queries: [
        { mainResearchGoal: 'goal A', researchGoal: 'sub 1', reasoning: 'r1' },
        { mainResearchGoal: 'goal A', researchGoal: 'sub 2', reasoning: 'r1' },
      ],
    });
    // 'goal A' and 'r1' dedupe to one; the two distinct sub-goals join.
    expect(r.mainResearchGoal).toBe('goal A');
    expect(r.researchGoal).toBe('sub 1; sub 2');
    expect(r.reasoning).toBe('r1');
  });

  it('falls back to the flat object when queries is empty', () => {
    const r = extractResearchFields({
      queries: [],
      mainResearchGoal: 'flat goal',
    });
    expect(r.mainResearchGoal).toBe('flat goal');
  });

  it('falls back to the flat object when queries is not an array', () => {
    const r = extractResearchFields({
      queries: 'not-an-array',
      reasoning: 'flat reasoning',
    });
    expect(r.reasoning).toBe('flat reasoning');
  });

  it('omits keys entirely when no query contributes them', () => {
    const r = extractResearchFields({
      queries: [{ unrelated: 'x' }],
    });
    expect(r).toEqual({});
    expect('mainResearchGoal' in r).toBe(false);
  });
});

describe('extractRepoOwnerFromParams', () => {
  it('returns a full owner/repo when repository field already contains a slash', () => {
    expect(
      extractRepoOwnerFromParams({ repository: 'facebook/react' })
    ).toEqual(['facebook/react']);
  });

  it('combines separate owner and repo fields', () => {
    expect(
      extractRepoOwnerFromParams({ owner: 'facebook', repo: 'react' })
    ).toEqual(['facebook/react']);
  });

  it('returns just the owner when only owner is present', () => {
    expect(extractRepoOwnerFromParams({ owner: 'facebook' })).toEqual([
      'facebook',
    ]);
  });

  it('returns empty array when neither repository nor owner is present', () => {
    expect(extractRepoOwnerFromParams({ repo: 'react' })).toEqual([]);
    expect(extractRepoOwnerFromParams({})).toEqual([]);
  });

  it('ignores a repository field without a slash and falls back to owner/repo', () => {
    // 'react' has no slash, so it is not treated as owner/repo; owner+repo wins.
    expect(
      extractRepoOwnerFromParams({
        repository: 'react',
        owner: 'facebook',
        repo: 'react',
      })
    ).toEqual(['facebook/react']);
  });

  it('aggregates and de-duplicates repos across a queries array', () => {
    const r = extractRepoOwnerFromParams({
      queries: [
        { repository: 'facebook/react' },
        { owner: 'facebook', repo: 'react' },
        { owner: 'vercel', repo: 'next.js' },
      ],
    });
    expect(r).toContain('facebook/react');
    expect(r).toContain('vercel/next.js');
    // facebook/react is contributed twice but must appear once.
    expect(r.filter(x => x === 'facebook/react')).toHaveLength(1);
  });

  it('ignores non-string repository/owner/repo values', () => {
    expect(
      extractRepoOwnerFromParams({ repository: 123, owner: {}, repo: [] })
    ).toEqual([]);
  });
});
