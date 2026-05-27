/**
 * Response-state hints for githubSearchCode.
 *
 * Only emits hints that are conditional on the response itself
 * (pagination, failures, empty-with-context). No static guidance.
 *
 * @module tools/github_search_code/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const out: string[] = [];
    const c = ctx as Record<string, unknown>;
    const hasMore = c.hasMore === true;
    const totalMatches =
      typeof c.totalMatches === 'number' ? c.totalMatches : undefined;
    const currentPage = typeof c.currentPage === 'number' ? c.currentPage : 1;
    const totalPages = typeof c.totalPages === 'number' ? c.totalPages : 1;

    // Concept-match guard: warn when ALL returned paths live in non-canonical
    // dirs (examples/, __tests__/, docs/, fixtures/, samples/, benchmarks/).
    // Benchmark Q4 lost a point by quoting React Router example code as the
    // canonical entry — this hint pushes the agent to re-search inside src/
    // or packages/ before treating the file contents as authoritative.
    const paths = Array.isArray(c.matchedPaths)
      ? (c.matchedPaths as unknown[]).filter(
          (p): p is string => typeof p === 'string'
        )
      : undefined;
    if (paths && paths.length > 0) {
      const nonCanonical =
        /(^|\/)(examples?|__tests?__|tests?|docs?|fixtures?|samples?|benchmarks?|e2e)(\/|$)/i;
      const allNonCanonical = paths.every(p => nonCanonical.test(p));
      if (allNonCanonical) {
        out.push(
          '⚠️ All matches are in examples/__tests__/docs/fixtures — NON-CANONICAL paths. Do not quote as the real implementation. Re-search with path="packages/" or path="src/" before reading contents.'
        );
      }
    }

    if (hasMore) {
      // There are more pages — emit an assertive enumeration warning.
      // This is the primary cause of missed call sites in multi-file usage queries
      // (e.g. Q4: finding ALL startTransition usages across vercel/next.js).
      const totalStr = totalMatches ? ` of ${totalMatches}` : '';
      out.push(
        `⚠️ Page ${currentPage}/${totalPages} only. This result shows the FIRST page${totalStr} matches. ` +
          'For exhaustive usage enumeration (all call sites, all files), fetch every page: ' +
          `page=${currentPage + 1}${totalPages > currentPage + 1 ? `, page=${currentPage + 2}, … page=${totalPages}` : ''}. ` +
          'Critical files are often on later pages — stop paginating only when hasMore=false.'
      );
    }
    return out;
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
    // Negative result is not "not found" — force a pivot. Benchmark Q8 lost a
    // point because an empty Recoil search led to a "not found" conclusion;
    // a scout of packages/recoil/core/ would have surfaced Recoil_ReactMode.js.
    if (ctx.hasOwnerRepo && owner && repo) {
      out.push(
        `PIVOT before concluding absent: githubViewRepoStructure on ${owner}/${repo}${typeof c.path === 'string' ? ` (path="${c.path}")` : ''} to surface sibling / *Mode / *Config / *Flag files that may gate the implementation.`
      );
    } else {
      out.push(
        'PIVOT before concluding absent: try filename variants (kebab/camel/snake), a feature-flag/*Mode term, or githubViewRepoStructure to scout the candidate dir.'
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
      out.push('Check GITHUB_TOKEN is valid and not expired.');
    }

    if (ctx.status === 403 && !ctx.isRateLimited) {
      out.push(
        'Permission denied. Token needs `repo` scope for private repos.'
      );
    }

    return out;
  },
};
