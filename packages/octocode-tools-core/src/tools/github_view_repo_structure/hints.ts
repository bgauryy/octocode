import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    if (c.wasFilteredToEmpty === true) {
      return [
        'All entries were filtered by the ignored-paths list (node_modules, .git, dist, etc.). Path is valid — navigate to a specific subdirectory.',
      ];
    }
    const path = typeof c.path === 'string' && c.path ? c.path : undefined;
    if (!path) return [];
    return ['Try the parent path, or omit `path` to list from root.'];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.isRateLimited) {
      return [
        `GitHub API rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
      ];
    }
    if (ctx.status === 401) return ['GITHUB_TOKEN is missing or expired.'];
    if (ctx.status === 403) return ['Token lacks `repo` scope.'];
    if (ctx.status === 404) {
      const c = ctx as Record<string, unknown>;
      const owner = typeof c.owner === 'string' ? c.owner : undefined;
      const repo = typeof c.repo === 'string' ? c.repo : undefined;
      const scope = owner && repo ? `'${owner}/${repo}'` : 'the repository';
      return [`${scope} not found — check spelling or use \`ghSearchRepos\`.`];
    }
    return [];
  },
};
