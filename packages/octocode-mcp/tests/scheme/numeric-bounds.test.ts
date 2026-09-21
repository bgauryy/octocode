import { z } from 'zod';
import { describe, expect, it } from 'vitest';
import { FileContentQueryBaseLocalSchema } from '@octocodeai/octocode-core/schema';
import { GitHubCodeSearchQueryLocalSchema } from '@octocodeai/octocode-core/schema';
import { GitHubReposSearchSingleQueryLocalSchema } from '@octocodeai/octocode-core/schema';
import { GitHubPullRequestSearchQueryLocalSchema } from '@octocodeai/octocode-core/schema';
import { GitHubViewRepoStructureQueryLocalSchema } from '@octocodeai/octocode-core/schema';
import { ArtifactSearchQueryLocalSchema } from '@octocodeai/octocode-core/schema';
import { LocalFetchContentQuerySchema } from '@octocodeai/octocode-core/schema';
import { AstFilesQuerySchema } from '@octocodeai/octocode-core/schema';
import { LocalRipgrepQuerySchema } from '@octocodeai/octocode-core/schema';
import { AstFilesystemTreeQuerySchema } from '@octocodeai/octocode-core/schema';
import { LspSearchQuerySchema } from '@octocodeai/octocode-core/schema';

const SENTINEL = 9007199254740991;

const schemas: Record<string, z.ZodTypeAny> = {
  'fileContent(remote)': FileContentQueryBaseLocalSchema,
  'code(remote)': GitHubCodeSearchQueryLocalSchema,
  'repos(remote)': GitHubReposSearchSingleQueryLocalSchema,
  'pullRequests(remote)': GitHubPullRequestSearchQueryLocalSchema,
  'viewRepoStructure(remote)': GitHubViewRepoStructureQueryLocalSchema,
  'artifactSearch(remote)': ArtifactSearchQueryLocalSchema,
  'fetchContent(local)': LocalFetchContentQuerySchema,
  astFiles: AstFilesQuerySchema,
  ripgrep: LocalRipgrepQuerySchema,
  astFilesystemTree: AstFilesystemTreeQuerySchema,
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
    const query = { path: '/fixture.txt', chunkType: 'bytes' as const };
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
    const r = FileContentQueryBaseLocalSchema.safeParse({
      owner: 'o',
      repo: 'r',
      path: 'a.ts',
      matchString: 'foo',
      contextLines: 120,
    });
    expect(r.success).toBe(false);
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
    const r = GitHubPullRequestSearchQueryLocalSchema.safeParse({
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
    const ok = GitHubPullRequestSearchQueryLocalSchema.safeParse({
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
