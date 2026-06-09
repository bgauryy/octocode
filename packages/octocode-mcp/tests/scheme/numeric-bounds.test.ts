import { z } from 'zod';
import { describe, expect, it } from 'vitest';
import { FileContentQueryBaseLocalSchema } from '../../src/tools/github_fetch_content/scheme.js';
import { GitHubCodeSearchQueryLocalSchema } from '../../src/tools/github_search_code/scheme.js';
import { GitHubReposSearchSingleQueryLocalSchema } from '../../src/tools/github_search_repos/scheme.js';
import { GitHubPullRequestSearchQueryLocalSchema } from '../../src/tools/github_search_pull_requests/scheme.js';
import { GitHubViewRepoStructureQueryLocalSchema } from '../../src/tools/github_view_repo_structure/scheme.js';
import { PackageSearchQueryLocalSchema } from '../../src/tools/package_search/scheme.js';
import { LocalFetchContentQuerySchema } from '../../src/tools/local_fetch_content/scheme.js';
import { LocalFindFilesQuerySchema } from '../../src/tools/local_find_files/scheme.js';
import { LocalRipgrepQuerySchema } from '../../src/tools/local_ripgrep/scheme.js';
import { LocalViewStructureQuerySchema } from '../../src/tools/local_view_structure/scheme.js';
import { LspGetSemanticContentQuerySchema } from '../../src/tools/lsp/semantic_content/scheme.js';
import { LspGetDiagnosticsQuerySchema } from '../../src/tools/lsp/diagnostics/scheme.js';

const SENTINEL = 9007199254740991;

const schemas: Record<string, z.ZodTypeAny> = {
  'fileContent(remote)': FileContentQueryBaseLocalSchema,
  'code(remote)': GitHubCodeSearchQueryLocalSchema,
  'repos(remote)': GitHubReposSearchSingleQueryLocalSchema,
  'pullRequests(remote)': GitHubPullRequestSearchQueryLocalSchema,
  'viewRepoStructure(remote)': GitHubViewRepoStructureQueryLocalSchema,
  'packageSearch(remote)': PackageSearchQueryLocalSchema,
  'fetchContent(local)': LocalFetchContentQuerySchema,
  findFiles: LocalFindFilesQuerySchema,
  ripgrep: LocalRipgrepQuerySchema,
  viewStructure: LocalViewStructureQuerySchema,
  lspSemantic: LspGetSemanticContentQuerySchema,
  lspDiagnostics: LspGetDiagnosticsQuerySchema,
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

  it('githubSearchCode rejects page 0 instead of silently clamping to page 1', () => {
    const r = GitHubCodeSearchQueryLocalSchema.safeParse({
      keywordsToSearch: ['x'],
      page: 0,
    });
    expect(r.success).toBe(false);
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
    const r = LspGetSemanticContentQuerySchema.safeParse({
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

  it('pullRequests: content.patches.ranges line arrays are bounded and clamp safely', () => {
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

    expect(r.success).toBe(true);
    if (r.success) {
      const range = (
        r.data as never as {
          content: {
            patches: {
              ranges: Array<{ additions: number[]; deletions: number[] }>;
            };
          };
        }
      ).content.patches.ranges[0]!;
      expect(range.additions[0]).toBe(1_000_000_000);
      expect(range.deletions[0]).toBe(1_000_000_000);
    }
  });
});
