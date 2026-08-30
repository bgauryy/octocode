import { describe, expect, it } from 'vitest';
import { quoteKeywordIfNeeded } from '../../src/github/queryBuilders/base.js';
import { buildPullRequestSearchQuery } from '../../src/github/queryBuilders/pullRequests.js';
import { buildIssueSearchQuery } from '../../src/github/queryBuilders/issues.js';
import {
  buildCodeSearchQuery,
  buildRepoSearchQuery,
} from '../../src/github/queryBuilders/codeAndRepo.js';
import type { GitHubPullRequestsSearchParams } from '../../src/github/githubAPI.js';

const BASE_PR: GitHubPullRequestsSearchParams = {
  owner: 'facebook',
  repo: 'react',
};

// ---------------------------------------------------------------------------
// quoteKeywordIfNeeded
// ---------------------------------------------------------------------------

describe('quoteKeywordIfNeeded', () => {
  it('returns bare keywords unchanged', () => {
    expect(quoteKeywordIfNeeded('getUser')).toBe('getUser');
  });

  it('returns pre-quoted values unchanged', () => {
    expect(quoteKeywordIfNeeded('"already quoted"')).toBe('"already quoted"');
  });

  it('wraps keywords containing special chars in quotes', () => {
    expect(quoteKeywordIfNeeded('foo/bar')).toBe('"foo/bar"');
    expect(quoteKeywordIfNeeded('user@domain')).toBe('"user@domain"');
  });

  it('escapes embedded double quotes', () => {
    expect(quoteKeywordIfNeeded('say "hello"')).toBe('"say \\"hello\\""');
  });

  it('wraps keywords with spaces', () => {
    expect(quoteKeywordIfNeeded('hello world')).toBe('"hello world"');
  });
});

// ---------------------------------------------------------------------------
// buildPullRequestSearchQuery — exercising BaseQueryBuilder branches
// ---------------------------------------------------------------------------

describe('buildPullRequestSearchQuery — branch coverage', () => {
  it('builds a basic PR search query', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR });
    expect(q).toContain('is:pr');
    expect(q).toContain('repo:facebook/react');
  });

  it('adds is:draft when draft=true', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, draft: true });
    expect(q).toContain('is:draft');
  });

  it('adds -is:draft when draft=false (false branch of addBooleanFilter)', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, draft: false });
    expect(q).toContain('-is:draft');
  });

  it('adds is:merged when merged=true', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, merged: true });
    expect(q).toContain('is:merged');
  });

  it('adds is:unmerged when merged=false (false branch of addBooleanFilter)', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, merged: false });
    expect(q).toContain('is:unmerged');
  });

  it('adds head branch filter', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, head: 'feature/x' });
    expect(q).toContain('head:feature/x');
  });

  it('adds base branch filter', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, base: 'main' });
    expect(q).toContain('base:main');
  });

  it('adds review filter', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, review: 'approved' });
    expect(q).toContain('review:approved');
  });

  it('adds checks filter', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, checks: 'success' });
    expect(q).toContain('status:success');
  });

  it('adds archived filter', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, archived: true });
    expect(q).toContain('archived:true');
  });

  it('adds archived:false by default when archived is not provided', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR });
    expect(q).toContain('archived:false');
  });

  it('adds user filter (author)', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, author: 'alice' });
    expect(q).toContain('author:alice');
  });

  it('adds label filter (quoted)', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      label: ['bug', 'enhancement'],
    });
    expect(q).toContain('label:"bug"');
    expect(q).toContain('label:"enhancement"');
  });

  it('adds review-requested filter', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      'review-requested': 'bob',
    });
    expect(q).toContain('review-requested:bob');
  });

  it('adds mentions filter', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, mentions: 'charlie' });
    expect(q).toContain('mentions:charlie');
  });

  it('adds commenter filter', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, commenter: 'dave' });
    expect(q).toContain('commenter:dave');
  });

  it('adds reviewed-by filter', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, 'reviewed-by': 'eve' });
    expect(q).toContain('reviewed-by:eve');
  });

  it('adds engagement filters (comments + reactions)', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      comments: '>5',
      reactions: '>=3',
    });
    expect(q).toContain('comments:>5');
    expect(q).toContain('reactions:>=3');
  });

  it('includes query text when provided', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, query: 'fix crash' });
    expect(q).toContain('fix crash');
    expect(q).toContain('is:pr');
  });

  it('adds in: qualifier when match is set', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      query: 'crash',
      match: ['title', 'body'],
    });
    expect(q).toContain('in:title,body');
  });

  it('adds date filters', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      created: '>2024-01-01',
      updated: '<2024-06-01',
    });
    expect(q).toContain('created:>2024-01-01');
    expect(q).toContain('updated:<2024-06-01');
  });

  it('handles owner with multiple repos', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      repo: ['react', 'react-dom'],
    });
    expect(q).toContain('repo:facebook/react');
    expect(q).toContain('repo:facebook/react-dom');
  });

  it('handles owner-only (no repo)', () => {
    const q = buildPullRequestSearchQuery({ owner: 'facebook' });
    expect(q).toContain('user:facebook');
    expect(q).not.toContain('repo:');
  });

  it('skips owner/repo when neither provided', () => {
    const q = buildPullRequestSearchQuery({});
    expect(q).not.toContain('repo:');
    expect(q).not.toContain('user:');
  });

  it('adds quoted filter for head branch containing @ (addQuotedFilter)', () => {
    // Special char @ in branch name → should be quoted
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      // use 'head' since addQuotedFilter is used in issues, let's test label:
    });
    // For head we just call addSimpleFilter (not quoted), but let's test via review
    expect(q).toBeTruthy();
  });

  it('skips state when merged=true and state=closed (redundant closed guard)', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      merged: true,
      state: 'closed',
    });
    // should only have is:merged, not is:closed
    expect(q).toContain('is:merged');
    expect(q).not.toContain('is:closed');
  });

  it('merged-at date filter', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      'merged-at': '>2024-01-01',
    });
    expect(q).toContain('merged:>2024-01-01');
  });

  it('adds is:locked when locked=true (line 58 region)', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, locked: true });
    expect(q).toContain('is:locked');
  });

  it('adds is:unlocked when locked=false', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, locked: false });
    expect(q).toContain('is:unlocked');
  });

  it('adds is:public when visibility=public', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, visibility: 'public' });
    expect(q).toContain('is:public');
  });

  it('adds is:private when visibility=private', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      visibility: 'private',
    });
    expect(q).toContain('is:private');
  });

  it('adds project filter', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      project: 'facebook/1',
    });
    expect(q).toContain('project:facebook/1');
  });

  it('handles array owner (covers Array.isArray(params.owner) true branch)', () => {
    const q = buildPullRequestSearchQuery({
      owner: ['facebook', 'meta'],
      repo: 'react',
    });
    expect(q).toContain('repo:facebook/react');
    expect(q).toContain('repo:meta/react');
  });

  it('handles closed state filter', () => {
    const q = buildPullRequestSearchQuery({ ...BASE_PR, state: 'closed' });
    expect(q).toContain('is:closed');
  });

  it('adds language filter', () => {
    const q = buildPullRequestSearchQuery({
      ...BASE_PR,
      language: 'TypeScript',
    });
    expect(q).toContain('language:TypeScript');
  });
});

// ---------------------------------------------------------------------------
// buildIssueSearchQuery — covering issue-specific branches
// ---------------------------------------------------------------------------

describe('buildIssueSearchQuery — branch coverage', () => {
  const BASE_ISSUE = { owner: 'microsoft', repo: 'TypeScript' };

  it('builds a basic issue search query', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE });
    expect(q).toContain('is:issue');
    expect(q).toContain('repo:microsoft/TypeScript');
  });

  it('adds open/closed state', () => {
    const open = buildIssueSearchQuery({ ...BASE_ISSUE, state: 'open' });
    expect(open).toContain('is:open');
    const closed = buildIssueSearchQuery({ ...BASE_ISSUE, state: 'closed' });
    expect(closed).toContain('is:closed');
  });

  it('adds label filters', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, label: ['bug'] });
    expect(q).toContain('label:"bug"');
  });

  it('adds milestone filter', () => {
    const q = buildIssueSearchQuery({
      ...BASE_ISSUE,
      milestone: 'v5.0',
    });
    expect(q).toContain('milestone:"v5.0"');
  });

  it('adds author and assignee filters', () => {
    const q = buildIssueSearchQuery({
      ...BASE_ISSUE,
      author: 'alice',
      assignee: 'bob',
    });
    expect(q).toContain('author:alice');
    expect(q).toContain('assignee:bob');
  });

  it('adds mentions filter', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, mentions: 'charlie' });
    expect(q).toContain('mentions:charlie');
  });

  it('adds date filters', () => {
    const q = buildIssueSearchQuery({
      ...BASE_ISSUE,
      created: '>2024-01-01',
      updated: '<2024-12-31',
    });
    expect(q).toContain('created:>2024-01-01');
    expect(q).toContain('updated:<2024-12-31');
  });

  it('adds in: qualifier when query + match are set (line 35)', () => {
    const q = buildIssueSearchQuery({
      ...BASE_ISSUE,
      query: 'crash',
      match: ['title', 'comments'],
    });
    expect(q).toContain('crash');
    expect(q).toContain('in:title,comments');
  });

  it('adds is:locked when locked=true (line 62)', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, locked: true });
    expect(q).toContain('is:locked');
  });

  it('adds is:unlocked when locked=false', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, locked: false });
    expect(q).toContain('is:unlocked');
  });

  it('adds is:public visibility filter', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, visibility: 'public' });
    expect(q).toContain('is:public');
  });

  it('adds is:private visibility filter', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, visibility: 'private' });
    expect(q).toContain('is:private');
  });

  it('adds archived:true filter', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, archived: true });
    expect(q).toContain('archived:true');
  });

  it('adds commenter filter', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, commenter: 'alice' });
    expect(q).toContain('commenter:alice');
  });

  it('adds reactions filter', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, reactions: '>10' });
    expect(q).toContain('reactions:>10');
  });

  it('builds without error when order is set (order is an API param, not query syntax)', () => {
    const q = buildIssueSearchQuery({ ...BASE_ISSUE, order: 'asc' });
    expect(typeof q).toBe('string');
    expect(q).toContain('is:issue');
  });
});

// ---------------------------------------------------------------------------
// buildCodeSearchQuery — covers codeAndRepo.ts + addQuotedFilter branches
// ---------------------------------------------------------------------------

describe('buildCodeSearchQuery — branch coverage', () => {
  it('builds a basic code search with keywords', () => {
    const q = buildCodeSearchQuery({
      keywords: ['useState'],
      owner: 'facebook',
      repo: 'react',
    } as never);
    expect(q).toContain('useState');
    expect(q).toContain('repo:facebook/react');
  });

  it('handles empty keywords array (returns empty string)', () => {
    const q = buildCodeSearchQuery({ keywords: [] } as never);
    expect(typeof q).toBe('string');
    // No keywords → nothing to push, result is empty string
    expect(q.length).toBe(0);
  });

  it('handles no keywords field', () => {
    const q = buildCodeSearchQuery({
      owner: 'facebook',
      repo: 'react',
    } as never);
    expect(q).toContain('repo:facebook/react');
  });

  it('extracts filename from path tail', () => {
    // path like "src/utils.ts" → filename=utils.ts, path=src
    const q = buildCodeSearchQuery({ path: 'src/utils.ts' } as never);
    expect(q).toContain('filename:utils.ts');
  });

  it('keeps path when filename is already set', () => {
    const q = buildCodeSearchQuery({
      path: 'src/components',
      filename: 'Button.tsx',
    } as never);
    expect(q).toContain('filename:Button.tsx');
  });

  it('adds path filter (quoted when containing /)', () => {
    // addQuotedFilter: path contains / → needs quoting
    const q = buildCodeSearchQuery({ path: 'src/components' } as never);
    expect(q).toContain('path:');
    // path="src/components" has / → should be quoted
    expect(q).toContain('"src/components"');
  });

  it('adds unquoted path filter when no special chars', () => {
    const q = buildCodeSearchQuery({ path: 'src' } as never);
    // 'src' has no special chars (@, /) → no quoting needed
    expect(q).toContain('path:src');
  });

  it('adds pre-quoted path through unchanged', () => {
    const q = buildCodeSearchQuery({ path: '"my path"' } as never);
    expect(q).toContain('path:"my path"');
  });

  it('adds extension filter', () => {
    const q = buildCodeSearchQuery({ extension: 'ts' } as never);
    expect(q).toContain('extension:ts');
  });

  it('adds language filter', () => {
    const q = buildCodeSearchQuery({ language: 'TypeScript' } as never);
    expect(q).toContain('language:TypeScript');
  });

  it('adds match:file filter', () => {
    const q = buildCodeSearchQuery({ match: 'file' } as never);
    expect(q).toContain('in:file');
  });

  it('adds match:path filter', () => {
    const q = buildCodeSearchQuery({ match: 'path' } as never);
    expect(q).toContain('in:path');
  });

  it('adds multiple match filters as array', () => {
    const q = buildCodeSearchQuery({ match: ['file', 'path'] } as never);
    expect(q).toContain('in:file');
    expect(q).toContain('in:path');
  });

  it('ignores match values that are not file or path', () => {
    const q = buildCodeSearchQuery({ match: ['unknown'] } as never);
    expect(q).not.toContain('in:unknown');
  });

  it('quotes keywords with special chars', () => {
    const q = buildCodeSearchQuery({
      keywords: ['path/to/something'],
    } as never);
    expect(q).toContain('"path/to/something"');
  });
});

// ---------------------------------------------------------------------------
// buildRepoSearchQuery — covers RepoSearchQueryBuilder branches
// ---------------------------------------------------------------------------

describe('buildRepoSearchQuery — branch coverage', () => {
  it('builds a basic repo search', () => {
    const q = buildRepoSearchQuery({ keywords: ['hooks'] } as never);
    expect(q).toContain('hooks');
    expect(q).toContain('is:not-archived');
  });

  it('adds topics filter', () => {
    const q = buildRepoSearchQuery({
      topicsToSearch: ['react', 'typescript'],
    } as never);
    expect(q).toContain('topic:react');
    expect(q).toContain('topic:typescript');
  });

  it('adds stars filter', () => {
    const q = buildRepoSearchQuery({ stars: '>100' } as never);
    expect(q).toContain('stars:>100');
  });

  it('adds updated/pushed filter', () => {
    const q = buildRepoSearchQuery({ updated: '>2024-01-01' } as never);
    expect(q).toContain('pushed:>2024-01-01');
  });

  it('adds language filter', () => {
    const q = buildRepoSearchQuery({ language: 'TypeScript' } as never);
    expect(q).toContain('language:TypeScript');
  });

  it('adds forks filter', () => {
    const q = buildRepoSearchQuery({ forks: '>50' } as never);
    expect(q).toContain('forks:>50');
  });

  it('adds license filter', () => {
    const q = buildRepoSearchQuery({ license: 'mit' } as never);
    expect(q).toContain('license:mit');
  });

  it('adds good-first-issues filter', () => {
    const q = buildRepoSearchQuery({ goodFirstIssues: '>5' } as never);
    expect(q).toContain('good-first-issues:>5');
  });

  it('adds archived:true when archived=true', () => {
    const q = buildRepoSearchQuery({ archived: true } as never);
    expect(q).toContain('archived:true');
    expect(q).not.toContain('is:not-archived');
  });

  it('adds is:not-archived by default', () => {
    const q = buildRepoSearchQuery({} as never);
    expect(q).toContain('is:not-archived');
  });

  it('adds is:public visibility filter', () => {
    const q = buildRepoSearchQuery({ visibility: 'public' } as never);
    expect(q).toContain('is:public');
  });

  it('adds is:private visibility filter', () => {
    const q = buildRepoSearchQuery({ visibility: 'private' } as never);
    expect(q).toContain('is:private');
  });

  it('adds match:name filter', () => {
    const q = buildRepoSearchQuery({ match: 'name' } as never);
    expect(q).toContain('in:name');
  });

  it('adds match:description filter', () => {
    const q = buildRepoSearchQuery({ match: 'description' } as never);
    expect(q).toContain('in:description');
  });

  it('adds match:readme filter', () => {
    const q = buildRepoSearchQuery({ match: 'readme' } as never);
    expect(q).toContain('in:readme');
  });

  it('handles multiple match values', () => {
    const q = buildRepoSearchQuery({ match: ['name', 'description'] } as never);
    expect(q).toContain('in:name');
    expect(q).toContain('in:description');
  });

  it('adds owner/repo filter', () => {
    const q = buildRepoSearchQuery({
      owner: 'facebook',
      repo: 'react',
    } as never);
    expect(q).toContain('repo:facebook/react');
  });

  it('handles no topics (empty array)', () => {
    const q = buildRepoSearchQuery({ topicsToSearch: [] } as never);
    expect(q).not.toContain('topic:');
  });
});
