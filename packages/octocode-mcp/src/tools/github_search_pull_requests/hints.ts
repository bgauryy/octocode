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
  hasResults: (_ctx: HintContext = {}) => [],

  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const out: string[] = [];
    const state = typeof c.state === 'string' ? c.state : undefined;
    const owner = typeof c.owner === 'string' ? c.owner : undefined;
    const repo = typeof c.repo === 'string' ? c.repo : undefined;
    const author = typeof c.author === 'string' ? c.author : undefined;
    const query = typeof c.query === 'string' ? c.query : undefined;
    const prNumber = typeof c.prNumber === 'number' ? c.prNumber : undefined;
    const scope = owner && repo ? `${owner}/${repo}` : 'this scope';

    if (prNumber !== undefined && owner && repo) {
      out.push(
        `PR #${prNumber} not found in ${owner}/${repo}. Verify the number or drop owner/repo to search elsewhere.`
      );
      return out;
    }

    if (state === 'merged') {
      out.push(
        `No merged PRs in ${scope}. Try state="closed" (includes merged+rejected) or drop the state filter.`
      );
    } else if (state === 'open') {
      out.push(
        `No open PRs in ${scope}. Try state="closed" or state="merged".`
      );
    } else if (state === 'closed') {
      out.push(`No closed PRs in ${scope}. Try state="open".`);
    }

    if (author) {
      out.push(
        `No PRs by author=${author}. Verify the username or drop the author filter.`
      );
    }

    if (query && query.length > 0 && out.length === 0) {
      out.push(
        `No PRs match query="${query}" in ${scope}. Relax the query or drop other filters.`
      );
    }

    if (out.length === 0) {
      out.push(
        `No PRs found in ${scope}. Relax filters one at a time (state, author, label, query).`
      );
    }
    return out;
  },

  error: (_ctx: HintContext = {}) => [],
};
