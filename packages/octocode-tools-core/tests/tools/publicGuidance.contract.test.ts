import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { buildPathSuggestionHints } from '../../src/github/fileContentRaw/pathSuggestions.js';
import { PUBLIC_TOOL_DESCRIPTIONS } from '../../src/toolContract/descriptions.js';

const RETIRED_PUBLIC_NAMES =
  /github.code|github.repositories|github.tree|local.text|local.files|local.tree|ghSearchPullRequests|ghSearchIssues|ghSearchCommits/;

describe('public guidance uses only registered tool names', () => {
  it('describes strict search and exact-item history operations', () => {
    expect(PUBLIC_TOOL_DESCRIPTIONS.ghSearchHistory).toContain(
      'operation:"pullRequests"'
    );
    expect(PUBLIC_TOOL_DESCRIPTIONS.ghGetHistoryItem).toContain(
      'operation:"pullRequest"'
    );
    expect(JSON.stringify(PUBLIC_TOOL_DESCRIPTIONS)).not.toMatch(
      RETIRED_PUBLIC_NAMES
    );
  });

  it('returns a runnable unified tree hint for case mismatches', () => {
    const hints = buildPathSuggestionHints('src/file.ts', ['src/File.ts']);
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
