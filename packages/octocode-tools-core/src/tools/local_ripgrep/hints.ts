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
    const hasFilters = langType || include.length > 0 || excludeDir.length > 0;

    if (!pattern && !path && !hasFilters) return [];

    if (hasFilters) {
      return [
        `No matches in ${path ?? 'this scope'} with active filters.`,
        'Remove a filter or try a shorter/partial term.',
      ];
    }

    const out: string[] = [
      `No matches for "${pattern}" in ${path ?? 'this scope'}.`,
      'Broaden: try a shorter or partial term, use fixedString=true for a literal match, or search a parent directory.',
    ];

    if (mode !== 'discovery') {
      out.push(
        'Use mode="discovery" to check for file presence before a full search.'
      );
    }

    return out;
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const count = ctx.matchCount ? ` (${ctx.matchCount} matches)` : '';
      return [
        `Too many results${count} — narrow the pattern, add a filter, or use fixedString=true.`,
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
