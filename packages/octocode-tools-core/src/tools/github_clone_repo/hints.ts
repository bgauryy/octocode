import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const sparsePath =
      typeof c.sparsePath === 'string'
        ? c.sparsePath
        : typeof c.path === 'string'
          ? c.path
          : undefined;
    if (sparsePath) {
      return [
        'Omit `sparsePath` to check out the full repo.',
        'Confirm path with `ghViewRepoStructure` before cloning sparse.',
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `GitHub API rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
        'Use `ghViewRepoStructure` to inspect the tree without cloning.',
      ];
    }
    if (ctx.errorType === 'permission') {
      return [
        'Token lacks read access — verify GITHUB_TOKEN has `repo` scope.',
      ];
    }
    if (ctx.errorType === 'not_found') {
      return ['Check spelling or omit `branch` to use the default branch.'];
    }
    if (ctx.errorType === 'timeout') {
      return ['Clone timed out — use `sparsePath` for a subdirectory only.'];
    }
    return [];
  },
};
