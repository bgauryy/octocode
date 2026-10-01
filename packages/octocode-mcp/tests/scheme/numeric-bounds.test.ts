import { z } from 'zod';
import { describe, expect, it } from 'vitest';
import { FileContentQueryLocalSchema } from '@octocodeai/config/schema';
import { GitHubCodeSearchQueryLocalSchema } from '@octocodeai/config/schema';
import { GitHubReposSearchSingleQueryLocalSchema } from '@octocodeai/config/schema';
import { SearchPullRequestsLocalSchema } from '@octocodeai/config/schema';
import { GitHubViewRepoStructureQueryLocalSchema } from '@octocodeai/config/schema';
import { ArtifactSearchQueryLocalSchema } from '@octocodeai/config/schema';
import { LocalFetchContentQuerySchema } from '@octocodeai/config/schema';
import { StructureFilesQuerySchema } from '@octocodeai/config/schema';
import { LocalSearchQuerySchema } from '@octocodeai/config/schema';
import { StructureTreeQuerySchema } from '@octocodeai/config/schema';
import { LspSearchQuerySchema } from '@octocodeai/config/schema';

const SENTINEL = 9007199254740991;

const schemas: Record<string, z.ZodTypeAny> = {
  'fileContent(remote)': FileContentQueryLocalSchema,
  'code(remote)': GitHubCodeSearchQueryLocalSchema,
  'repos(remote)': GitHubReposSearchSingleQueryLocalSchema,
  'pullRequests(remote)': SearchPullRequestsLocalSchema,
  'viewRepoStructure(remote)': GitHubViewRepoStructureQueryLocalSchema,
  'artifactSearch(remote)': ArtifactSearchQueryLocalSchema,
  'fetchContent(local)': LocalFetchContentQuerySchema,
  structureFiles: StructureFilesQuerySchema,
  localSearch: LocalSearchQuerySchema,
  structureTree: StructureTreeQuerySchema,
  lspSemantic: LspSearchQuerySchema,
};

describe('numeric schema fields are bounded (#C1)', () => {
  for (const [name, schema] of Object.entries(schemas)) {
    it(`${name}: result caps avoid sentinel bounds; view offsets use safe integers`, () => {
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
      expect(offenders).toEqual(
        ['fetchContent(local)', 'fileContent(remote)'].includes(name)
          ? ['offset']
          : []
      );
    });
  }

  it('local view offsets accept safe integers and reject fractional or unsafe values', () => {
    const query = {
      goal: 'test goal',
      reasoning: 'exercise offset bounds',
      path: '/fixture.txt',
      chunkType: 'bytes' as const,
    };
    expect(
      LocalFetchContentQuerySchema.safeParse({ ...query, offset: SENTINEL })
        .success
    ).toBe(true);
    for (const offset of [-1, 0.5, SENTINEL + 1]) {
      expect(
        LocalFetchContentQuerySchema.safeParse({ ...query, offset }).success
      ).toBe(false);
    }
  });

  it('github.code rejects page 0 instead of rewriting caller input', () => {
    const r = GitHubCodeSearchQueryLocalSchema.safeParse({
      keywords: ['x'],
      page: 0,
    });
    expect(r.success).toBe(false);
  });

  it('rejects contextLines above the documented maximum', () => {
    const query = (contextLines: number) => ({
      goal: 'test goal',
      reasoning: 'exercise contextLines bounds',
      owner: 'o',
      repo: 'r',
      path: 'a.ts',
      matchString: 'foo',
      contextLines,
    });
    const maximum = (
      z.toJSONSchema(FileContentQueryLocalSchema) as {
        properties: { contextLines: { maximum: number } };
      }
    ).properties.contextLines.maximum;
    // Values between the runtime clamp and the maximum are accepted (and
    // clamped natively); only values past the published maximum reject.
    expect(FileContentQueryLocalSchema.safeParse(query(120)).success).toBe(
      true
    );
    expect(FileContentQueryLocalSchema.safeParse(query(maximum)).success).toBe(
      true
    );
    const r = FileContentQueryLocalSchema.safeParse(query(maximum + 1));
    expect(r.success).toBe(false);
    if (!r.success)
      expect(r.error.issues.map(i => i.path.join('.'))).toEqual([
        'contextLines',
      ]);
  });

  it('rejects a negative LSP line without changing the observed anchor', () => {
    const r = LspSearchQuerySchema.safeParse({
      uri: 'a.ts',
      operation: 'definition',
      symbolName: 'x',
      lineHint: -5,
    });
    expect(r.success).toBe(false);
  });

  it('pullRequests: content.patches.ranges line arrays are bounded (reject above the cap)', () => {
    // The SENTINEL is above the 1e9 line-number cap -> rejected as too_big,
    // and the cap is never the ±MAX_SAFE_INTEGER sentinel.
    const r = SearchPullRequestsLocalSchema.safeParse({
      goal: 'test goal',
      reasoning: 'exercise patch line bounds',
      owner: 'o',
      repo: 'r',
      prNumber: 1,
      content: {
        patches: {
          mode: 'selected',
          ranges: [
            {
              file: 'a.ts',
              additions: [SENTINEL],
              deletions: [SENTINEL],
            },
          ],
        },
      },
    });

    expect(r.success).toBe(false);
    if (!r.success) {
      const tooBig = r.error.issues.filter(i => i.code === 'too_big');
      expect(tooBig.length).toBeGreaterThan(0);
      const paths = tooBig.map(i => i.path.join('.'));
      expect(paths).toContain('content.patches.ranges.0.additions.0');
      expect(paths).toContain('content.patches.ranges.0.deletions.0');
    }

    // A value exactly at the cap is accepted.
    const ok = SearchPullRequestsLocalSchema.safeParse({
      goal: 'test goal',
      reasoning: 'exercise patch line bounds',
      owner: 'o',
      repo: 'r',
      prNumber: 1,
      content: {
        patches: {
          mode: 'selected',
          ranges: [
            {
              file: 'a.ts',
              additions: [1_000_000_000],
              deletions: [1_000_000_000],
            },
          ],
        },
      },
    });
    expect(ok.success).toBe(true);
  });
});
