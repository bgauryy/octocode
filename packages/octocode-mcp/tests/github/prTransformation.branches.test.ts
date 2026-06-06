import { describe, it, expect } from 'vitest';
import { applyPartialContentFilter } from '../../src/github/prTransformation.js';
import type { GitHubPullRequestsSearchParams } from '../../src/github/githubAPI.js';

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

  it('should strip patches for metadata type', () => {
    const files = [
      createFile('a.ts', '@@ -1 +1 @@\n+hello'),
      createFile('b.ts', '@@ -1 +1 @@\n+world'),
    ];
    const params: GitHubPullRequestsSearchParams = { type: 'metadata' };

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

  it('should filter files for partialContent type', () => {
    const files = [
      createFile('a.ts', '@@ -1,3 +1,5 @@\n+line1\n+line2'),
      createFile('b.ts', '@@ -1,3 +1,5 @@\n+other'),
      createFile('c.ts', '@@ -1 +1 @@\n+excluded'),
    ];
    const params: GitHubPullRequestsSearchParams = {
      type: 'partialContent',
      partialContentMetadata: [{ file: 'a.ts' }, { file: 'b.ts' }],
    };

    const result = applyPartialContentFilter(files, params);

    expect(result).toHaveLength(2);
    expect(result.map(f => f.filename)).toEqual(['a.ts', 'b.ts']);
  });

  it('should handle partialContent with no patch', () => {
    const files = [createFile('a.ts', undefined)];
    const params: GitHubPullRequestsSearchParams = {
      type: 'partialContent',
      partialContentMetadata: [{ file: 'a.ts' }],
    };

    const result = applyPartialContentFilter(files, params);

    expect(result).toHaveLength(1);
    expect(result[0]!.patch).toBeUndefined();
  });

  it('should return all files with patches for fullContent type', () => {
    const files = [
      createFile('a.ts', 'patch-a'),
      createFile('b.ts', 'patch-b'),
    ];
    const params: GitHubPullRequestsSearchParams = { type: 'fullContent' };

    const result = applyPartialContentFilter(files, params);

    expect(result).toHaveLength(2);
    expect(result[0]!.patch).toBe('patch-a');
    expect(result[1]!.patch).toBe('patch-b');
  });

  it('should handle empty partialContentMetadata', () => {
    const files = [createFile('a.ts', 'patch')];
    const params: GitHubPullRequestsSearchParams = {
      type: 'partialContent',
      partialContentMetadata: [],
    };

    const result = applyPartialContentFilter(files, params);

    expect(result).toHaveLength(0);
  });
});
