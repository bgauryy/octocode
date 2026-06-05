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
    // Support both field names — ctx uses the actual query param name which
    // may be `keywords` or `keywordsToSearch` depending on the schema version.
    const keywords =
      Array.isArray(c.keywords)
        ? c.keywords
        : Array.isArray(c.keywordsToSearch)
          ? c.keywordsToSearch
          : undefined;
    const language = typeof c.language === 'string' ? c.language : undefined;
    const owner = typeof c.owner === 'string' ? c.owner : undefined;
    const topic = typeof c.topic === 'string' ? c.topic : undefined;
    const hasFilters = language || owner || topic;
    const searchTerm = query ?? (keywords && keywords.length > 0 ? String(keywords[0]) : undefined);

    if (!searchTerm && !hasFilters) return [];

    const out: string[] = [];
    if (searchTerm) {
      out.push(`No repositories found for "${searchTerm}".`);
    } else {
      out.push('No repositories found matching the current filters.');
    }

    if (hasFilters) {
      out.push(
        'Remove filters one at a time (language → owner → topic) to widen; add stars filter to surface niche repos with fewer keywords.'
      );
    } else {
      out.push(
        'Try: (1) fewer/simpler keywords; (2) match="name" for an exact-name lookup; (3) separate queries — one keyword each.'
      );
    }

    if (
      searchTerm &&
      /^(@[\w-]+\/)?[\w.-]+$/.test(searchTerm)
    ) {
      out.push(
        `"${searchTerm}" looks like a package — use \`packageSearch\` to resolve it directly to the source repo.`
      );
    }

    return out;
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
      return [
        'Token lacks `public_repo` scope — update token permissions and retry.',
      ];
    }
    return [];
  },
};
