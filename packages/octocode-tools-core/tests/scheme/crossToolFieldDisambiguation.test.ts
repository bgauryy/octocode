import { describe, expect, it } from 'vitest';

import { GitHubCodeSearchQueryLocalSchema } from '../../src/tools/github_search_code/scheme.js';
import { GitHubReposSearchSingleQueryLocalSchema } from '../../src/tools/github_search_repos/scheme.js';
import { GitHubPullRequestSearchQueryLocalSchema } from '../../src/tools/github_search_pull_requests/scheme.js';
import { LocalRipgrepQuerySchema } from '../../src/tools/local_ripgrep/scheme.js';

function fieldDescription(schema: unknown, field: string): string | undefined {
  const shape = (schema as { shape?: Record<string, { description?: string }> })
    .shape;
  return shape?.[field]?.description;
}

describe('cross-tool field disambiguation (mode/match/keywords/filesOnly)', () => {
  it('ghSearch code match distinguishes the repositories operation', () => {
    const desc = fieldDescription(GitHubCodeSearchQueryLocalSchema, 'match');
    expect(desc).toContain('ghSearch operation:"code"');
    expect(desc).toContain('Operation:"repositories"');
  });

  it('ghSearch repositories match distinguishes the code operation', () => {
    const desc = fieldDescription(
      GitHubReposSearchSingleQueryLocalSchema,
      'match'
    );
    expect(desc).toContain('ghSearch operation:"repositories"');
    expect(desc).toContain('Operation:"code"');
  });

  it('ghSearchPullRequests match distinguishes ghSearch code; issueNumber is described', () => {
    const matchDesc = fieldDescription(
      GitHubPullRequestSearchQueryLocalSchema,
      'match'
    );
    expect(matchDesc).toContain('ghSearch operation:"code"');

    const issueNumberDesc = fieldDescription(
      GitHubPullRequestSearchQueryLocalSchema,
      'issueNumber'
    );
    expect(issueNumberDesc).toBeTruthy();
  });

  it('keeps the local lexical-pattern description self-contained', () => {
    const modeDesc = fieldDescription(LocalRipgrepQuerySchema, 'mode');
    expect(modeDesc).toContain('ghSearchPullRequests');
    expect(modeDesc).not.toContain('localBinaryInspect');

    const searchTextDesc = fieldDescription(
      LocalRipgrepQuerySchema,
      'searchText'
    );
    expect(searchTextDesc).toContain('Single lexical pattern');
    expect(searchTextDesc).not.toContain('ghSearch operation:"code"');
  });
});
