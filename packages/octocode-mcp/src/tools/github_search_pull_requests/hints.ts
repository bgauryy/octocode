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
  hasResults: (_ctx: HintContext = {}) => [
    // Strategy reminder when results exist but may not be the right PR
    'For PR archaeology: if this is not the PR you need, try sort="best-match" ' +
      'and narrow with match=["title"] to get title-only results, then widen to ' +
      'match=["title","body"] if needed.',
  ],

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

    // PR archaeology hint — always emitted first when a query was used
    if (query && query.length > 0) {
      out.push(
        `No PRs match query="${query}" in ${scope}. ` +
          'PR archaeology strategy: (1) try match=["title"] with sort="best-match" — ' +
          'title-only search returns the highest-signal results for finding a PR by keyword. ' +
          '(2) Try shorter / alternate keywords (e.g. if searching "use hook" try "experimental_use"). ' +
          '(3) Widen to match=["title","body"] or drop match entirely. ' +
          '(4) Drop state/date filters one at a time.'
      );
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

    if (!query && out.length === 0) {
      out.push(
        `No PRs found in ${scope}. Relax filters one at a time (state, author, label, query). ` +
          'For PR archaeology, use query + match=["title"] + sort="best-match" as your starting point.'
      );
    }
    return out;
  },

  error: (_ctx: HintContext = {}) => [],
};
