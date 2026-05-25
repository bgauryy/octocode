/**
 * Response-state hints for githubSearchCode.
 *
 * Only emits hints that are conditional on the response itself
 * (pagination, failures, empty-with-context). No static guidance.
 *
 * @module tools/github_search_code/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';
import { getActiveProvider } from '../../serverConfig.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const hasMore = c.hasMore === true;
    const totalMatches =
      typeof c.totalMatches === 'number' ? c.totalMatches : undefined;
    const currentPage = typeof c.currentPage === 'number' ? c.currentPage : 1;
    const totalPages = typeof c.totalPages === 'number' ? c.totalPages : 1;

    if (!hasMore) return [];

    // There are more pages — emit an assertive enumeration warning.
    // This is the primary cause of missed call sites in multi-file usage queries
    // (e.g. Q4: finding ALL startTransition usages across vercel/next.js).
    const totalStr = totalMatches ? ` of ${totalMatches}` : '';
    return [
      `⚠️ Page ${currentPage}/${totalPages} only. This result shows the FIRST page${totalStr} matches. ` +
        'For exhaustive usage enumeration (all call sites, all files), fetch every page: ' +
        `page=${currentPage + 1}${totalPages > currentPage + 1 ? `, page=${currentPage + 2}, … page=${totalPages}` : ''}. ` +
        'Critical files are often on later pages — stop paginating only when hasMore=false.',
    ];
  },

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

    if (ctx.match === 'path') {
      out.push('No paths matched. Try match="file" or drop the path filter.');
    } else if (ctx.hasOwnerRepo && owner && repo) {
      const filterList = filters.length > 0 ? filters.join('+') : 'filters';
      out.push(
        `No matches in ${owner}/${repo}. Drop ${filterList} or try different keywords.`
      );
    } else if (!ctx.hasOwnerRepo) {
      out.push(
        'No matches across repos. Narrow with owner+repo or simplify keywords (AND logic — every term must match).'
      );
    }

    if (keywords && keywords.length > 2) {
      out.push(
        `${keywords.length} keywords combined with AND. Try fewer (${keywords.slice(0, 2).join(', ')}) or broader synonyms.`
      );
    }
    if (filters.length >= 2) {
      out.push(
        `Combining ${filters.join(' + ')} is often too restrictive — drop one filter.`
      );
    }
    // Cross-tool routing recovery: if the only keyword looks like a package
    // identifier (scoped, dashed, dotted), point the agent at packageSearch
    // which resolves a known name → repo in one call.
    if (
      !ctx.hasOwnerRepo &&
      keywords &&
      keywords.length === 1 &&
      typeof keywords[0] === 'string' &&
      /^(@[\w-]+\/)?[\w.-]+$/.test(keywords[0])
    ) {
      out.push(
        `Looks like a package name ("${keywords[0]}") — try packageSearch first; it resolves to a repo in one call.`
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
      const provider = getActiveProvider();
      const tokenVarMap: Record<string, string> = {
        gitlab: 'GITLAB_TOKEN',
        bitbucket: 'BITBUCKET_TOKEN',
      };
      const tokenVar = tokenVarMap[provider] ?? 'GITHUB_TOKEN';
      out.push(`Check ${tokenVar} is valid and not expired.`);
    }

    if (ctx.status === 403 && !ctx.isRateLimited) {
      out.push(
        'Permission denied. Token needs `repo` scope for private repos.'
      );
    }

    return out;
  },
};
