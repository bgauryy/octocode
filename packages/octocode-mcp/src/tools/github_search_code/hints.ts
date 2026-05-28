/**
 * Response-state hints for githubSearchCode.
 * Fires only on empty/error. Pagination + non-canonical-path signals live in
 * the structured response (pagination / matches array); usage guidance lives
 * in the tool description.
 *
 * @module tools/github_search_code/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const out: string[] = [];
    const c = ctx as Record<string, unknown>;
    const keywords = Array.isArray(c.keywords) ? c.keywords : undefined;
    const owner = typeof c.owner === 'string' ? c.owner : undefined;
    const repo = typeof c.repo === 'string' ? c.repo : undefined;
    const filters: string[] = [];
    if (typeof c.extension === 'string') filters.push('extension');
    if (typeof c.filename === 'string') filters.push('filename');
    if (typeof c.path === 'string') filters.push('path');

    if (ctx.hasOwnerRepo && owner && repo) {
      const filterList = filters.length > 0 ? ` (${filters.join('+')})` : '';
      out.push(`No matches in ${owner}/${repo}${filterList}.`);
    }

    // Cross-tool pivot: scoped/dotted single keyword → likely a package.
    if (
      !ctx.hasOwnerRepo &&
      keywords &&
      keywords.length === 1 &&
      typeof keywords[0] === 'string' &&
      /^(@[\w-]+\/)?[\w.-]+$/.test(keywords[0])
    ) {
      out.push(
        `"${keywords[0]}" looks like a package name — try packageSearch.`
      );
    }
    return out;
  },

  error: (ctx: HintContext = {}) => {
    const out: string[] = [];
    if (ctx.isRateLimited) {
      out.push(
        `Rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ''}`
      );
    }
    if (ctx.status === 401) {
      out.push('GITHUB_TOKEN missing/expired.');
    }
    if (ctx.status === 403 && !ctx.isRateLimited) {
      out.push('Token lacks `repo` scope.');
    }
    return out;
  },
};
