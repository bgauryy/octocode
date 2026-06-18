import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const mode = typeof c.type === 'string' ? c.type : 'prs';

    // --- commits mode ---
    if (mode === 'commits') {
      const path = typeof c.path === 'string' ? c.path : undefined;
      if (path) {
        if (path.endsWith('/')) {
          return [
            `No commits found under "${path}". Check the directory prefix or widen since/until.`,
          ];
        }
        return [
          `No commits found for "${path}". Check path spelling or widen since/until.`,
          'File may have been renamed — re-query with its previous name.',
        ];
      }
      return [
        'No commits found. Try widening since/until or removing the author filter.',
      ];
    }

    // --- PRs mode ---
    const state = typeof c.state === 'string' ? c.state : undefined;
    const owner = typeof c.owner === 'string' ? c.owner : undefined;
    const repo = typeof c.repo === 'string' ? c.repo : undefined;
    const author = typeof c.author === 'string' ? c.author : undefined;
    const query = typeof c.query === 'string' ? c.query : undefined;
    const prNumber = typeof c.prNumber === 'number' ? c.prNumber : undefined;
    const prMatch = Array.isArray(c.prMatch)
      ? (c.prMatch as string[])
      : undefined;
    const alreadyTitleScope = prMatch?.includes('title') ?? false;
    const scope = owner && repo ? `${owner}/${repo}` : undefined;

    if (prNumber !== undefined && scope) {
      return [
        `PR #${prNumber} not found in ${scope}. Verify the PR number, or search by title using keywordsToSearch with match:["title"].`,
      ];
    }

    const filters: string[] = [];
    if (state) filters.push(`state=${state}`);
    if (author) filters.push(`author=${author}`);
    if (query) filters.push(`query="${query}"`);

    if (filters.length === 0) {
      return [];
    }

    return [
      state === 'merged'
        ? 'No merged PRs matched — widen the date range or remove author/label filters.'
        : 'Remove filters one at a time to find what is too narrow.',
      ...(query && !alreadyTitleScope
        ? [
            'For title-only matching use match:["title"] with sort:"best-match".',
          ]
        : !query
          ? [
              'Add a keyword (keywordsToSearch) to narrow by title or body text.',
            ]
          : []),
    ];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `GitHub API rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
      ];
    }
    if (ctx.status === 401) return ['GITHUB_TOKEN is missing or expired.'];
    if (ctx.status === 403)
      return ['Token lacks repo scope — check GITHUB_TOKEN permissions.'];
    return [];
  },
};
