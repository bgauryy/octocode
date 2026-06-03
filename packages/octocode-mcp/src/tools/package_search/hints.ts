/**
 * Response-state hints for packageSearch.
 *
 * Emits actionable recovery moves the agent can execute immediately.
 * Package-specific inline hints (deprecation, exact-match variants) are
 * injected in execution.ts via extraHints.
 *
 * @module tools/package_search/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const name =
      typeof c.query === 'string'
        ? c.query
        : Array.isArray(c.keywords) && typeof c.keywords[0] === 'string'
          ? c.keywords[0]
          : undefined;

    const out: string[] = [];
    if (name) {
      out.push(`Package '${name}' not found on npm.`);
    } else {
      out.push('No packages found on npm for this query.');
    }
    out.push(
      'Check spelling and remove any version suffix (e.g. search "express" not "express@4.18").'
    );
    out.push(
      'If you are looking for a GitHub project rather than a registry package, use `githubSearchRepositories` with the name as a keyword.'
    );
    return out;
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `npm registry rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
      ];
    }
    // Network / registry unreachable
    return [
      'npm registry is unreachable.',
      'Use `githubSearchRepositories` to find the source repo directly by package name or domain terms.',
      'Retry once connectivity is restored.',
    ];
  },
};
