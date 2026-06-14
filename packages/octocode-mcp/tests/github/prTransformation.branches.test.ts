import { describe, it, expect } from 'vitest';
import {
  applyPartialContentFilter,
  createBasePRTransformation,
  formatPRForResponse,
} from '../../../octocode-tools-core/src/github/prTransformation.js';
import type { GitHubPullRequestsSearchParams } from '../../../octocode-tools-core/src/github/githubAPI.js';

describe('createBasePRTransformation — labels', () => {
  const baseItem = {
    number: 1,
    html_url: 'https://github.com/owner/repo/pull/1',
    title: 'Test PR',
    body: null,
    state: 'open',
    user: { login: 'alice' },
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z',
    closed_at: null,
    draft: false,
    head: { ref: 'feat/x', sha: 'abc' },
    base: { ref: 'main', sha: 'def' },
  };

  it('maps label objects (name field) to string array', () => {
    const item = {
      ...baseItem,
      labels: [{ name: 'bug' }, { name: 'enhancement' }],
    };
    const { prData } = createBasePRTransformation(item);
    expect(prData.labels).toEqual(['bug', 'enhancement']);
  });

  it('passes through string labels unchanged', () => {
    const item = { ...baseItem, labels: ['bug', 'enhancement'] };
    const { prData } = createBasePRTransformation(item);
    expect(prData.labels).toEqual(['bug', 'enhancement']);
  });

  it('produces empty labels array when labels is null', () => {
    const item = { ...baseItem, labels: null };
    const { prData } = createBasePRTransformation(item);
    expect(prData.labels).toEqual([]);
  });
});

describe('formatPRForResponse — labels', () => {
  const makeBasePR = (labels: string[]) => ({
    number: 42,
    title: 'My PR',
    body: undefined,
    state: 'open' as const,
    author: 'alice',
    labels,
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-02T00:00:00Z',
    closed_at: null,
    url: 'https://github.com/owner/repo/pull/42',
    draft: false,
    reactions: 0,
    comments: [],
  });

  it('includes labels as objects when PR has labels', () => {
    const pr = makeBasePR(['bug', 'enhancement']);
    const result = formatPRForResponse(pr);
    expect(result.labels).toEqual([
      { id: 0, name: 'bug', color: '' },
      { id: 0, name: 'enhancement', color: '' },
    ]);
  });

  it('omits labels field entirely when labels array is empty', () => {
    const pr = makeBasePR([]);
    const result = formatPRForResponse(pr);
    expect(result).not.toHaveProperty('labels');
  });
});

describe('applyPartialContentFilter', () => {
  const createFile = (filename: string, patch?: string) => ({
    filename,
    status: 'modified' as const,
    additions: 5,
    deletions: 3,
    changes: 8,
    sha: 'abc',
    blob_url: '',
    raw_url: '',
    contents_url: '',
    patch,
  });

  it('should strip patches when no patch content is requested', () => {
    const files = [
      createFile('a.ts', '@@ -1 +1 @@\n+hello'),
      createFile('b.ts', '@@ -1 +1 @@\n+world'),
    ];
    const params: GitHubPullRequestsSearchParams = {};

    const result = applyPartialContentFilter(files, params);

    expect(result).toHaveLength(2);
    expect(result[0]!.patch).toBeUndefined();
    expect(result[1]!.patch).toBeUndefined();
  });

  it('should default to metadata when type is not specified', () => {
    const files = [createFile('a.ts', 'some patch')];
    const params: GitHubPullRequestsSearchParams = {};

    const result = applyPartialContentFilter(files, params);

    expect(result[0]!.patch).toBeUndefined();
  });

  it('should filter files for selected patch content', () => {
    const files = [
      createFile('a.ts', '@@ -1,3 +1,5 @@\n+line1\n+line2'),
      createFile('b.ts', '@@ -1,3 +1,5 @@\n+other'),
      createFile('c.ts', '@@ -1 +1 @@\n+excluded'),
    ];
    const params: GitHubPullRequestsSearchParams = {
      content: { patches: { mode: 'selected', files: ['a.ts', 'b.ts'] } },
    };

    const result = applyPartialContentFilter(files, params);

    expect(result).toHaveLength(2);
    expect(result.map(f => f.filename)).toEqual(['a.ts', 'b.ts']);
  });

  it('should handle partialContent with no patch', () => {
    const files = [createFile('a.ts', undefined)];
    const params: GitHubPullRequestsSearchParams = {
      content: { patches: { mode: 'selected', files: ['a.ts'] } },
    };

    const result = applyPartialContentFilter(files, params);

    expect(result).toHaveLength(1);
    expect(result[0]!.patch).toBeUndefined();
  });

  it('should return all files with patches for all-patches mode', () => {
    const files = [
      createFile('a.ts', 'patch-a'),
      createFile('b.ts', 'patch-b'),
    ];
    const params: GitHubPullRequestsSearchParams = {
      content: { patches: { mode: 'all' } },
    };

    const result = applyPartialContentFilter(files, params);

    expect(result).toHaveLength(2);
    expect(result[0]!.patch).toBe('patch-a');
    expect(result[1]!.patch).toBe('patch-b');
  });

  it('should handle empty selected patch list', () => {
    const files = [createFile('a.ts', 'patch')];
    const params: GitHubPullRequestsSearchParams = {
      content: { patches: { mode: 'selected', files: [] } },
    };

    const result = applyPartialContentFilter(files, params);

    expect(result).toHaveLength(0);
  });
});
