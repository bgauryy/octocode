import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    const type = typeof c.type === 'string' ? c.type : undefined;
    const include = Array.isArray(c.include) ? (c.include as unknown[]) : [];
    const excludeDir = Array.isArray(c.excludeDir)
      ? (c.excludeDir as unknown[])
      : [];
    const pattern = typeof c.pattern === 'string' ? c.pattern : undefined;

    if (
      !pattern &&
      !path &&
      !type &&
      include.length === 0 &&
      excludeDir.length === 0
    ) {
      return [];
    }

    const filters: string[] = [];
    if (type) filters.push(`type="${type}"`);
    if (include.length > 0) filters.push(`include=${JSON.stringify(include)}`);
    if (excludeDir.length > 0)
      filters.push(`excludeDir=${JSON.stringify(excludeDir)}`);

    const out: string[] = [];
    if (filters.length > 0) {
      out.push(
        `No matches in ${path ?? 'this scope'} with ${filters.join(' + ')}.`
      );
      out.push(
        'Remove filters one at a time (type → include → excludeDir) to widen the search.'
      );
    } else {
      out.push(`No matches for "${pattern}" in ${path ?? 'this scope'}.`);
      out.push(
        'Broaden: (1) use fixedString=true for a literal match; (2) drop regex meta-chars; ' +
          '(3) try a shorter/partial term; (4) run separate queries scoped to different subdirectories.'
      );
      out.push(
        "Verify files exist: use `localFindFiles` with a name filter or `localViewStructure` to confirm the path isn't empty before retrying."
      );
    }
    return out;
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const count = ctx.matchCount ? ` (${ctx.matchCount} matches)` : '';
      return [
        `Too many results${count} — narrow the pattern, add a type/path filter, or use fixedString=true.`,
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
