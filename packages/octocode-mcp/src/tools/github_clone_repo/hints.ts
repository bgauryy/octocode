import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const sparsePath = typeof c.path === 'string' ? c.path : undefined;
    if (sparsePath) {
      return [
        `Clone succeeded but 'sparse_path="${sparsePath}"' matched no files.`,
        'Broaden or omit `sparse_path` to check out the full repo, then inspect with `localViewStructure`.',
        'Use `githubViewRepoStructure` first to confirm the exact directory path before cloning sparse.',
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `GitHub API rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
        'While waiting, use `githubViewRepoStructure` to inspect the tree without cloning.',
      ];
    }
    if (ctx.errorType === 'permission') {
      return [
        'Token lacks read access — verify GITHUB_TOKEN has `repo` scope for private repos.',
      ];
    }
    if (ctx.errorType === 'not_found') {
      return [
        'Repo or branch not found — check spelling or omit `branch` to resolve the default branch.',
      ];
    }
    if (ctx.errorType === 'timeout') {
      return [
        'Clone timed out — use `sparse_path` to check out only the relevant subdirectory.',
      ];
    }
    return [];
  },
};
