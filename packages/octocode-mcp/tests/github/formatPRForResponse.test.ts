import { describe, it, expect } from 'vitest';
import { formatPRForResponse } from '../../../octocode-tools-core/src/github/prTransformation.js';
import type {
  GitHubPullRequestItem,
  DiffEntry,
} from '../../../octocode-tools-core/src/github/githubAPI.js';

const createMockDiffEntry = (
  overrides: Partial<DiffEntry> &
    Pick<
      DiffEntry,
      'filename' | 'status' | 'additions' | 'deletions' | 'changes'
    >
): DiffEntry =>
  ({
    sha: 'mock-sha',
    blob_url: 'https://github.com/mock/blob',
    raw_url: 'https://github.com/mock/raw',
    contents_url: 'https://api.github.com/mock/contents',
    ...overrides,
  }) as DiffEntry;

describe('formatPRForResponse', () => {
  const createBasePR = (
    overrides: Partial<GitHubPullRequestItem> = {}
  ): GitHubPullRequestItem => ({
    number: 123,
    title: 'Test PR',
    body: 'Test body',
    state: 'open',
    author: 'testuser',
    labels: ['bug', 'enhancement'],
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-02T00:00:00Z',
    closed_at: null,
    url: 'https://github.com/owner/repo/pull/123',
    comments: [],
    reactions: 5,
    draft: false,
    head: 'feature-branch',
    head_sha: 'abc123',
    base: 'main',
    base_sha: 'def456',
    ...overrides,
  });

  describe('basic fields', () => {
    it('should format basic PR fields correctly', () => {
      const pr = createBasePR();
      const result = formatPRForResponse(pr);

      expect(result.number).toBe(123);
      expect(result.title).toBe('Test PR');
      expect(result.url).toBe('https://github.com/owner/repo/pull/123');
      expect(result.state).toBe('open');
      expect(result.draft).toBe(false);
      expect(result.created_at).toBe('2024-01-01T00:00:00Z');
      expect(result.updated_at).toBe('2024-01-02T00:00:00Z');
      expect(result.author).toBe('testuser');
      expect(result.body).toBe('Test body');
    });

    it('should format head and base refs correctly', () => {
      const pr = createBasePR({
        head: 'feature-branch',
        head_sha: 'abc123def456',
        base: 'main',
        base_sha: 'xyz789012345',
      });
      const result = formatPRForResponse(pr);

      expect(result.head_ref).toBe('feature-branch');
      expect(result.head_sha).toBe('abc123def456');
      expect(result.base_ref).toBe('main');
      expect(result.base_sha).toBe('xyz789012345');
    });

    it('should default head/base refs to empty strings and omit SHAs when undefined', () => {
      const pr = createBasePR({
        head: undefined,
        head_sha: undefined,
        base: undefined,
        base_sha: undefined,
      });
      const result = formatPRForResponse(pr);

      expect(result.head_ref).toBe('');
      expect(result).not.toHaveProperty('head_sha');
      expect(result.base_ref).toBe('');
      expect(result).not.toHaveProperty('base_sha');
    });
  });

  describe('merged state', () => {
    it('should set merged to true when state is closed and merged_at is set', () => {
      const pr = createBasePR({
        state: 'closed',
        merged_at: '2024-01-03T00:00:00Z',
      });
      const result = formatPRForResponse(pr);

      expect(result.merged).toBe(true);
      expect(result.merged_at).toBe('2024-01-03T00:00:00Z');
    });

    it('should set merged to false when state is closed but merged_at is not set', () => {
      const pr = createBasePR({
        state: 'closed',
        closed_at: '2024-01-03T00:00:00Z',
        merged_at: undefined,
      });
      const result = formatPRForResponse(pr);

      expect(result.merged).toBe(false);
      expect(result.merged_at).toBeUndefined();
    });

    it('should set merged to false when state is open', () => {
      const pr = createBasePR({
        state: 'open',
      });
      const result = formatPRForResponse(pr);

      expect(result.merged).toBe(false);
    });
  });

  describe('draft state', () => {
    it('should correctly format draft PRs', () => {
      const pr = createBasePR({ draft: true });
      const result = formatPRForResponse(pr);

      expect(result.draft).toBe(true);
    });

    it('should default draft to false when undefined', () => {
      const pr = createBasePR({ draft: undefined });
      const result = formatPRForResponse(pr);

      expect(result.draft).toBe(false);
    });
  });

  describe('closed_at handling', () => {
    it('should include closed_at when set', () => {
      const pr = createBasePR({
        closed_at: '2024-01-03T00:00:00Z',
      });
      const result = formatPRForResponse(pr);

      expect(result.closed_at).toBe('2024-01-03T00:00:00Z');
    });

    it('should set closed_at to undefined when null', () => {
      const pr = createBasePR({
        closed_at: null,
      });
      const result = formatPRForResponse(pr);

      expect(result.closed_at).toBeUndefined();
    });
  });

  describe('comments and commits counts', () => {
    it('should count comments from array', () => {
      const pr = createBasePR({
        comments: [
          {
            id: '1',
            user: 'user1',
            body: 'Comment 1',
            created_at: '2024-01-01T00:00:00Z',
            updated_at: '2024-01-01T00:00:00Z',
          },
          {
            id: '2',
            user: 'user2',
            body: 'Comment 2',
            created_at: '2024-01-02T00:00:00Z',
            updated_at: '2024-01-02T00:00:00Z',
          },
        ],
      });
      const result = formatPRForResponse(pr);

      expect(result.comments).toBe(2);
    });

    it('should return 0 comments when array is empty', () => {
      const pr = createBasePR({ comments: [] });
      const result = formatPRForResponse(pr);

      expect(result.comments).toBe(0);
    });

    it('should return 0 comments when undefined', () => {
      const pr = createBasePR({ comments: undefined });
      const result = formatPRForResponse(pr);

      expect(result.comments).toBe(0);
    });

    it('should count commits from array', () => {
      const pr = createBasePR({
        commits: [
          {
            sha: 'abc123',
            message: 'Commit 1',
            author: 'author1',
            date: '2024-01-01T00:00:00Z',
            files: [],
          },
          {
            sha: 'def456',
            message: 'Commit 2',
            author: 'author2',
            date: '2024-01-02T00:00:00Z',
            files: [],
          },
          {
            sha: 'ghi789',
            message: 'Commit 3',
            author: 'author3',
            date: '2024-01-03T00:00:00Z',
            files: [],
          },
        ],
      });
      const result = formatPRForResponse(pr);

      expect(result.commits).toBe(3);
    });

    it('should return 0 commits when undefined', () => {
      const pr = createBasePR({ commits: undefined });
      const result = formatPRForResponse(pr);

      expect(result.commits).toBe(0);
    });
  });

  describe('file changes', () => {
    it('should calculate additions, deletions, and changed_files from file_changes', () => {
      const pr = createBasePR({
        file_changes: {
          total_count: 3,
          files: [
            createMockDiffEntry({
              filename: 'file1.ts',
              status: 'modified',
              additions: 10,
              deletions: 5,
              changes: 15,
            }),
            createMockDiffEntry({
              filename: 'file2.ts',
              status: 'added',
              additions: 50,
              deletions: 0,
              changes: 50,
            }),
            createMockDiffEntry({
              filename: 'file3.ts',
              status: 'removed',
              additions: 0,
              deletions: 30,
              changes: 30,
            }),
          ],
        },
      });
      const result = formatPRForResponse(pr);

      expect(result.additions).toBe(60);
      expect(result.deletions).toBe(35);
      expect(result.changed_files).toBe(3);
    });

    it('should return 0 for additions, deletions, changed_files when file_changes is undefined', () => {
      const pr = createBasePR({ file_changes: undefined });
      const result = formatPRForResponse(pr);

      expect(result.additions).toBe(0);
      expect(result.deletions).toBe(0);
      expect(result.changed_files).toBe(0);
    });

    it('should include file_changes array with formatted files', () => {
      const pr = createBasePR({
        file_changes: {
          total_count: 2,
          files: [
            createMockDiffEntry({
              filename: 'src/index.ts',
              status: 'modified',
              additions: 10,
              deletions: 5,
              changes: 15,
              patch: '@@ -1,5 +1,10 @@\n content',
            }),
            createMockDiffEntry({
              filename: 'src/utils.ts',
              status: 'added',
              additions: 20,
              deletions: 0,
              changes: 20,
              patch: '@@ -0,0 +1,20 @@\n new content',
            }),
          ],
        },
      });
      const result = formatPRForResponse(pr);

      expect(result.file_changes).toBeDefined();
      expect(result.file_changes).toHaveLength(2);
      expect(result.file_changes![0]).toEqual({
        filename: 'src/index.ts',
        status: 'modified',
        additions: 10,
        deletions: 5,
        patch: '@@ -1,5 +1,10 @@\n content',
      });
      expect(result.file_changes![1]).toEqual({
        filename: 'src/utils.ts',
        status: 'added',
        additions: 20,
        deletions: 0,
        patch: '@@ -0,0 +1,20 @@\n new content',
      });
    });

    it('should not include file_changes when undefined', () => {
      const pr = createBasePR({ file_changes: undefined });
      const result = formatPRForResponse(pr);

      expect(result.file_changes).toBeUndefined();
    });
  });

  describe('commit details', () => {
    it('should include commit_details when commits are present', () => {
      const pr = createBasePR({
        commits: [
          {
            sha: 'abc123',
            message: 'Fix bug',
            author: 'developer',
            date: '2024-01-01T00:00:00Z',
            files: [
              {
                filename: 'bug.ts',
                status: 'modified',
                additions: 5,
                deletions: 3,
                changes: 8,
              },
            ],
          },
        ],
      });
      const result = formatPRForResponse(pr);

      expect(result.commit_details).toBeDefined();
      expect(result.commit_details).toHaveLength(1);
      expect(result.commit_details?.[0]?.sha).toBe('abc123');
      expect(result.commit_details?.[0]?.message).toBe('Fix bug');
    });

    it('should not include commit_details when commits is undefined', () => {
      const pr = createBasePR({ commits: undefined });
      const result = formatPRForResponse(pr);

      expect(result.commit_details).toBeUndefined();
    });
  });

  describe('sanitization warnings', () => {
    it('should include sanitization warnings when present', () => {
      const pr = createBasePR({
        _sanitization_warnings: [
          'Warning: potentially harmful content detected',
          'Warning: script tags removed',
        ],
      });
      const result = formatPRForResponse(pr);

      expect(result._sanitization_warnings).toBeDefined();
      expect(result._sanitization_warnings).toHaveLength(2);
      expect(result._sanitization_warnings).toContain(
        'Warning: potentially harmful content detected'
      );
    });

    it('should not include sanitization warnings when undefined', () => {
      const pr = createBasePR({ _sanitization_warnings: undefined });
      const result = formatPRForResponse(pr);

      expect(result._sanitization_warnings).toBeUndefined();
    });
  });

  describe('edge cases', () => {
    it('should handle a fully populated PR', () => {
      const pr = createBasePR({
        state: 'closed',
        merged_at: '2024-01-03T00:00:00Z',
        closed_at: '2024-01-03T00:00:00Z',
        comments: [
          {
            id: '1',
            user: 'reviewer',
            body: 'LGTM',
            created_at: '2024-01-02T00:00:00Z',
            updated_at: '2024-01-02T00:00:00Z',
          },
        ],
        commits: [
          {
            sha: 'final',
            message: 'Final commit',
            author: 'dev',
            date: '2024-01-02T12:00:00Z',
            files: [],
          },
        ],
        file_changes: {
          total_count: 1,
          files: [
            createMockDiffEntry({
              filename: 'feature.ts',
              status: 'added',
              additions: 100,
              deletions: 0,
              changes: 100,
            }),
          ],
        },
        _sanitization_warnings: ['Minor warning'],
      });
      const result = formatPRForResponse(pr);

      expect(result.merged).toBe(true);
      expect(result.comments).toBe(1);
      expect(result.commits).toBe(1);
      expect(result.additions).toBe(100);
      expect(result.file_changes).toHaveLength(1);
      expect(result.commit_details).toHaveLength(1);
      expect(result._sanitization_warnings).toHaveLength(1);
    });

    it('should handle a minimal PR with mostly undefined fields', () => {
      const minimalPR: GitHubPullRequestItem = {
        number: 1,
        title: 'Minimal',
        body: undefined,
        state: 'open',
        author: 'user',
        labels: [],
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-01T00:00:00Z',
        closed_at: null,
        url: 'https://github.com/owner/repo/pull/1',
        comments: undefined,
        reactions: 0,
        draft: undefined,
        head: undefined,
        head_sha: undefined,
        base: undefined,
        base_sha: undefined,
      };
      const result = formatPRForResponse(minimalPR);

      expect(result.number).toBe(1);
      expect(result.title).toBe('Minimal');
      expect(result.body).toBeUndefined();
      expect(result.draft).toBe(false);
      expect(result.head_ref).toBe('');
      expect(result.base_ref).toBe('');
      expect(result.comments).toBe(0);
      expect(result.commits).toBe(0);
      expect(result.additions).toBe(0);
      expect(result.deletions).toBe(0);
      expect(result.changed_files).toBe(0);
      expect(result.file_changes).toBeUndefined();
      expect(result.commit_details).toBeUndefined();
      expect(result._sanitization_warnings).toBeUndefined();
    });
  });
});
