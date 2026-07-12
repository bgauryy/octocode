import { describe, expect, it } from 'vitest';

import { GitHubCodeSearchQueryLocalSchema } from '../../src/tools/github_search_code/scheme.js';
import { GitHubReposSearchSingleQueryLocalSchema } from '../../src/tools/github_search_repos/scheme.js';
import { GitHubPullRequestSearchQueryLocalSchema } from '../../src/tools/github_search_pull_requests/scheme.js';
import { LocalRipgrepQuerySchema } from '../../src/tools/local_ripgrep/scheme.js';
import { LocalViewStructureQuerySchema } from '../../src/tools/local_view_structure/scheme.js';

function fieldDescription(schema: unknown, field: string): string | undefined {
  const shape = (schema as { shape?: Record<string, { description?: string }> })
    .shape;
  return shape?.[field]?.description;
}

describe('cross-tool field disambiguation (mode/match/keywords/filesOnly)', () => {
  it('ghSearchCode match cross-references ghSearchRepos/ghHistoryResearch', () => {
    const desc = fieldDescription(GitHubCodeSearchQueryLocalSchema, 'match');
    expect(desc).toContain('ghSearchRepos');
  });

  it('ghSearchRepos match cross-references ghSearchCode', () => {
    const desc = fieldDescription(
      GitHubReposSearchSingleQueryLocalSchema,
      'match'
    );
    expect(desc).toContain('ghSearchCode');
  });

  it('ghHistoryResearch match cross-references ghSearchCode; issueNumber is now described', () => {
    const matchDesc = fieldDescription(
      GitHubPullRequestSearchQueryLocalSchema,
      'match'
    );
    expect(matchDesc).toContain('ghSearchCode');

    const issueNumberDesc = fieldDescription(
      GitHubPullRequestSearchQueryLocalSchema,
      'issueNumber'
    );
    expect(issueNumberDesc).toBeTruthy();
  });

  it('localSearchCode mode/keywords/filesOnly cross-reference other tools\' same-named fields', () => {
    const modeDesc = fieldDescription(LocalRipgrepQuerySchema, 'mode');
    expect(modeDesc).toContain('ghHistoryResearch');
    expect(modeDesc).not.toContain('localBinaryInspect');

    const keywordsDesc = fieldDescription(LocalRipgrepQuerySchema, 'keywords');
    expect(keywordsDesc).toContain('ghSearchCode');

    const filesOnlyDesc = fieldDescription(LocalRipgrepQuerySchema, 'filesOnly');
    expect(filesOnlyDesc).toContain('localViewStructure');
  });

  it('localViewStructure filesOnly cross-references localSearchCode', () => {
    const desc = fieldDescription(LocalViewStructureQuerySchema, 'filesOnly');
    expect(desc).toContain('localSearchCode');
  });
});
