type PatternQuery = { label: string; query: Record<string, unknown> };

/** Patterns for split-mode and opt-in GitHub tools. */
export function buildOptionalDirectToolCommandPatternQueries(
  toolName: string
): PatternQuery[] {
  if (toolName === 'ghListReleases') {
    return [
      {
        label: 'release list',
        query: { owner: 'bgauryy', repo: 'octocode', pageSize: 5 },
      },
      {
        label: 'release list with assets',
        query: {
          owner: 'bgauryy',
          repo: 'octocode',
          includeAssets: true,
          pageSize: 5,
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
          keywords: ['plugin'],
          pageSize: 10,
        },
      },
    ];
  }

  return [];
}
