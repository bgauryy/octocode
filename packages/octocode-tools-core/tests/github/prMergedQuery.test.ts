import { describe, expect, it } from 'vitest';
import { buildPullRequestSearchQuery } from '../../src/github/queryBuilders.js';

/**
 * Regression for `pr --state merged` returning empty. The provider maps
 * state:"merged" -> state:"closed" + merged:true; the search query must NOT be
 * over-constrained with a redundant `is:closed` alongside `is:merged`.
 */
describe('PR search query for merged state', () => {
  it('emits a clean is:merged (no redundant is:closed)', () => {
    const q = buildPullRequestSearchQuery({
      owner: 'facebook',
      repo: 'react',
      state: 'closed', // already mapped from "merged" by the caller
      merged: true,
    } as never);
    expect(q).toContain('repo:facebook/react');
    expect(q).toContain('is:pr');
    expect(q).toContain('is:merged');
    expect(q).not.toContain('is:closed');
  });

  it('still emits is:closed for a plain closed (non-merged) search', () => {
    const q = buildPullRequestSearchQuery({
      owner: 'facebook',
      repo: 'react',
      state: 'closed',
    } as never);
    expect(q).toContain('is:closed');
    expect(q).not.toContain('is:merged');
  });

  it('emits is:open for open PRs', () => {
    const q = buildPullRequestSearchQuery({
      owner: 'facebook',
      repo: 'react',
      state: 'open',
    } as never);
    expect(q).toContain('is:open');
  });
});
