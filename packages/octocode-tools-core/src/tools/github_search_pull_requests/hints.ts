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
        `PR #${prNumber} not found in ${scope}.`,
        'Verify the PR number, or search by title using `query` with `sort="best-match"`.',
      ];
    }

    const filters: string[] = [];
    if (state) filters.push(`state=${state}`);
    if (author) filters.push(`author=${author}`);
    if (query) filters.push(`query="${query}"`);

    if (filters.length === 0) {
      if (!scope && query) {
        return [
          'Cross-GitHub PR search requires at least one qualifier — add owner/repo, state, or author.',
        ];
      }
      return [];
    }

    return [
      `0 PRs in ${scope ?? 'this scope'} matching ${filters.join(' + ')}.`,
      state === 'merged'
        ? "is:merged isn't indexed on all repos — try state=closed (merged PRs have mergedAt set)."
        : 'Remove a filter or try broader keywords.',
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
    if (ctx.status === 401) {
      return [
        'GITHUB_TOKEN is missing or expired — set a valid token and retry.',
      ];
    }
    if (ctx.status === 403) {
      return ['Token lacks `repo` scope — update token permissions and retry.'];
    }
    return [];
  },
};
