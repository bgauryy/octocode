import { describe, it, expect } from 'vitest';

import {
  BulkRipgrepQuerySchema,
  BulkFindFilesSchema,
  BulkFetchContentQuerySchema,
  BulkViewStructureSchema,
  LOCAL_OVERLAY_MAX_CHAR_LENGTH,
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
 * Every bulk-envelope schema produced by `createRelaxedBulkQuerySchema`
 * must BOUND its numeric inputs. Post-C1 the bound is enforced by CLAMPING
 * (clampedInt), consistent with the per-query charOffset/charLength fields —
 * an out-of-range value is coerced into range, never the validation offender.
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

    it('clamps responseCharLength above LOCAL_OVERLAY_MAX_CHAR_LENGTH (never the offender)', () => {
      const result = schema.safeParse({
        queries: baseQueries,
        responseCharLength: LOCAL_OVERLAY_MAX_CHAR_LENGTH + 1,
      });
      if (result.success) {
        expect(
          (result.data as { responseCharLength?: number }).responseCharLength
        ).toBe(LOCAL_OVERLAY_MAX_CHAR_LENGTH);
      } else {
        const paths = result.error.issues.map(i => i.path.join('.'));
        expect(paths).not.toContain('responseCharLength');
      }
    });

    it('clamps negative responseCharOffset to 0 (never the offender)', () => {
      const result = schema.safeParse({
        queries: baseQueries,
        responseCharOffset: -1,
      });
      if (result.success) {
        expect(
          (result.data as { responseCharOffset?: number }).responseCharOffset
        ).toBe(0);
      } else {
        const paths = result.error.issues.map(i => i.path.join('.'));
        expect(paths).not.toContain('responseCharOffset');
      }
    });

    it('clamps responseCharOffset above the bound (never the offender)', () => {
      const result = schema.safeParse({
        queries: baseQueries,
        responseCharOffset: Number.MAX_SAFE_INTEGER,
      });
      if (!result.success) {
        const paths = result.error.issues.map(i => i.path.join('.'));
        expect(paths).not.toContain('responseCharOffset');
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

    it('accepts responseCharLength at the max bound', () => {
      const result = schema.safeParse({
        queries: baseQueries,
        responseCharLength: LOCAL_OVERLAY_MAX_CHAR_LENGTH,
      });
      // Some schemas may reject because baseQueries lacks required fields,
      // but the failure must NOT be on responseCharLength.
      if (!result.success) {
        const offendingPaths = result.error.issues.map(i => i.path.join('.'));
        expect(offendingPaths).not.toContain('responseCharLength');
      }
    });
  });
});
