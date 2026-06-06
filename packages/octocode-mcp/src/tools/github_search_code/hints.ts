import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const out: string[] = [];
    const c = ctx as Record<string, unknown>;
    const keywords = Array.isArray(c.keywords) ? c.keywords : undefined;
    const owner = typeof c.owner === 'string' ? c.owner : undefined;
    const repo = typeof c.repo === 'string' ? c.repo : undefined;
    const filters: string[] = [];
    if (typeof c.extension === 'string') filters.push('extension');
    if (typeof c.filename === 'string') filters.push('filename');
    if (typeof c.path === 'string') filters.push('path');

    if (c.nonExistentScope === true) {
      const scope = owner && repo ? `${owner}/${repo}` : owner || 'target';
      out.push(
        `"${scope}" doesn't exist or isn't searchable (not "no matches") — check spelling/access.`
      );
      return out;
    }

    if (ctx.hasOwnerRepo && owner && repo) {
      const filterList = filters.length > 0 ? ` (${filters.join('+')})` : '';
      out.push(`No matches in ${owner}/${repo}${filterList}.`);

      const hasPhrase =
        Array.isArray(keywords) &&
        keywords.some(k => typeof k === 'string' && /\s/.test(k));
      if (filters.includes('extension') || filters.includes('filename')) {
        out.push(
          'extension: and filename: filters stack with AND and silently zero out results — remove them and search with keywords only, then re-add once you have hits.'
        );
      } else if (filters.includes('path')) {
        out.push(
          'GitHub path: matches a directory, not a file — broaden path: to a parent directory (use filename: to target one file).'
        );
      } else if (hasPhrase) {
        out.push(
          'A multi-word phrase is matched literally — broaden with fewer/looser keyword terms.'
        );
      }
      out.push(
        'For archived repos a zero isn\'t proof — code search is unindexed; confirm via githubGetFileContent before "not found".'
      );
    }

    if (
      !ctx.hasOwnerRepo &&
      keywords &&
      keywords.length === 1 &&
      typeof keywords[0] === 'string' &&
      /^(@[\w-]+\/)?[\w.-]+$/.test(keywords[0])
    ) {
      out.push(
        `"${keywords[0]}" looks like a package name — try packageSearch.`
      );
    }

    if (
      !ctx.hasOwnerRepo &&
      out.length === 0 &&
      keywords &&
      keywords.length > 0
    ) {
      out.push(
        'No matches across GitHub — scope to owner/repo, run separate single-term queries, or add extension/path filters.'
      );
      if (filters.includes('path')) {
        out.push(
          'GitHub path: matches a directory prefix, not a full path — broaden or omit path to search the whole repo.'
        );
      }
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
    if (ctx.status === 401) {
      out.push('GITHUB_TOKEN missing/expired.');
    }
    if (ctx.status === 403 && !ctx.isRateLimited) {
      out.push('Token lacks `repo` scope.');
    }
    return out;
  },
};
