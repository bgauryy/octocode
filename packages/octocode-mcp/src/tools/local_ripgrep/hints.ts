import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    const langType =
      typeof c.langType === 'string'
        ? c.langType
        : typeof c.type === 'string'
          ? c.type
          : undefined;
    const include = Array.isArray(c.include) ? (c.include as unknown[]) : [];
    const excludeDir = Array.isArray(c.excludeDir)
      ? (c.excludeDir as unknown[])
      : [];
    const pattern = typeof c.keywords === 'string' ? c.keywords : undefined;
    const mode = typeof c.mode === 'string' ? c.mode : undefined;

    if (
      !pattern &&
      !path &&
      !langType &&
      include.length === 0 &&
      excludeDir.length === 0
    ) {
      return [];
    }

    const filters: string[] = [];
    if (langType) filters.push(`langType="${langType}"`);
    if (include.length > 0) filters.push(`include=${JSON.stringify(include)}`);
    if (excludeDir.length > 0)
      filters.push(`excludeDir=${JSON.stringify(excludeDir)}`);

    const out: string[] = [];
    if (filters.length > 0) {
      out.push(
        `No matches in ${path ?? 'this scope'} with ${filters.join(' + ')}.`
      );
      out.push(
        'Remove filters one at a time (langType → include → excludeDir) to widen the search.'
      );
    } else {
      out.push(`No matches for "${pattern}" in ${path ?? 'this scope'}.`);
      if (
        pattern &&
        (pattern.includes('\\n') ||
          /\[[\^]?[^\]]*\]/.test(pattern) ||
          pattern.includes('[\\s\\S]') ||
          pattern.includes('(.|\n)'))
      ) {
        out.push(
          'Multiline pattern detected — ripgrep runs in single-line mode by default, so patterns ' +
            'spanning newlines (e.g. [^}], [\\s\\S], \\n) will never match. ' +
            'Split into two separate single-line queries and post-filter results instead.'
        );
      }
      out.push(
        'Broaden: (1) use fixedString=true for a literal match; (2) drop regex meta-chars; ' +
          '(3) try a shorter/partial term; (4) run separate queries scoped to different subdirectories.'
      );
      out.push(
        "Verify files exist: use `localFindFiles` with a name filter or `localViewStructure` to confirm the path isn't empty before retrying."
      );
      if (mode !== 'discovery') {
        out.push(
          'Tip: use mode="discovery" for a fast cheap presence-check (returns only file paths, no content or match counts) before doing a full read.'
        );
      }
    }
    return out;
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const count = ctx.matchCount ? ` (${ctx.matchCount} matches)` : '';
      return [
        `Too many results${count} — narrow the pattern, add a langType/path filter, or use fixedString=true.`,
      ];
    }
    if (ctx.errorType === 'not_found') {
      return [
        'Search path not found — verify it with `localViewStructure` at the parent directory.',
      ];
    }
    return [];
  },
};
