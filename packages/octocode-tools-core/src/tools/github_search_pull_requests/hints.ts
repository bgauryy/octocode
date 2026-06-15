import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
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
        'Verify the PR number, or search by title using `query` with sort="best-match".',
      ];
    }

    const filters: string[] = [];
    if (state) filters.push(`state=${state}`);
    if (author) filters.push(`author=${author}`);
    if (query) filters.push(`query="${query}"`);

    if (filters.length === 0) {
      if (!scope && query) {
        return ['Add owner/repo, state, or author to scope the search.'];
      }
      return [];
    }

    return [
      state === 'merged'
        ? 'is:merged is unreliable — use state=closed instead (mergedAt is set on closed PRs).'
        : 'Remove state/author/label filters first, then retry broader keywords.',
      ...(query && !alreadyTitleScope
        ? [
            'For title-only matching add match:["title"] with sort:"best-match".',
          ]
        : !query
          ? ['Add a `query` keyword to narrow by title or body.']
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
    if (ctx.status === 403) return ['Token lacks `repo` scope.'];
    return [];
  },
};
