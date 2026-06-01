/**
 * Response-state hints for githubSearchCode.
 * Fires only on empty/error. Pagination + non-canonical-path signals live in
 * the structured response (pagination / matches array); usage guidance lives
 * in the tool description.
 *
 * @module tools/github_search_code/hints
 */

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

    // Nonexistent scope (GitHub 422): empty means the scope doesn't exist, not
    // "no matches". Lead with this so the agent fixes the scope, not the query.
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

      // Recovery for the most common silent-zero causes. NOTE: the builder
      // already auto-splits a file-pointing path (dir/file.ext) into
      // filename: + directory path:, so a path that survives to here is a
      // directory. GitHub matches path: against a file's DIRECTORY only — so
      // the lever is broadening the directory, not dropping the phrase (a
      // single token + a file-pointing path returns zero just the same).
      const hasPhrase =
        Array.isArray(keywords) &&
        keywords.some(k => typeof k === 'string' && /\s/.test(k));
      if (filters.includes('path')) {
        out.push(
          'GitHub path: matches a directory, not a file — broaden path: to a parent directory (use filename: to target one file).'
        );
      } else if (hasPhrase) {
        out.push(
          'A multi-word phrase is matched literally — broaden with fewer/looser keyword terms.'
        );
      }
      // (2) archived repos are under-indexed by GitHub code search, so a
      // zero result is NOT proof of absence. Verify before concluding.
      out.push(
        'For archived repos a zero isn\'t proof — code search is unindexed; confirm via githubGetFileContent before "not found".'
      );
    }

    // Cross-tool pivot: scoped/dotted single keyword → likely a package.
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
