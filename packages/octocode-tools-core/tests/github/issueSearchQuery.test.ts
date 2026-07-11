import { describe, expect, it } from 'vitest';
import {
  buildIssueSearchQuery,
  shouldUseSearchForIssues,
} from '../../src/github/queryBuilders.js';

describe('buildIssueSearchQuery', () => {
  it('emits is:issue and never is:pr', () => {
    const q = buildIssueSearchQuery({
      owner: 'microsoft',
      repo: 'TypeScript',
      query: 'crash',
      state: 'open',
      label: 'bug',
    });
    expect(q).toContain('is:issue');
    expect(q).not.toContain('is:pr');
    expect(q).toContain('repo:microsoft/TypeScript');
    expect(q).toContain('is:open');
    expect(q).toContain('label:"bug"');
  });

  it('shouldUseSearchForIssues is true when keywords/filters need search', () => {
    expect(
      shouldUseSearchForIssues({
        owner: 'o',
        repo: 'r',
        query: 'crash',
      })
    ).toBe(true);
    expect(
      shouldUseSearchForIssues({
        owner: 'o',
        repo: 'r',
        state: 'open',
      })
    ).toBe(false);
  });
});
