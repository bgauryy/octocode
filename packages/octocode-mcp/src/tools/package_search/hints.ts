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

    if (!name) return [];
    return [
      `Package '${name}' not found on npm.`,
      'Check spelling and remove any version suffix (e.g. search "express" not "express@4.18").',
      'If you are looking for a GitHub project rather than a registry package, use `githubSearchRepositories` with the name as a keyword.',
    ];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `npm registry rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
      ];
    }
    if (ctx.originalError) {
      return [
        'npm registry is unreachable.',
        'Use `githubSearchRepositories` to find the source repo directly by package name or domain terms.',
      ];
    }
    return [];
  },
};
