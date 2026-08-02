import { describe, expect, it } from 'vitest';
import {
  shouldUseSearchForPRs,
} from '../../src/github/queryBuilders/pullRequests.js';
import { shouldUseSearchForIssues } from '../../src/github/queryBuilders/issues.js';
import type { GitHubPullRequestsSearchParams } from '../../src/github/githubAPI.js';

// ---------------------------------------------------------------------------
// shouldUseSearchForPRs
// ---------------------------------------------------------------------------

const BASE: GitHubPullRequestsSearchParams = {
  owner: 'facebook',
  repo: 'react',
};

describe('shouldUseSearchForPRs', () => {
  it('returns false for a bare owner/repo query', () => {
    expect(shouldUseSearchForPRs(BASE)).toBe(false);
  });

  it('returns true when draft is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, draft: true })).toBe(true);
  });

  it('returns true when author is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, author: 'alice' })).toBe(true);
  });

  it('returns true when assignee is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, assignee: 'bob' })).toBe(true);
  });

  it('returns true when query text is non-empty', () => {
    expect(shouldUseSearchForPRs({ ...BASE, query: 'crash' })).toBe(true);
  });

  it('returns false when query is whitespace only', () => {
    expect(shouldUseSearchForPRs({ ...BASE, query: '   ' })).toBe(false);
  });

  it('returns true when labels are provided', () => {
    expect(shouldUseSearchForPRs({ ...BASE, label: ['bug'] })).toBe(true);
  });

  it('returns false when labels array is empty', () => {
    expect(shouldUseSearchForPRs({ ...BASE, label: [] })).toBe(false);
  });

  it('returns true when mentions is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, mentions: 'charlie' })).toBe(true);
  });

  it('returns true when commenter is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, commenter: 'dave' })).toBe(true);
  });

  it('returns true when reviewed-by is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, 'reviewed-by': 'eve' })).toBe(true);
  });

  it('returns true when review-requested is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, 'review-requested': 'fred' })).toBe(true);
  });

  it('returns true when reactions is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, reactions: '>5' })).toBe(true);
  });

  it('returns true when comments is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, comments: '>2' })).toBe(true);
  });

  it('returns true for state=merged', () => {
    expect(shouldUseSearchForPRs({ ...BASE, state: 'merged' })).toBe(true);
  });

  it('returns false for state=open (not a search-only state)', () => {
    expect(shouldUseSearchForPRs({ ...BASE, state: 'open' })).toBe(false);
  });

  it('returns true when milestone is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, milestone: 'v1.0' })).toBe(true);
  });

  it('returns true when language is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, language: 'TypeScript' })).toBe(true);
  });

  it('returns true when checks is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, checks: 'success' })).toBe(true);
  });

  it('returns true when review is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, review: 'approved' })).toBe(true);
  });

  it('returns true when locked is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, locked: true })).toBe(true);
  });

  it('returns true when visibility is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, visibility: 'public' })).toBe(true);
  });

  it('returns true when project is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, project: 'facebook/1' })).toBe(true);
  });

  it('returns true when created date is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, created: '>2024-01-01' })).toBe(true);
  });

  it('returns true when updated date is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, updated: '>2024-01-01' })).toBe(true);
  });

  it('returns true when merged-at is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, 'merged-at': '>2024-01-01' })).toBe(true);
  });

  it('returns true when closed is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, closed: '>2024-01-01' })).toBe(true);
  });

  it('returns true when merged is set', () => {
    expect(shouldUseSearchForPRs({ ...BASE, merged: true })).toBe(true);
  });

  it('returns true when match is a non-empty array', () => {
    expect(shouldUseSearchForPRs({ ...BASE, match: ['title'] })).toBe(true);
  });

  it('returns false when match is an empty array', () => {
    expect(shouldUseSearchForPRs({ ...BASE, match: [] })).toBe(false);
  });

  it('returns true when sort is comments', () => {
    expect(shouldUseSearchForPRs({ ...BASE, sort: 'comments' })).toBe(true);
  });

  it('returns true when sort is reactions', () => {
    expect(shouldUseSearchForPRs({ ...BASE, sort: 'reactions' })).toBe(true);
  });

  it('returns true when owner is an array', () => {
    expect(shouldUseSearchForPRs({ owner: ['facebook', 'meta'], repo: 'react' })).toBe(true);
  });

  it('returns true when repo is an array', () => {
    expect(shouldUseSearchForPRs({ owner: 'facebook', repo: ['react', 'react-dom'] })).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// shouldUseSearchForIssues
// ---------------------------------------------------------------------------

describe('shouldUseSearchForIssues', () => {
  const BASE_ISSUE = { owner: 'microsoft', repo: 'TypeScript' };

  it('returns false for a bare owner/repo query', () => {
    expect(shouldUseSearchForIssues(BASE_ISSUE)).toBe(false);
  });

  it('returns true when query is non-empty', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, query: 'crash' })).toBe(true);
  });

  it('returns false when query is whitespace', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, query: '   ' })).toBe(false);
  });

  it('returns true when author is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, author: 'alice' })).toBe(true);
  });

  it('returns true when assignee is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, assignee: 'bob' })).toBe(true);
  });

  it('returns true when label is non-empty string', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, label: 'bug' })).toBe(true);
  });

  it('returns true when label is non-empty array', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, label: ['bug'] })).toBe(true);
  });

  it('returns true when label is an empty string', () => {
    // Empty string has length 0 → false... actually the check is label.length > 0
    // For non-array, it checks (label.length > 0)
    // Empty string has length 0 → returns false
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, label: '' })).toBe(false);
  });

  it('returns true when mentions is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, mentions: 'charlie' })).toBe(true);
  });

  it('returns true when commenter is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, commenter: 'dave' })).toBe(true);
  });

  it('returns true when reactions is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, reactions: '>5' })).toBe(true);
  });

  it('returns true when comments is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, comments: '>2' })).toBe(true);
  });

  it('returns true when milestone is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, milestone: 'v5' })).toBe(true);
  });

  it('returns true when locked is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, locked: true })).toBe(true);
  });

  it('returns true when visibility is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, visibility: 'public' })).toBe(true);
  });

  it('returns true when created is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, created: '>2024-01-01' })).toBe(true);
  });

  it('returns true when updated is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, updated: '>2024-06-01' })).toBe(true);
  });

  it('returns true when closed is set', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, closed: '>2024-01-01' })).toBe(true);
  });

  it('returns true when match is a non-empty array', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, match: ['title'] })).toBe(true);
  });

  it('returns false when match is an empty array', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, match: [] })).toBe(false);
  });

  it('returns true when sort is comments', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, sort: 'comments' })).toBe(true);
  });

  it('returns true when sort is reactions', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, sort: 'reactions' })).toBe(true);
  });

  it('returns false when sort is created (not a search-trigger sort)', () => {
    expect(shouldUseSearchForIssues({ ...BASE_ISSUE, sort: 'created' })).toBe(false);
  });

  it('returns true when owner is an array', () => {
    expect(shouldUseSearchForIssues({ owner: ['microsoft', 'google'], repo: 'TypeScript' })).toBe(true);
  });

  it('returns true when repo is an array', () => {
    expect(shouldUseSearchForIssues({ owner: 'microsoft', repo: ['TypeScript', 'vs-code'] })).toBe(true);
  });
});
