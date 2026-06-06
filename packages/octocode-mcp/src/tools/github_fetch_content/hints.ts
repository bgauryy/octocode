import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    if (!path) return [];
    return [
      `File '${path}' exists but returned no content (may be an empty file).`,
      'Verify the path points to a file, not a directory — use `githubViewRepoStructure` to inspect the tree.',
      'To fetch a directory subtree to disk, use `githubCloneRepo` with `sparse_path`.',
    ];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const c = ctx as Record<string, unknown>;
      const size = typeof c.fileSize === 'number' ? `${c.fileSize}KB ` : '';
      return [
        `File ${size}exceeds the 300KB cap — use startLine+endLine or matchString for a focused section.`,
      ];
    }
    if (ctx.errorType === 'not_found') {
      const c = ctx as Record<string, unknown>;
      const where = typeof c.path === 'string' ? `'${c.path}'` : 'path';
      const branch =
        typeof c.branch === 'string' ? ` on branch '${c.branch}'` : '';
      return [
        `${where} not found${branch}.`,
        'Check the path with `githubViewRepoStructure` to confirm the exact file name and location.',
        'If the branch is wrong, omit `branch` to resolve the default branch automatically.',
      ];
    }
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
    return [];
  },
};
