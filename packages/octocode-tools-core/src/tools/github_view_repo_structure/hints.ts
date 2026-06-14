import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' && c.path ? c.path : undefined;
    const branch =
      typeof c.branch === 'string' && c.branch ? c.branch : undefined;
    if (!path && !branch) return [];
    const where = path ? `'${path}'` : 'root';
    const onBranch = branch ? ` on branch '${branch}'` : '';
    return [
      `Empty listing for ${where}${onBranch}.`,
      path
        ? 'Try the parent directory path, or omit `path` to list from the repo root.'
        : 'The repo may be empty or the default branch has no commits.',
    ];
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
      return ['Token lacks `repo` scope — update token permissions and retry.'];
    }
    if (ctx.status === 404) {
      const c = ctx as Record<string, unknown>;
      const owner = typeof c.owner === 'string' ? c.owner : undefined;
      const repo = typeof c.repo === 'string' ? c.repo : undefined;
      const scope = owner && repo ? `'${owner}/${repo}'` : 'the repository';
      return [
        `${scope} not found or not accessible.`,
        'Check spelling of owner and repo, and verify your token has read access.',
        'Use `githubSearchRepositories` to discover the correct owner/repo if unsure.',
      ];
    }
    return [];
  },
};
