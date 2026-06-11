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
    const matchScope = Array.isArray(c.matchScope)
      ? (c.matchScope as string[])
      : undefined;
    const alreadyTitleScope = matchScope?.includes('title') ?? false;
    const scope = owner && repo ? `${owner}/${repo}` : undefined;

    if (prNumber !== undefined && scope) {
      return [
        `PR #${prNumber} not found in ${scope}.`,
        'Verify the PR number is correct, or search by title keyword using `query` with `sort="best-match"`.',
      ];
    }

    const filters: string[] = [];
    if (state) filters.push(`state=${state}`);
    if (author) filters.push(`author=${author}`);
    if (query) filters.push(`query="${query}"`);

    if (filters.length === 0) {
      if (!scope && query) {
        return [
          'Searching PRs across all of GitHub requires at least one qualifier — add owner/repo, state, or author alongside the query keyword.',
        ];
      }
      return [];
    }
    return [
      `0 PRs found in ${scope ?? 'this scope'} matching ${filters.join(' + ')} — result is confirmed empty.`,
      state === 'merged'
        ? 'Zero merged PRs matched. If unexpected, verify the label spelling or date range; try omitting `state` to check all PR states.'
        : author
          ? 'If unexpected, try removing one filter at a time: drop `author` first, then loosen `query` keywords.'
          : query
            ? 'If unexpected, loosen `query` keywords or remove filters one at a time.'
            : 'If unexpected, try removing or loosening filters one at a time.',
      ...(query && !alreadyTitleScope
        ? [
            'For approximate title matching, use `matchScope=["title"]` with `sort="best-match"` to surface the closest PR.',
          ]
        : !query
          ? [
              'Add a `query` with keywords from the PR title or body to narrow the search.',
            ]
          : []),
      'Tip: the `query` field supports GitHub search qualifiers — e.g. `label:bug`, `created:>2024-01-01`, `merged:>2024-06-01`, `involves:<user>` — to filter by date, label, or involvement.',
      ...(scope
        ? [
            `Zero PRs in ${scope} — the repo may have been renamed or transferred. Confirm the canonical name with githubSearchRepositories before concluding the PR does not exist.`,
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
