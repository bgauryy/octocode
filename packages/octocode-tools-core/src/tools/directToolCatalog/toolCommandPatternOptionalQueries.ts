type PatternQuery = { label: string; query: Record<string, unknown> };

/** Patterns for split-mode and opt-in GitHub tools. */
export function buildOptionalDirectToolCommandPatternQueries(
  toolName: string
): PatternQuery[] {
  if (toolName === 'ghSearchIssues') {
    return [
      {
        label: 'issue search (list)',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          keywordsToSearch: ['schema'],
          state: 'open',
          limit: 5,
        },
      },
      {
        label: 'issue detail (with comments)',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          issueNumber: 1,
          content: { body: true, comments: { discussion: true } },
        },
      },
    ];
  }

  if (toolName === 'ghListReleases') {
    return [
      {
        label: 'release list',
        query: { owner: 'bgauryy', repo: 'octocode', limit: 5 },
      },
      {
        label: 'release list with assets',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          includeAssets: true,
          limit: 5,
        },
      },
    ];
  }

  if (toolName === 'ghSearchDiscussions') {
    return [
      {
        label: 'discussion search (first page)',
        query: {
          owner: 'vitejs',
          repo: 'vite',
          keywordsToSearch: ['plugin'],
          itemsPerPage: 10,
        },
      },
    ];
  }

  return [];
}
