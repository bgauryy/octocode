import { describe, it, expect } from 'vitest';

import { LocalRipgrepBulkQuerySchema } from '../../src/tools/local_ripgrep/scheme.js';
import { LocalFindFilesBulkQuerySchema } from '../../src/tools/local_find_files/scheme.js';
import { LocalFetchContentBulkQuerySchema } from '../../src/tools/local_fetch_content/scheme.js';
import { LocalViewStructureBulkQuerySchema } from '../../src/tools/local_view_structure/scheme.js';
import { FileContentBulkQueryLocalSchema } from '../../src/tools/github_fetch_content/scheme.js';
import { GitHubCodeSearchBulkQueryLocalSchema } from '../../src/tools/github_search_code/scheme.js';
import { GitHubViewRepoStructureBulkQueryLocalSchema } from '../../src/tools/github_view_repo_structure/scheme.js';
import { GitHubReposSearchBulkQueryLocalSchema } from '../../src/tools/github_search_repos/scheme.js';
import { GitHubPullRequestSearchBulkQueryLocalSchema } from '../../src/tools/github_search_pull_requests/scheme.js';
import { PackageSearchBulkQueryLocalSchema } from '../../src/tools/package_search/scheme.js';
import { BulkCloneRepoLocalSchema } from '../../src/tools/github_clone_repo/scheme.js';
import { BulkLspGetSemanticContentQuerySchema } from '../../src/tools/lsp/semantic_content/scheme.js';

const ALL_BULK_SCHEMAS = [
  ['LocalRipgrepBulkQuerySchema', LocalRipgrepBulkQuerySchema],
  ['LocalFindFilesBulkQuerySchema', LocalFindFilesBulkQuerySchema],
  ['LocalFetchContentBulkQuerySchema', LocalFetchContentBulkQuerySchema],
  ['LocalViewStructureBulkQuerySchema', LocalViewStructureBulkQuerySchema],
  ['FileContentBulkQueryLocalSchema', FileContentBulkQueryLocalSchema],
  [
    'GitHubCodeSearchBulkQueryLocalSchema',
    GitHubCodeSearchBulkQueryLocalSchema,
  ],
  [
    'GitHubViewRepoStructureBulkQueryLocalSchema',
    GitHubViewRepoStructureBulkQueryLocalSchema,
  ],
  [
    'GitHubReposSearchBulkQueryLocalSchema',
    GitHubReposSearchBulkQueryLocalSchema,
  ],
  [
    'GitHubPullRequestSearchBulkQueryLocalSchema',
    GitHubPullRequestSearchBulkQueryLocalSchema,
  ],
  ['PackageSearchBulkQueryLocalSchema', PackageSearchBulkQueryLocalSchema],
  ['BulkCloneRepoLocalSchema', BulkCloneRepoLocalSchema],
  [
    'BulkLspGetSemanticContentQuerySchema',
    BulkLspGetSemanticContentQuerySchema,
  ],
] as const;

describe('bulk envelope numeric bounds', () => {
  describe.each(ALL_BULK_SCHEMAS)('%s', (_name, schema) => {
    const baseQueries = [{ id: 'q1' }];

    it('parses with minimal queries (envelope accepted, per-query errors ok)', () => {
      const result = schema.safeParse({ queries: baseQueries });
      if (!result.success) {
        const envelopeErrors = result.error.issues.filter(
          i => i.path.length === 1 && i.path[0] === 'queries'
        );
        expect(envelopeErrors).toHaveLength(0);
      }
    });

    it('does not expose responseCharOffset or responseCharLength', () => {
      const result = schema.safeParse({ queries: baseQueries });
      if (result.success) {
        expect(result.data).not.toHaveProperty('responseCharOffset');
        expect(result.data).not.toHaveProperty('responseCharLength');
      }
    });

    it('rejects more than five queries', () => {
      const result = schema.safeParse({
        queries: Array.from({ length: 6 }, (_, index) => ({
          id: `q${index + 1}`,
        })),
      });
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(
          result.error.issues.some(issue => issue.path.join('.') === 'queries')
        ).toBe(true);
      }
    });
  });
});
