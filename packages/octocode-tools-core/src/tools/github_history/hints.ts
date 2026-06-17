import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const path = typeof ctx.path === 'string' ? ctx.path : undefined;
    if (path) {
      // Directory prefix ends with '/' — repo scope with directory filter
      if (path.endsWith('/')) {
        return [
          `No commits found under "${path}". Check the directory prefix or widen the date range.`,
        ];
      }
      // Specific file
      return [
        `No commits found for "${path}". Check path spelling, branch name, or widen the date range.`,
        'If the file was renamed, re-query with its previous name.',
      ];
    }
    return ['No commits found. Try widening since/until or removing the author filter.'];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `GitHub API rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
      ];
    }
    if (ctx.status === 401) return ['GITHUB_TOKEN is missing or expired.'];
    if (ctx.status === 403)
      return ['Token lacks `repo` scope — required for private repositories.'];
    if (ctx.status === 404)
      return [
        'Repository or path not found. Verify owner, repo, and path spelling.',
        'For private repos ensure the token has `repo` scope.',
      ];
    return [];
  },
};
