import { z } from 'zod/v4';
import { describe, expect, it } from 'vitest';
import {
  FileContentQueryBaseLocalSchema,
  GitHubCodeSearchQueryLocalSchema,
  GitHubReposSearchSingleQueryLocalSchema,
  GitHubPullRequestSearchQueryLocalSchema,
  GitHubViewRepoStructureQueryLocalSchema,
  PackageSearchQueryLocalSchema,
} from '../../src/scheme/remoteSchemaOverlay.js';
import {
  FetchContentQuerySchema,
  FindFilesQuerySchema,
  RipgrepQuerySchema,
  ViewStructureQuerySchema,
} from '../../src/scheme/localSchemaOverlay.js';
import {
  LSPGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema,
} from '../../src/scheme/lspSchemaOverlay.js';

// #C1: bare `z.number().int()` fields serialize as
// {minimum:-9007199254740991, maximum:9007199254740991} — schema bloat in every
// published inputSchema AND a validation gap (negatives / absurd values pass).
// Every numeric field must carry meaningful bounds.
const SENTINEL = 9007199254740991;

const schemas: Record<string, z.ZodTypeAny> = {
  'fileContent(remote)': FileContentQueryBaseLocalSchema,
  'code(remote)': GitHubCodeSearchQueryLocalSchema,
  'repos(remote)': GitHubReposSearchSingleQueryLocalSchema,
  'pullRequests(remote)': GitHubPullRequestSearchQueryLocalSchema,
  'viewRepoStructure(remote)': GitHubViewRepoStructureQueryLocalSchema,
  'packageSearch(remote)': PackageSearchQueryLocalSchema,
  'fetchContent(local)': FetchContentQuerySchema,
  findFiles: FindFilesQuerySchema,
  ripgrep: RipgrepQuerySchema,
  viewStructure: ViewStructureQuerySchema,
  lspGoto: LSPGotoDefinitionQuerySchema,
  lspRefs: LSPFindReferencesQuerySchema,
  lspCalls: LSPCallHierarchyQuerySchema,
};

describe('numeric schema fields are bounded (#C1)', () => {
  for (const [name, schema] of Object.entries(schemas)) {
    it(`${name}: no field uses the ±MAX_SAFE_INTEGER sentinel as a bound`, () => {
      const js = z.toJSONSchema(schema) as {
        properties?: Record<string, { minimum?: number; maximum?: number }>;
      };
      const props = js.properties ?? {};
      const offenders = Object.entries(props)
        .filter(
          ([, v]) =>
            v &&
            (Math.abs(v.minimum ?? 0) === SENTINEL ||
              Math.abs(v.maximum ?? 0) === SENTINEL)
        )
        .map(([k]) => k);
      expect(offenders).toEqual([]);
    });
  }

  it('clamps matchStringContextLines:120 to 100 instead of rejecting (FC-2)', () => {
    const r = FileContentQueryBaseLocalSchema.safeParse({
      owner: 'o',
      repo: 'r',
      path: 'a.ts',
      matchString: 'foo',
      matchStringContextLines: 120,
    });
    expect(r.success).toBe(true);
    if (r.success) {
      expect(
        (r.data as { matchStringContextLines?: number }).matchStringContextLines
      ).toBe(100);
    }
  });

  it('clamps a negative line number instead of rejecting it', () => {
    const r = LSPGotoDefinitionQuerySchema.safeParse({
      uri: 'a.ts',
      symbolName: 'x',
      lineHint: -5,
    });
    // Out-of-range magnitudes clamp (to the min), so lineHint is never the
    // validation offender. If the parse fails it is for some other field.
    if (r.success) {
      expect(r.data.lineHint).toBe(1);
    } else {
      const paths = r.error.issues.map(i => i.path.join('.'));
      expect(paths).not.toContain('lineHint');
    }
  });

  // The top-level sweep above only inspects `js.properties[*]`; nested array
  // item types slip through. PR `partialContentMetadata[].additions/deletions`
  // are bare upstream ints — assert the overlay re-bounds them.
  it('pullRequests: nested partialContentMetadata line arrays are bounded (no sentinel)', () => {
    const js = z.toJSONSchema(GitHubPullRequestSearchQueryLocalSchema) as {
      properties?: Record<
        string,
        {
          items?: {
            properties?: Record<
              string,
              { items?: { minimum?: number; maximum?: number } }
            >;
          };
        }
      >;
    };
    const pcm = js.properties?.partialContentMetadata?.items?.properties ?? {};
    for (const key of ['additions', 'deletions']) {
      const itemBounds = pcm[key]?.items;
      expect(itemBounds, `${key} should be present`).toBeDefined();
      expect(Math.abs(itemBounds?.minimum ?? 0)).not.toBe(SENTINEL);
      expect(Math.abs(itemBounds?.maximum ?? 0)).not.toBe(SENTINEL);
      expect(itemBounds?.maximum).toBe(1_000_000_000);
    }
  });
});
