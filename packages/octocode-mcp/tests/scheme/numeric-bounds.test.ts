import { z } from 'zod';
import { describe, expect, it } from 'vitest';
import { FileContentQueryBaseLocalSchema } from '../../../octocode-tools-core/src/tools/github_fetch_content/scheme.js';
import { GitHubCodeSearchQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_search_code/scheme.js';
import { GitHubReposSearchSingleQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_search_repos/scheme.js';
import { GitHubPullRequestSearchQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_search_pull_requests/scheme.js';
import { GitHubViewRepoStructureQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_view_repo_structure/scheme.js';
import { NpmSearchQueryLocalSchema } from '../../../octocode-tools-core/src/tools/package_search/scheme.js';
import { LocalFetchContentQuerySchema } from '../../../octocode-tools-core/src/tools/local_fetch_content/scheme.js';
import { LocalFindFilesQuerySchema } from '../../../octocode-tools-core/src/tools/local_find_files/scheme.js';
import { LocalRipgrepQuerySchema } from '../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js';
import { LocalViewStructureQuerySchema } from '../../../octocode-tools-core/src/tools/local_view_structure/scheme.js';
import { LspGetSemanticsQuerySchema } from '../../../octocode-tools-core/src/tools/lsp/semantic_content/scheme.js';

const SENTINEL = 9007199254740991;

const schemas: Record<string, z.ZodTypeAny> = {
  'fileContent(remote)': FileContentQueryBaseLocalSchema,
  'code(remote)': GitHubCodeSearchQueryLocalSchema,
  'repos(remote)': GitHubReposSearchSingleQueryLocalSchema,
  'pullRequests(remote)': GitHubPullRequestSearchQueryLocalSchema,
  'viewRepoStructure(remote)': GitHubViewRepoStructureQueryLocalSchema,
  'npmSearch(remote)': NpmSearchQueryLocalSchema,
  'fetchContent(local)': LocalFetchContentQuerySchema,
  findFiles: LocalFindFilesQuerySchema,
  ripgrep: LocalRipgrepQuerySchema,
  viewStructure: LocalViewStructureQuerySchema,
  lspSemantic: LspGetSemanticsQuerySchema,
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

  it('ghSearchCode clamps page 0 to page 1 (relaxed page field)', () => {
    const r = GitHubCodeSearchQueryLocalSchema.safeParse({
      keywords: ['x'],
      page: 0,
    });
    expect(r.success).toBe(true);
    if (r.success) expect(r.data.page).toBe(1);
  });

  it('clamps contextLines:120 to 100 instead of rejecting (FC-2)', () => {
    const r = FileContentQueryBaseLocalSchema.safeParse({
      owner: 'o',
      repo: 'r',
      path: 'a.ts',
      matchString: 'foo',
      contextLines: 120,
    });
    expect(r.success).toBe(true);
    if (r.success) {
      expect((r.data as { contextLines?: number }).contextLines).toBe(100);
    }
  });

  it('clamps a negative line number instead of rejecting it', () => {
    const r = LspGetSemanticsQuerySchema.safeParse({
      uri: 'a.ts',
      type: 'definition',
      symbolName: 'x',
      lineHint: -5,
    });
    if (r.success) {
      expect(r.data.lineHint).toBe(1);
    } else {
      const paths = r.error.issues.map(i => i.path.join('.'));
      expect(paths).not.toContain('lineHint');
    }
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
