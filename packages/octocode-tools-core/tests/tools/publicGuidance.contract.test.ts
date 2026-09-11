import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { buildPathSuggestionHints } from '../../src/github/fileContentRaw/pathSuggestions.js';
import {
  FileContentQueryLocalSchema,
  GitHubGetHistoryItemQueryLocalSchema,
  GitHubSearchHistoryQueryLocalSchema,
  PUBLIC_TOOL_DESCRIPTIONS,
} from '@octocodeai/octocode-core/schema';

const RETIRED_PUBLIC_NAMES =
  /github\.code|github\.repositories|github\.tree|local\.text|local\.files|local\.tree|ghSearchPullRequests|ghSearchIssues|ghSearchCommits/;

describe('public guidance uses only registered tool names', () => {
  it('describes strict search and exact-item history operations', () => {
    expect(PUBLIC_TOOL_DESCRIPTIONS.ghSearchHistory).toMatch(
      /number or ref is unknown.*filter metadata/i
    );
    expect(PUBLIC_TOOL_DESCRIPTIONS.ghGetHistoryItem).toMatch(/read a known/i);
    expect(
      GitHubSearchHistoryQueryLocalSchema.safeParse({
        operation: 'pullRequests',
        owner: 'owner',
        repo: 'repo',
      }).success
    ).toBe(true);
    expect(
      GitHubGetHistoryItemQueryLocalSchema.safeParse({
        operation: 'pullRequest',
        owner: 'owner',
        repo: 'repo',
        number: 1,
      }).success
    ).toBe(true);
    expect(
      GitHubGetHistoryItemQueryLocalSchema.safeParse({
        operation: 'pullRequests',
        owner: 'owner',
        repo: 'repo',
      }).success
    ).toBe(false);
    expect(JSON.stringify(PUBLIC_TOOL_DESCRIPTIONS)).not.toMatch(
      RETIRED_PUBLIC_NAMES
    );
  });

  it('permits a path-only remote read while using the same exact pagination defaults as local fetch', () => {
    expect(
      FileContentQueryLocalSchema.safeParse({
        owner: 'owner',
        repo: 'repo',
        path: 'src/index.ts',
      }).success
    ).toBe(true);
    expect(PUBLIC_TOOL_DESCRIPTIONS.ghGetFileContent).toMatch(
      /exact by default/i
    );
    expect(PUBLIC_TOOL_DESCRIPTIONS.ghGetFileContent).toMatch(
      /matchString.*line ranges.*minify:symbols/i
    );
    expect(PUBLIC_TOOL_DESCRIPTIONS.ghGetFileContent).toMatch(
      /ghSearch.*ghCloneRepo/i
    );
  });

  it('returns a runnable unified tree hint for case mismatches', () => {
    const hints = buildPathSuggestionHints('src/file.ts', ['src/File.ts']);
    expect(hints).toHaveLength(1);
    expect(hints[0]).toContain('src/File.ts');
    expect(hints[0]).toContain('ghSearch operation:"tree"');
    expect(hints.join('\n')).not.toMatch(RETIRED_PUBLIC_NAMES);
  });

  it.each([
    'src/errors/localToolErrors.ts',
    'src/github/fileContentRaw/pathSuggestions.ts',
    'src/tools/github_clone_repo/cloneRepo.ts',
    'src/tools/github_fetch_content/finalizer.ts',
    'src/tools/lsp/semantic_content/semanticEnvelopes/locationEnvelopes.ts',
    'src/tools/lsp/semantic_content/semanticFileOps/anchor.ts',
    'src/utils/package/npm/npmDeprecation.ts',
    'src/utils/package/npm/npmDetailsFetchers.ts',
  ])(
    '%s contains no retired name in its public guidance',
    async relativePath => {
      const source = await readFile(join(process.cwd(), relativePath), 'utf8');
      expect(source).not.toMatch(RETIRED_PUBLIC_NAMES);
    }
  );
});
