/**
 * Response-state hints for githubSearchRepositories.
 *
 * Emits actionable recovery moves the agent can execute immediately.
 *
 * @module tools/github_search_repos/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const query = typeof c.query === 'string' ? c.query : undefined;
    const hasFilters =
      typeof c.language === 'string' ||
      typeof c.owner === 'string' ||
      typeof c.topic === 'string';

    const out: string[] = [];
    if (query) {
      out.push(`No repositories found for "${query}".`);
    } else {
      out.push('No repositories found matching the current filters.');
    }

    if (hasFilters) {
      out.push('Remove `language`, `owner`, or topic filters to broaden the search — try one qualifier at a time.');
    } else {
      out.push('Try simpler or fewer search terms.');
    }
    out.push('If you have an exact npm/PyPI package name, use `packageSearch` instead — it resolves directly to the source repo.');
    return out;
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `GitHub API rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
      ];
    }
    if (ctx.status === 401) {
      return ['GITHUB_TOKEN is missing or expired — set a valid token and retry.'];
    }
    if (ctx.status === 403) {
      return ["Token lacks `public_repo` scope — update token permissions and retry."];
    }
    return [];
  },
};
