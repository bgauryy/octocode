/**
 * Response-state hints for githubSearchPullRequests.
 *
 * Empty-result branch is query-shape aware: it inspects which filters were
 * applied (state, author, label, prNumber, query string) and proposes the
 * single most likely-helpful next move.
 *
 * @module tools/github_search_pull_requests/hints
 */

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
    const scope = owner && repo ? `${owner}/${repo}` : undefined;

    if (prNumber !== undefined && scope) {
      return [`PR #${prNumber} not found in ${scope}.`];
    }

    const filters: string[] = [];
    if (state) filters.push(`state=${state}`);
    if (author) filters.push(`author=${author}`);
    if (query) filters.push(`query="${query}"`);

    if (filters.length === 0) return [];
    return [
      `No PRs in ${scope ?? 'this scope'} matching ${filters.join(' + ')}.`,
    ];
  },

  error: () => [],
};
