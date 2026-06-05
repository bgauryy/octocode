import { describe, it, expect } from 'vitest';

import {
  BulkRipgrepQuerySchema,
  BulkFindFilesSchema,
  BulkFetchContentQuerySchema,
  BulkViewStructureSchema,
} from '../../src/scheme/localSchemaOverlay.js';
import {
  FileContentBulkQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubViewRepoStructureBulkQueryLocalSchema,
  GitHubReposSearchBulkQueryLocalSchema,
  GitHubPullRequestSearchBulkQueryLocalSchema,
  PackageSearchBulkQueryLocalSchema,
  BulkCloneRepoLocalSchema,
} from '../../src/scheme/remoteSchemaOverlay.js';
import {
  BulkLSPGotoDefinitionQuerySchema,
  BulkLSPFindReferencesQuerySchema,
  BulkLSPCallHierarchyQuerySchema,
} from '../../src/scheme/lspSchemaOverlay.js';

/**
 * Every bulk-envelope schema must validate and clamp its `page` parameter.
 * The char-based responseCharOffset/responseCharLength envelope fields have been
 * removed in favour of page-based pagination with fixed page sizes.
 */
const ALL_BULK_SCHEMAS = [
  ['BulkRipgrepQuerySchema', BulkRipgrepQuerySchema],
  ['BulkFindFilesSchema', BulkFindFilesSchema],
  ['BulkFetchContentQuerySchema', BulkFetchContentQuerySchema],
  ['BulkViewStructureSchema', BulkViewStructureSchema],
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
  ['BulkLSPGotoDefinitionQuerySchema', BulkLSPGotoDefinitionQuerySchema],
  ['BulkLSPFindReferencesQuerySchema', BulkLSPFindReferencesQuerySchema],
  ['BulkLSPCallHierarchyQuerySchema', BulkLSPCallHierarchyQuerySchema],
] as const;

describe('bulk envelope numeric bounds', () => {
  describe.each(ALL_BULK_SCHEMAS)('%s', (_name, schema) => {
    const baseQueries = [{ id: 'q1' }];

    it('parses with minimal queries (envelope accepted, per-query errors ok)', () => {
      const result = schema.safeParse({ queries: baseQueries });
      // Some schemas may fail on missing required per-query fields, but
      // the envelope itself (array length, max-queries) must not be the failure.
      // If parsing fails, errors must come from inside queries[n].field, not
      // from the queries array itself (e.g. too many/few items).
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
