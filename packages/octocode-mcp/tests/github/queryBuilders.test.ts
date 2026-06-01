import { describe, it, expect } from 'vitest';
import {
  getOwnerQualifier,
  buildCodeSearchQuery,
  buildRepoSearchQuery,
  buildPullRequestSearchQuery,
  shouldUseSearchForPRs,
} from '../../src/github/queryBuilders.js';
import type { GitHubCodeSearchQuery } from '@octocodeai/octocode-core';

const toCodeSearchQuery = (params: {
  keywordsToSearch: string[];
  owner?: string | string[];
  repo?: string | string[];
  extension?: string;
  filename?: string;
  path?: string;
  match?: 'file' | 'path' | Array<'file' | 'path'>;
  limit?: number;
  minify?: boolean;
}): GitHubCodeSearchQuery => params as GitHubCodeSearchQuery;

describe('Query Builders', () => {
  describe('getOwnerQualifier', () => {
    it('should always use user: qualifier as it matches both users and orgs', () => {
      expect(getOwnerQualifier('my-org')).toBe('user:my-org');
      expect(getOwnerQualifier('my_org')).toBe('user:my_org');
      expect(getOwnerQualifier('myorg')).toBe('user:myorg');
      expect(getOwnerQualifier('john')).toBe('user:john');
      expect(getOwnerQualifier('Organization')).toBe('user:Organization');
    });
  });

  describe('buildCodeSearchQuery', () => {
    it('should build basic query with terms', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['function', 'auth'],

        minify: true,
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('function auth');
    });

    it('should build query with owner and repo', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['test'],
        owner: 'microsoft',
        repo: 'vscode',

        minify: true,
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('test repo:microsoft/vscode');
    });

    it('should build query with owner only', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['test'],
        owner: 'google',

        minify: true,
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('test user:google');
    });

    it('should build query with multiple owners and repos', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['test'],
        owner: ['microsoft', 'google'],
        repo: ['vscode', 'typescript'],

        minify: true,
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe(
        'test repo:microsoft/vscode repo:microsoft/typescript repo:google/vscode repo:google/typescript'
      );
    });

    it('should build query with file filters', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['test'],
        filename: 'package.json',
        extension: 'ts',
        path: 'src/',

        minify: true,
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('test filename:package.json extension:ts path:"src/"');
    });

    it('should quote path values containing slashes', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['export'],
        owner: 'bgauryy',
        repo: 'octocode-mcp',
        path: 'src/tools',
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toContain('path:"src/tools"');
    });

    it('should not quote simple path values without special chars', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['test'],
        path: 'src',
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toContain('path:src');
      expect(query).not.toContain('path:"src"');
    });

    it('should build query with match filters', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['test'],
        match: ['file', 'path'],

        minify: true,
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('test in:file in:path');
    });

    it('should build query with single match filter', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['test'],
        match: 'file',

        minify: true,
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('test in:file');
    });

    it('should handle empty query terms', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: [],
        owner: 'microsoft',

        minify: true,
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('user:microsoft');
    });

    it('should quote keywords containing @ character', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['@scope/package'],
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('"@scope/package"');
    });

    it('should quote keywords containing / character', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['owner/repo'],
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('"owner/repo"');
    });

    it('should not quote regular keywords', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['express', 'middleware'],
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('express middleware');
    });

    it('should not double-quote already quoted keywords', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['"@scope/package"'],
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('"@scope/package"');
    });

    it('should handle mix of special and regular keywords', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['express', '@types/node', 'middleware'],
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('express "@types/node" middleware');
    });

    it('should quote multi-word keywords as a phrase (SC-1)', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['export function parse'],
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('"export function parse"');
    });

    it('should quote a phrase and split a file path into filename: + dir (SC-1/SC-2)', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['const patch'],
        owner: 'vuejs',
        repo: 'core',
        path: 'packages/runtime-core/src/renderer.ts',
      });

      const query = buildCodeSearchQuery(params);
      // Phrase is still quoted...
      expect(query).toContain('"const patch"');
      // ...and the file path is rewritten to the qualifiers GitHub honors.
      // GitHub `path:` matches a file's DIRECTORY only; a full `dir/file.ext`
      // returns zero (proven at the API, quoted or not). filename: + dir works.
      expect(query).toContain('filename:renderer.ts');
      expect(query).toContain('path:"packages/runtime-core/src"');
      expect(query).not.toContain('renderer.ts"');
    });

    describe('file-path rewrite (SC-2): path: matches directories, not files', () => {
      it('splits a path pointing at a file into filename: + directory path:', () => {
        const query = buildCodeSearchQuery(
          toCodeSearchQuery({
            keywordsToSearch: ['createRenderer'],
            path: 'packages/runtime-core/src/renderer.ts',
          })
        );
        expect(query).toContain('filename:renderer.ts');
        expect(query).toContain('path:"packages/runtime-core/src"');
        // never the broken full-file path: qualifier
        expect(query).not.toMatch(/path:"?[^"\s]*\/renderer\.ts/);
      });

      it('drops path: entirely when the file path has no directory part', () => {
        const query = buildCodeSearchQuery(
          toCodeSearchQuery({ keywordsToSearch: ['x'], path: 'renderer.ts' })
        );
        expect(query).toBe('x filename:renderer.ts');
      });

      it('handles compound extensions (foo.test.ts)', () => {
        const query = buildCodeSearchQuery(
          toCodeSearchQuery({
            keywordsToSearch: ['x'],
            path: 'src/foo.test.ts',
          })
        );
        expect(query).toContain('filename:foo.test.ts');
        expect(query).toContain('path:src');
      });

      it('does NOT split a plain directory path', () => {
        const query = buildCodeSearchQuery(
          toCodeSearchQuery({
            keywordsToSearch: ['x'],
            path: 'packages/runtime-core/src',
          })
        );
        expect(query).toContain('path:"packages/runtime-core/src"');
        expect(query).not.toContain('filename:');
      });

      it('does NOT split a directory whose name looks like a version (src/v1.2)', () => {
        const query = buildCodeSearchQuery(
          toCodeSearchQuery({ keywordsToSearch: ['x'], path: 'src/v1.2' })
        );
        expect(query).toContain('path:"src/v1.2"');
        expect(query).not.toContain('filename:');
      });

      it('does NOT clobber an explicitly provided filename', () => {
        const query = buildCodeSearchQuery(
          toCodeSearchQuery({
            keywordsToSearch: ['x'],
            filename: 'index.ts',
            path: 'src/renderer.ts',
          })
        );
        expect(query).toContain('filename:index.ts');
        expect(query).not.toContain('filename:renderer.ts');
      });
    });

    it('should quote punctuation-heavy keywords so they match literally (SC-3)', () => {
      expect(
        buildCodeSearchQuery(
          toCodeSearchQuery({ keywordsToSearch: ['$state'] })
        )
      ).toBe('"$state"');
      expect(
        buildCodeSearchQuery(
          toCodeSearchQuery({ keywordsToSearch: ['React.useState'] })
        )
      ).toBe('"React.useState"');
      expect(
        buildCodeSearchQuery(
          toCodeSearchQuery({ keywordsToSearch: ['$ZodAsyncError'] })
        )
      ).toBe('"$ZodAsyncError"');
    });

    it('should still leave bare identifiers (alnum/_/-) unquoted', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['baseCreateRenderer', 'attach_ping-listener'],
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('baseCreateRenderer attach_ping-listener');
    });

    it('should escape embedded double quotes when wrapping a keyword', () => {
      const params = toCodeSearchQuery({
        keywordsToSearch: ['say "hi"'],
      });

      const query = buildCodeSearchQuery(params);
      expect(query).toBe('"say \\"hi\\""');
    });
  });

  describe('buildRepoSearchQuery', () => {
    it('should build basic repo search query', () => {
      const params = {
        keywordsToSearch: ['todo', 'app'],
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('todo app is:not-archived');
    });

    it('should build query with topicsToSearch', () => {
      const params = {
        keywordsToSearch: ['app'],
        topicsToSearch: ['react', 'typescript'],
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('app topic:react topic:typescript is:not-archived');
    });

    it('should build query with single topic', () => {
      const params: Parameters<typeof buildRepoSearchQuery>[0] = {
        keywordsToSearch: ['framework'],
        topicsToSearch: ['javascript'],
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('framework topic:javascript is:not-archived');
    });

    it('should build query with repository metrics', () => {
      const params = {
        keywordsToSearch: ['library'],
        stars: '>1000',
        size: '<10000',
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('library stars:>1000 size:<10000 is:not-archived');
    });

    it('should build query with match filters', () => {
      const params = {
        keywordsToSearch: ['awesome'],
        match: ['name', 'description'],
      } as Parameters<typeof buildRepoSearchQuery>[0];

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('awesome in:name in:description is:not-archived');
    });

    it('should map updated to pushed', () => {
      const params = {
        keywordsToSearch: ['active'],
        updated: '>2023-01-01',
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('active pushed:>2023-01-01 is:not-archived');
    });

    it('should build query with readme match filter', () => {
      const params = {
        keywordsToSearch: ['awesome'],
        match: ['readme'],
      } as Parameters<typeof buildRepoSearchQuery>[0];

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('awesome in:readme is:not-archived');
    });

    it('should build query with single readme match', () => {
      const params = {
        keywordsToSearch: ['awesome'],
        match: ['readme'],
      } as Parameters<typeof buildRepoSearchQuery>[0];

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('awesome in:readme is:not-archived');
    });

    it('should build query with created date filter', () => {
      const params = {
        keywordsToSearch: ['repo'],
        created: '>2023-01-01',
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('repo created:>2023-01-01 is:not-archived');
    });

    it('should build query with owner only (no keywords or topics)', () => {
      const params = {
        owner: 'facebook',
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toContain('user:facebook');
      expect(query).toContain('is:not-archived');
    });

    it('should build query with stars range (100..500)', () => {
      const params = {
        keywordsToSearch: ['react'],
        stars: '100..500',
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('react stars:100..500 is:not-archived');
    });

    it('should build query with stars >=1000', () => {
      const params = {
        keywordsToSearch: ['react'],
        stars: '>=1000',
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('react stars:>=1000 is:not-archived');
    });

    it('should quote scoped package keywords with @ and /', () => {
      const params = {
        keywordsToSearch: ['@scope/package'],
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('"@scope/package" is:not-archived');
    });

    it('should quote keywords with / in repo search', () => {
      const params = {
        keywordsToSearch: ['facebook/react'],
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('"facebook/react" is:not-archived');
    });

    it('should not quote normal repo search keywords', () => {
      const params = {
        keywordsToSearch: ['react', 'typescript'],
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('react typescript is:not-archived');
    });

    it('should include language filter when language is provided', () => {
      const params = {
        keywordsToSearch: ['testing'],
        language: 'TypeScript',
      } as Parameters<typeof buildRepoSearchQuery>[0];

      const query = buildRepoSearchQuery(params);
      expect(query).toContain('language:TypeScript');
      expect(query).toBe('testing language:TypeScript is:not-archived');
    });

    it('should combine language with topics', () => {
      const params = {
        topicsToSearch: ['testing'],
        language: 'TypeScript',
        stars: '>1000',
        created: '>=2022-01-01',
      } as Parameters<typeof buildRepoSearchQuery>[0];

      const query = buildRepoSearchQuery(params);
      expect(query).toContain('language:TypeScript');
      expect(query).toContain('topic:testing');
      expect(query).toContain('stars:>1000');
      expect(query).toContain('created:>=2022-01-01');
    });

    it('should omit language qualifier when language is not provided', () => {
      const params = {
        keywordsToSearch: ['testing'],
      };

      const query = buildRepoSearchQuery(params);
      expect(query).not.toContain('language:');
    });

    it('should exclude archived repos by default (archived omitted)', () => {
      const params = {
        keywordsToSearch: ['recoil'],
      };

      const query = buildRepoSearchQuery(params);
      expect(query).toContain('is:not-archived');
      expect(query).not.toContain('archived:true');
    });

    it('should exclude archived repos when archived:false', () => {
      const params = {
        keywordsToSearch: ['recoil'],
        archived: false,
      } as Parameters<typeof buildRepoSearchQuery>[0];

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('recoil is:not-archived');
    });

    it('should opt into archived repos when archived:true', () => {
      const params = {
        keywordsToSearch: ['recoil'],
        archived: true,
      } as Parameters<typeof buildRepoSearchQuery>[0];

      const query = buildRepoSearchQuery(params);
      expect(query).toBe('recoil archived:true');
      expect(query).not.toContain('is:not-archived');
    });
  });

  describe('buildPullRequestSearchQuery', () => {
    it('should build basic PR search query', () => {
      const params = {
        query: 'bug fix',
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe('bug fix is:pr archived:false');
    });

    it('should build query with state filters', () => {
      const params = {
        state: 'open' as const,
        draft: true,
        merged: false,
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe('is:pr is:open is:draft is:unmerged archived:false');
    });

    it('should opt into PRs from archived repos when archived:true', () => {
      const params = {
        query: 'bug fix',
        archived: true,
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe('bug fix is:pr archived:true');
    });

    it('should exclude PRs from archived repos by default', () => {
      const params = {
        query: 'bug fix',
        archived: false,
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe('bug fix is:pr archived:false');
    });

    it('should build query with user filters', () => {
      const params = {
        author: 'john',
        assignee: 'alice',
        mentions: 'bob',
        commenter: 'charlie',
        'reviewed-by': 'dave',
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe(
        'is:pr author:john assignee:alice mentions:bob commenter:charlie reviewed-by:dave archived:false'
      );
    });

    it('should build query with branch filters', () => {
      const params = {
        head: 'feature-branch',
        base: 'main',
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe('is:pr head:feature-branch base:main archived:false');
    });

    it('should build query with engagement filters', () => {
      const params = {
        comments: '>5',
        reactions: '>10',
        interactions: '>20',
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe(
        'is:pr comments:>5 reactions:>10 interactions:>20 archived:false'
      );
    });

    it('applies matchScope as an in: qualifier when a query term is present', () => {
      const params = {
        query: 'Suspense',
        match: ['title'] as ('title' | 'body' | 'comments')[],
      };
      const query = buildPullRequestSearchQuery(params);
      expect(query).toContain('Suspense');
      expect(query).toContain('in:title');
    });

    it('joins multiple matchScope values into one comma-separated in: qualifier', () => {
      const params = {
        query: 'Suspense',
        match: ['title', 'body'] as ('title' | 'body' | 'comments')[],
      };
      const query = buildPullRequestSearchQuery(params);
      expect(query).toContain('in:title,body');
      expect(query).not.toContain('in:title in:body');
    });

    it('omits in: when matchScope is set but there is no free-text query to scope', () => {
      const params = {
        match: ['title'] as ('title' | 'body' | 'comments')[],
        state: 'open' as const,
      };
      const query = buildPullRequestSearchQuery(params);
      expect(query).not.toContain('in:');
    });

    it('omits in: when no matchScope is provided', () => {
      const params = { query: 'Suspense' };
      const query = buildPullRequestSearchQuery(params);
      expect(query).not.toContain('in:');
    });

    it('should build query with label filters', () => {
      const params = {
        label: ['bug', 'enhancement'],
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe(
        'is:pr label:"bug" label:"enhancement" archived:false'
      );
    });

    it('should build query with negative filters', () => {
      const params = {
        'no-assignee': true,
        'no-label': true,
        'no-milestone': true,
        'no-project': true,
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe(
        'is:pr no:assignee no:label no:milestone no:project archived:false'
      );
    });

    it('should build query with all date filters', () => {
      const params = {
        created: '>2023-01-01',
        updated: '2023-01-01..2023-12-31',
        'author-date': '>2023-01-01',
        'committer-date': '>2023-01-01',
        'merged-at': '>2023-06-01',
        closed: '<2023-12-31',
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toContain('created:>2023-01-01');
      expect(query).toContain('updated:2023-01-01..2023-12-31');
      expect(query).toContain('author-date:>2023-01-01');
      expect(query).toContain('committer-date:>2023-01-01');
      expect(query).toContain('merged:>2023-06-01');
      expect(query).toContain('closed:<2023-12-31');
    });

    it('should build query with involves user filter', () => {
      const params = {
        involves: 'alice',
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe('is:pr involves:alice archived:false');
    });

    it('should build query with review-requested filter', () => {
      const params = {
        'review-requested': 'bob',
      };

      const query = buildPullRequestSearchQuery(params);
      expect(query).toBe('is:pr review-requested:bob archived:false');
    });
  });

  describe('shouldUseSearchForPRs', () => {
    it('should return false for simple list operations', () => {
      const params = {
        owner: 'microsoft',
        repo: 'vscode',
        state: 'open' as const,
      };

      expect(shouldUseSearchForPRs(params)).toBe(false);
    });

    it('should return true when draft filter is used', () => {
      const params = {
        draft: true,
      };

      expect(shouldUseSearchForPRs(params)).toBe(true);
    });

    it('should return true when author filter is used', () => {
      const params = {
        author: 'john',
      };

      expect(shouldUseSearchForPRs(params)).toBe(true);
    });

    it('should return true when query is provided', () => {
      const params = {
        query: 'bug fix',
      };

      expect(shouldUseSearchForPRs(params)).toBe(true);
    });

    it('should return true when labels are specified', () => {
      const params = {
        label: ['bug', 'enhancement'],
      };

      expect(shouldUseSearchForPRs(params)).toBe(true);
    });

    it('should return true when complex filters are used', () => {
      const params = {
        reactions: '>10',
        comments: '>5',
        'reviewed-by': 'alice',
      };

      expect(shouldUseSearchForPRs(params)).toBe(true);
    });

    it('should return true when multiple owners/repos are specified', () => {
      const params = {
        owner: ['microsoft', 'google'],
        repo: 'vscode',
      };

      expect(shouldUseSearchForPRs(params)).toBe(true);
    });

    it('should return true when date filters are used', () => {
      const params = {
        created: '>2023-01-01',
        updated: '2023-01-01..2023-12-31',
      };

      expect(shouldUseSearchForPRs(params)).toBe(true);
    });

    it('should return true when negative filters are used', () => {
      const params = {
        'no-assignee': true,
        'no-label': true,
      };

      expect(shouldUseSearchForPRs(params)).toBe(true);
    });
  });
});
