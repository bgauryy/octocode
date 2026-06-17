import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const keywords = Array.isArray(c.keywords) ? c.keywords : undefined;
    const owner = typeof c.owner === 'string' ? c.owner : undefined;
    const repo = typeof c.repo === 'string' ? c.repo : undefined;
    const hasFilters =
      typeof c.extension === 'string' ||
      typeof c.filename === 'string' ||
      typeof c.path === 'string';

    if (c.nonExistentScope === true) {
      const scope = owner && repo ? `${owner}/${repo}` : owner || 'target';
      return [`"${scope}" doesn't exist or isn't accessible — check spelling.`];
    }

    if (ctx.hasOwnerRepo && owner && repo) {
      return [
        hasFilters
          ? 'Remove path/filename/extension first, then retry keywords.'
          : `No results in ${owner}/${repo} — large or popular repos often require narrowing: add extension, filename, or path to reduce scope. Repo may also be unindexed (new/private/recently renamed). Fall back to ghGetFileContent with a known path, or ghViewRepoStructure to discover paths.`,
        'GitHub code search indexes the default branch only.',
      ];
    }

    const out: string[] = [];

    if (
      keywords &&
      keywords.length === 1 &&
      typeof keywords[0] === 'string' &&
      /^@[\w-]+\/[\w.-]+$|^[a-z][\w]*[-.][\w.-]+$/.test(keywords[0])
    ) {
      out.push(
        `"${keywords[0]}" looks like a package name — try \`npmSearch\`.`
      );
    }

    if (out.length === 0 && keywords && keywords.length > 0) {
      out.push(
        'Scope to owner/repo, split into one-keyword queries, or try a shorter exact term. For large repos (react, webpack, electron), narrow with extension="ts" or path="src".'
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
    if (ctx.status === 401) out.push('GITHUB_TOKEN missing/expired.');
    if (ctx.status === 403 && !ctx.isRateLimited)
      out.push('Token lacks `repo` scope.');
    return out;
  },
};
