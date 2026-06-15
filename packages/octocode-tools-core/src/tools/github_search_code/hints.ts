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
        `No matches in ${owner}/${repo}.`,
        hasFilters
          ? 'Remove a filter or try different keywords.'
          : `Try different keywords, or broaden to all of ${owner}.`,
        'Verify with `githubGetFileContent` — the index covers only the default branch.',
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
        `"${keywords[0]}" looks like a package name — try \`packageSearch\`.`
      );
    }

    if (out.length === 0 && keywords && keywords.length > 0) {
      out.push(
        'No matches across GitHub — scope to owner/repo, try different keywords, or split into one-term queries.'
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
