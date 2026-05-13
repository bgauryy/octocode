import { describe, it, expect } from 'vitest';
import { extractRepoOwnerFromParams } from '../src/paramExtractors.js';

describe('extractRepoOwnerFromParams', () => {
  describe('Bulk Operations (queries array)', () => {
    describe('Combined repository field format', () => {
      it('should extract single repository from combined format', () => {
        const params = {
          queries: [{ repository: 'facebook/react' }],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });

      it('should extract multiple repositories from combined format', () => {
        const params = {
          queries: [
            { repository: 'facebook/react' },
            { repository: 'microsoft/vscode' },
            { repository: 'vercel/next.js' },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([
          'facebook/react',
          'microsoft/vscode',
          'vercel/next.js',
        ]);
      });

      it('should deduplicate identical repositories', () => {
        const params = {
          queries: [
            { repository: 'facebook/react' },
            { repository: 'facebook/react' },
            { repository: 'microsoft/vscode' },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react', 'microsoft/vscode']);
      });

      it('should skip repository field without slash', () => {
        const params = {
          queries: [
            { repository: 'invalid-repo-format' },
            { repository: 'facebook/react' },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });
    });

    describe('Separate owner/repo fields format', () => {
      it('should extract single repository from owner/repo format', () => {
        const params = {
          queries: [{ owner: 'facebook', repo: 'react' }],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });

      it('should extract multiple repositories from owner/repo format', () => {
        const params = {
          queries: [
            { owner: 'facebook', repo: 'react' },
            { owner: 'microsoft', repo: 'vscode' },
            { owner: 'vercel', repo: 'next.js' },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([
          'facebook/react',
          'microsoft/vscode',
          'vercel/next.js',
        ]);
      });

      it('should skip query with only repo field', () => {
        const params = {
          queries: [{ repo: 'react' }, { owner: 'facebook', repo: 'react' }],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });

      it('should extract owner when repo is non-string', () => {
        const params = {
          queries: [
            { owner: 123, repo: 'react' }, // Both invalid - skipped
            { owner: 'facebook', repo: true }, // Owner valid, repo invalid - extracts owner
            { owner: 'microsoft', repo: 'vscode' }, // Both valid
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook', 'microsoft/vscode']);
      });
    });

    describe('Owner only format', () => {
      it('should extract single owner without repo', () => {
        const params = {
          queries: [{ owner: 'facebook' }],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook']);
      });

      it('should extract multiple owners without repos', () => {
        const params = {
          queries: [{ owner: 'facebook' }, { owner: 'microsoft' }],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook', 'microsoft']);
      });

      it('should deduplicate identical owners', () => {
        const params = {
          queries: [
            { owner: 'facebook' },
            { owner: 'facebook' },
            { owner: 'microsoft' },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook', 'microsoft']);
      });

      it('should skip non-string owner', () => {
        const params = {
          queries: [{ owner: 123 }, { owner: 'facebook' }],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook']);
      });
    });

    describe('Mixed formats', () => {
      it('should handle mix of all three formats', () => {
        const params = {
          queries: [
            { repository: 'facebook/react' },
            { owner: 'microsoft', repo: 'vscode' },
            { owner: 'vercel' },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([
          'facebook/react',
          'microsoft/vscode',
          'vercel',
        ]);
      });

      it('should prefer repository field over owner/repo fields', () => {
        const params = {
          queries: [
            {
              repository: 'facebook/react',
              owner: 'ignored',
              repo: 'ignored',
            },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });

      it('should deduplicate across different formats', () => {
        const params = {
          queries: [
            { repository: 'facebook/react' },
            { owner: 'facebook', repo: 'react' },
            { owner: 'microsoft', repo: 'vscode' },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react', 'microsoft/vscode']);
      });
    });

    describe('Edge cases', () => {
      it('should return empty array for empty queries', () => {
        const params = { queries: [] };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });

      it('should return empty array for queries with no repo info', () => {
        const params = {
          queries: [
            { someOtherField: 'value' },
            { keywordsToSearch: ['test'] },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });

      it('should handle queries with undefined/null values', () => {
        const params = {
          queries: [
            { owner: undefined, repo: 'react' },
            { owner: null, repo: 'vscode' },
            { owner: 'facebook', repo: 'react' },
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });

      it('should handle queries with empty strings', () => {
        const params = {
          queries: [
            { owner: '', repo: 'react' }, // Empty owner - skipped
            { owner: 'facebook', repo: '' }, // Empty repo - extracts owner only
            { owner: 'microsoft', repo: 'vscode' }, // Both valid
          ],
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook', 'microsoft/vscode']);
      });
    });
  });

  describe('Single Operations (direct params)', () => {
    describe('Combined repository field format', () => {
      it('should extract repository from combined format', () => {
        const params = { repository: 'facebook/react' };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });

      it('should skip repository field without slash', () => {
        const params = { repository: 'invalid-format' };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });

      it('should skip non-string repository field', () => {
        const params = { repository: 123 };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });
    });

    describe('Separate owner/repo fields format', () => {
      it('should extract repository from owner/repo format', () => {
        const params = { owner: 'facebook', repo: 'react' };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });

      it('should skip params with only owner', () => {
        const params = { owner: 'facebook' };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook']);
      });

      it('should skip params with only repo', () => {
        const params = { repo: 'react' };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });

      it('should skip non-string owner or repo', () => {
        const params = { owner: 123, repo: 'react' };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });
    });

    describe('Owner only format', () => {
      it('should extract owner without repo', () => {
        const params = { owner: 'facebook' };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook']);
      });

      it('should skip non-string owner', () => {
        const params = { owner: 123 };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });
    });

    describe('Mixed formats', () => {
      it('should prefer repository field over owner/repo fields', () => {
        const params = {
          repository: 'facebook/react',
          owner: 'ignored',
          repo: 'ignored',
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });

      it('should fall back to owner/repo when repository has no slash', () => {
        const params = {
          repository: 'invalid',
          owner: 'facebook',
          repo: 'react',
        };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual(['facebook/react']);
      });
    });

    describe('Edge cases', () => {
      it('should return empty array for params with no repo info', () => {
        const params = { someOtherField: 'value' };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });

      it('should handle undefined/null values', () => {
        const params = { owner: undefined, repo: null };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });

      it('should handle empty strings', () => {
        const params = { owner: '', repo: '' };

        const result = extractRepoOwnerFromParams(params);

        expect(result).toEqual([]);
      });
    });
  });

  describe('No queries parameter', () => {
    it('should check direct params when queries is undefined', () => {
      const params = { owner: 'facebook', repo: 'react' };

      const result = extractRepoOwnerFromParams(params);

      expect(result).toEqual(['facebook/react']);
    });

    it('should check direct params when queries is null', () => {
      const params = { queries: null, owner: 'facebook', repo: 'react' };

      const result = extractRepoOwnerFromParams(params);

      expect(result).toEqual(['facebook/react']);
    });

    it('should check direct params when queries is not an array', () => {
      const params = {
        queries: 'not-an-array',
        owner: 'facebook',
        repo: 'react',
      };

      const result = extractRepoOwnerFromParams(params);

      expect(result).toEqual(['facebook/react']);
    });

    it('should return empty array for completely empty params', () => {
      const params = {};

      const result = extractRepoOwnerFromParams(params);

      expect(result).toEqual([]);
    });
  });

  describe('Real-world scenarios', () => {
    it('should handle github_search_code bulk query', () => {
      const params = {
        queries: [
          {
            id: 'query1',
            owner: 'facebook',
            repo: 'react',
            keywordsToSearch: ['useState'],
          },
          {
            id: 'query2',
            owner: 'microsoft',
            repo: 'vscode',
            keywordsToSearch: ['extension'],
          },
        ],
      };

      const result = extractRepoOwnerFromParams(params);

      expect(result).toEqual(['facebook/react', 'microsoft/vscode']);
    });

    it('should handle github_search_repos query with only owner', () => {
      const params = {
        queries: [
          { id: 'query1', owner: 'vercel', topicsToSearch: ['nextjs'] },
          { id: 'query2', owner: 'facebook', keywordsToSearch: ['react'] },
        ],
      };

      const result = extractRepoOwnerFromParams(params);

      expect(result).toEqual(['vercel', 'facebook']);
    });

    it('should handle github_fetch_content query', () => {
      const params = {
        queries: [
          {
            id: 'query1',
            owner: 'facebook',
            repo: 'react',
            path: 'src/index.js',
          },
        ],
      };

      const result = extractRepoOwnerFromParams(params);

      expect(result).toEqual(['facebook/react']);
    });

    it('should handle mixed tool queries in single call', () => {
      const params = {
        queries: [
          { owner: 'facebook', repo: 'react' },
          { owner: 'vercel' },
          { repository: 'microsoft/vscode' },
        ],
      };

      const result = extractRepoOwnerFromParams(params);

      expect(result).toEqual(['facebook/react', 'vercel', 'microsoft/vscode']);
    });
  });
});
